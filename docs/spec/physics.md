# Physics: the locked model, its constants, and the fitting ladder

Status: spec section, locked at #7 (2026-09-11) and amended by the prototype's gate at #8; merged into
the assembled spec by #12. The observation contract it must satisfy is `rules.md` §10; the crate that
holds it is `architecture.md` §1; the data and curves it is fitted against are `benchmark-data.md` and
`data/curves/`. Every coefficient is labelled *measured* / *fitted* / *carried* / *not identifiable*,
and every deliberate simplification is listed with its direction of error (§10).

## Scope and sources

An analytic event-driven core with **full 3D ball state**, rendered top-down, fitted against published
measured curves and then replayed against a real pro corpus. The literature this model comes from, and
the two facts a reader must know before trusting a number in it:

- **No standard benchmark exists for pool physics.** There is no KITTI-style fitting set; the ladder
  below is this spec's construction, and its stages are gated against the sources' own scatter.
- **Table condition moves every interaction coefficient.** The numbers in §5 are a *default profile*,
  not claims of universality; a fit is only reproducible with the condition metadata of §5's profile
  record.

| Source | Used here for |
|---|---|
| Coriolis, *Théorie mathématique des effets du jeu de billard* (1835); Alciatore's AJP review | the 90°/30° rules, the cloth/ball model shape |
| Marlow, *The Physics of Pocket Billiards* (1995) | μr's measured magnitude; the pooltool theory series is explicitly Marlow-based |
| Leckie & Greenspan 2006 | the event-driven analytic evolution this core is built on |
| Alciatore, TP A-5/A-6/A-14/A-28/A-30/A-31 and TP B-2/B-3/B-5/B-6/B-8/B-28 | the locked sub-model formulations and the ladder's gates (digitized in `data/curves/`) |
| Mathavan, Jackson & Parkin 2010 (cushion) and 2014 (ball–ball) | the named deferred upgrades, and the trap of §3.3 |
| WPA *Equipment Specifications* §5–§16 | the fixed geometry and equipment block of §5 |
| Cross 2008; Shepard; Platinum's robot table | cue endmass/squirt and the pivot-length gate |

The model shortlist, the tradeoff tables, and the per-constant provenance are the asset on #3; the
prototype's measurements and its gate rulings are on #8, with the artifacts indexed in
`prototypes/README.md`.

## 1. Core architecture

**State advances event to event**, never on a fixed step.

- Each step solves the next event time across every candidate (ball–ball, ball–cushion, ball–jaw,
  ball–pocket drop, motion-mode transitions), takes the minimum, applies the impulse or transition, and
  repeats. Fixed stepping is disqualified by contact duration — tip–ball ≈ 2 ms, a hard impact ≈ 2.8 ms —
  which a break-speed ball crosses in well under one tick.
- **Simultaneity:** events within ε of the earliest form one *group*; impulses are applied in a
  documented canonical order (contact type, then ball ids) and the group is re-checked until it clears.
  The group id is reported — the rules layer's legality-favouring tie-break operates on groups, never on
  the simulator's internal order.
- **Depenetration step (required).** A simultaneity group can leave two balls overlapping after a
  sequential impulse pass, and a perfect rack can stall on it. After a group clears, the solver runs a
  **position-only separation step for already-separating pairs** (push apart along the line of centres,
  to a gap of exactly 2R), and reports it as a fact. Without it the rack's exact-contact lattice is not
  simulable: the prototype's stall guard fired before the step existed.
- **Stall guard (required).** A candidate event rejected on application (a ball sliding along a cushion
  at exactly R) re-appears at t = 0 forever. The solver must require **approach** before proposing a
  contact, and the loop needs a **no-progress counter**. Both are spec requirements, not prototype warts.
- **Rest criterion:** a shot ends when linear and angular speed fall below the fixed sleep thresholds
  (§5), then the state snaps to rest. An analytic solver chases micro-motion forever without this.
- **No RNG in the sim.** Randomness lives in the shell: the rack seed is an input, and execution-noise
  models belong to the AI/UX layer.
- **Facade:** `state_at(t)` is exact for any t — implemented as a **segment timeline of per-ball analytic
  laws** — so no interpolation exists anywhere in the design. A shot is driven as: rack + placements +
  strike declaration → advance to rest → publish the fact stream and the rest block.

## 2. Geometry and frame

SI millimetres; origin at the centre of the playing surface; x along the long axis (head → foot
positive), y across, z up, right-handed. Playing surface 2540 × 1270 mm; head string at x = −635 mm; foot
spot at x = +635 mm; the long string is the x-axis. Ball radius 28.575 mm, mass 170 g, inertia ⅖mR². Ball
state is position, linear velocity, and full angular velocity. The rack's closed-form slot geometry is
`rules-break.md` §2.

## 3. Sub-models

### 3.1 Ball–cloth

Piecewise Coulomb sliding (quadratic contact-point decay) + linear rolling resistance + constant spin
decay, with sliding → rolling → spinning → stationary solved as analytic mode transitions. Deferred
upgrade: a unified torque-based rolling model (it breaks the closed form). μs and μr are **not
identifiable from the curves in hand** (§12).

### 3.2 Ball–ball

Impulse-based frictional inelastic (Alciatore TP A-5/A-6/A-14) with a speed-dependent μb — the source of
throw. Deferred upgrade: Mathavan 2014 (table coupling + spin, high-speed validated) behind the same
interface.

- **μb ships as a piecewise-linear table**, not as `a + b·e^(−cv)`: the numeric discipline bans
  transcendentals from the core (`architecture.md` §3). The committed table's shape is a 25 mm/s step
  with a maximum **absolute** error of 1.0e−5 — measured at M0 as 9.84e−6 over a 1 mm/s sweep. (The
  *relative* error reaches 8.4e−5 at the fast end, where μb is smallest — μb(12 m/s) = 0.00995 — so the
  bound is absolute, and it is far below any throw measurement's resolution either way.)
- The published **0.03–0.08 band is provisional and marked not portable** until the stick/slip
  parametrisation is pinned: the refit to TP B-3's six measured points wants 0.148 at 1 m/s, five times
  outside the band, because the fit absorbs this formulation's error (§12).
- **Airborne:** ball–ball contacts at equal heights have a horizontal normal, so they add **no vertical
  impulse**. That is why the off-table corpus rows need a jump mechanism with its own formulation
  (elevated cue at speed + airborne ball–cushion geometry), and the spec says so rather than implying a
  lever exists.
- **Throw sign convention (pinned).** `throw_deg` is the signed angle from the **impact line of centres**
  (cue-ball centre → object-ball centre at the contact instant) to the **object ball's departure
  direction**, positive when the rotation is **clockwise viewed from above** in the table frame — i.e.
  positive when `ẑ · (n̂ × v̂_OB) < 0`. With this convention a **no-english cut gives a negative**
  `throw_deg`: the object ball departs toward the cue ball's incoming line, away from the tangent line.
  Every published throw table is unsigned, so the ladder compares `|throw_deg|`; without this paragraph a
  correct implementation looks like a sign error.

### 3.3 Cushion

Impulse frictional-inelastic with `e_n` and `μc`, **nose height (63.5 % of ball diameter) in the contact
normal** — the tilted normal gives every cushion contact a downward impulse, which is what makes the
vertical channel (§3.4) entailed rather than optional. Deferred upgrades: Han 2005, Mathavan 2010
rigid-cushion, Stronge compliant.

- **`e_n` is the normal-channel coefficient, not an effective rail COR.** The two differ by 0.06 here:
  the horizontal COR from the normal channel alone is `|n_x|²(1+e_n) − 1` = 0.6224 at `e_n` = 0.75, while
  the measured retention with the tangential and vertical channels is 0.6714. TP B-6's measured
  `e_c` = 0.70 corresponds to **`e_n` ≈ 0.78**, which is the default; the range is 0.75–0.85, and the fit
  is insensitive to the tangential ruling (measured).
- **The tangential channel is 3D (ruled).** The contact-point velocity is used in full — including the
  roll-induced vertical component — so the tangential impulse does vertical work whenever the ball has
  roll. The 2D projection was tried and rejected; it is recorded with its numbers so it is not
  re-attempted blind, and the falsifier is the check-up measurement of §12. Consequence: the rail-induced
  hop is fed by **both** the tilted normal and this channel, so it depends on `e_slate` *and* `μc`
  together and is pinned only by a check-up measurement, not by the drop test alone.
- **Travel shortfall is structural, not a coefficient problem.** At the ruled constants, TP B-6's five
  travel anchors are a reported residual (mean |err| **0.337** table lengths; 12 and 20 mph at +0.001 and
  +0.008, the shortfall concentrated at the three slow anchors) and not a gate: `e_n` trades retention
  for travel (raising it 0.75 → 0.85 lifts a 4 m/s stroke from 2.14 to 2.92 lengths but pushes the
  retention from 0.6714 to 0.7641, past the measured 0.70), and `μc` is flat (0.537–0.550) under the
  ruled channel.
- **Trap:** never copy Mathavan's model-internal `e_n` = 0.98 / μ = 0.14 as an effective rail COR; it is
  a normal restitution inside their rigid-cushion model, not a rebound. TP B-6's measured `e_c` = 0.7 is
  the usable rail figure.

### 3.4 The vertical channel and the ceiling rule

The tilted normal's downward component is absorbed at the cloth/slate with **`e_slate`** — a named,
profile-settable coefficient — and the ball hops. It is not suppressed, because suppressing it would
discard the impulse's z-component by fiat while keeping the geometry that produces it, and #7's airborne
row would become dead text.

- **`e_slate` is the most sensitive un-identified number in the model.** Sweeping it alone over
  0.0 / 0.6 / 0.9 moves the break's hop from 1.3 mm to 124.3 mm, its event count 198 → 7,928 (40×), and
  **changes which balls are pocketed on the break**. It is pinned by the **ball-drop test** of §12, and
  every rail result reports the `e_slate` it ran at.
- **Ceiling rule.** A ball clears a cushion only when its underside passes the cushion top: centre
  height > R + nose height = **64.9 mm** (i.e. the centre must rise 36.3 mm above its resting height).
- A **level-cue break does not off-table a ball at the default** `e_slate` (measured hop 15.79 mm against
  the 36.3 mm clearance). The three off-table corpus rows are therefore **conditional on the pinned
  `e_slate` and the tangential ruling**, not properties of the model (§8).

### 3.5 Pockets and the drop predicate

Mouths as geometry with cut-angle jaws as colliders.

1. **Drop region.** Two 2D region shapes, one per pocket class: **corner = the chord between the two jaw
   tips** (the diagonal chord); **side = the mouth disc** on the cushion line, radius from the pinned
   side-mouth width. The region is the pocket's mouth, not the rail line.
2. **Drop predicate.** A ball is pocketed when its **centre crosses into the mouth region** — root-found
   on the current analytic segment, **not sampled**. The root must satisfy three conditions, all
   required: (a) the crossing is **inward** (a separating solution is rejected), (b) the boundary is
   actually reached, `|f(pos_root)| ≤ 1 mm`, and (c) the crossing point is inside the mouth's lateral
   extent (the jaw-tip window). The boundary tolerance and the approach guard are what make the predicate
   usable: without them the prototype pocketed balls at rest in the rack, and a hard break pocketed 4–7
   balls instead of none.
3. **This replaces #7's phrasing.** "Pocketed = comes to rest below the playing surface (2.2)" is not a
   state this model can represent — there is no shelf, no slate edge, no 3D pocket volume. The
   mouth-crossing predicate *is* the model's definition, recorded as a deviation.
4. **Jaw tips.** Sharp point colliders with straight jaw faces at the pinned cut angles (142° / 104°).
   The jaw-tip radius and jaw-face compliance stay deferred (they are free parameters in reality, and
   sharp points already produce rattle); this is stated, not silent.
5. **Supported-over-mouth annotation — in scope.** Per `CONTEXT.md`'s *Pocketed* definition, a ball
   resting on another ball over a mouth such that removing the support would drop it counts as pocketed.
   The simulation leaves the ball in place (the cloth has no hole) and reports `supported_over_mouth`
   with its supporting ball ids as a rest-state fact; the rules layer counts it as pocketed. This is the
   only route by which "pocketed" arises without a drop event (`rules.md` §10.7).

### 3.6 Cue → ball

Effective endmass / pivot-length model: **one fitted scalar per cue (the pivot length)**, plus the miscue
envelope. Flexible-shaft modelling is deferred forever.

- **Squirt ships as one scalar** with the rotation evaluated algebraically (`sin α = a / √(a² + L²)`), no
  transcendental.
- **The reference tip offset is pinned: `a` = 8.07 mm ± 0.10** (0.282 R — quoted as "0.28 R" when rounded to two decimals; the 0.07 mm difference from 0.28 R exactly is inside the fit's own scatter) — the only value at which Platinum's 46
  shafts are self-consistent under `tan α = a / L` (sd 0.10 mm). Comparing the published angle column
  against 0.5 R offsets gives 4.2°, outside the published band; the ladder must not do it.
- **The miscue envelope is a fixed cone**, `ρ_max = R·μ/√(1+μ²)` = 14.70 mm at μ = 0.6, at every
  elevation. Offsets past it are an **input error, not a simulated event** — rejected at the input
  boundary by both the human and the policy path (`ux-cue.md` §6, `ai.md` §2). Elevation-dependent
  widening is deferred with its direction of error recorded: a downward offset gains, an upward one
  loses, and extreme elevation adds a shaft-clearance limit.

### 3.7 Airborne

Ballistic with gravity only — **drag and Magnus neglected, stated as such** — landing restitution to
cloth/slate, rolling resumes on landing (the landing friction impulse is deferred). 3D ball–ball
detection lets a jumped ball strike in the air. A jump needs its own formulation (§3.2): an elevated cue
at speed plus airborne ball–cushion geometry.

## 4. Cue input model

A strike is `{aim direction, cue-ball launch speed, spin offsets (a, b), elevation}` — post-impact
cue-ball state, because no stick is simulated (tip restitution is therefore not a constant). The offsets
are **fractions of the miscue envelope**, dimensionless, `1.0` = the limit, with `a > 0` the shooter's
right and `b > 0` above centre; the contract is the `spin` description in `input-log.schema.json`, and
the sim converts once at the boundary (`offset_mm = ρ_max · value`). This declaration is the AI's action
space and the UX's authored object; both are bounded by the same envelope.

## 5. Constants

**Fixed geometry and equipment** (spec constants; sources in the right-hand column):

| Constant | Default | Source |
|---|---|---|
| Playing surface | 2540 × 1270 mm (9 ft) | WPA Equipment §5 |
| Ball radius / mass / inertia | 28.575 mm / 170 g / ⅖mR² | WPA §16 (2.25″ +0.005″, 5.5–6 oz; 170 g is the nominal-6 oz end), Dr. Dave |
| Cushion nose height | 63.5 % of ball diameter (±1 %) | WPA §7 |
| Pocket mouths (corner / side) | 115.9 mm / 128.6 mm | WPA §9 (mid of 4.5–4.625″ and 5–5.125″) |
| Pocket cut angles | 142° / 104° (±1°) | WPA §9 |
| Gravity | 9.80665 m/s² | — |
| Tip–ball friction μ | 0.6 | Dr. Dave, Cross; defines the miscue envelope |
| Simultaneity window ε | 1 × 10⁻⁹ s | prototype |
| Sleep thresholds | 1 mm/s linear, 0.01 rad/s angular | **measured** by the prototype: across 0.01 → 1 mm/s the break's rest positions move by ≤ 0.0001 mm and one micro-event disappears; 10 mm/s already moves a ball 7.33 mm, and 50 mm/s changes the shot — the pin is the middle of a plateau |
| Frozen tolerance | surface gap ≤ 0.5 mm, one constant for rail and ball frozen status | #8's fact-vocabulary ruling (`rules.md` §10.2) |
| Rack lattice | exact contact, 2R; `√3` as the literal `1.7320508075688772` | `rules-break.md` §2 |

**Profile-settable interaction coefficients** (a *profile* is a named record: values, provenance, and
condition metadata — cloth speed, humidity, ball set — so every fit is reproducible; the shipped default
profile is one such record, and these are defaults, not claims of universality):

| Constant | Default | Range | Label |
|---|---|---|---|
| Cloth sliding μs | 0.20 | 0.15–0.4 | **not identifiable from the curves in hand** (§12) |
| Cloth rolling μr | 0.010 | 0.005–0.015 | **not identifiable from the curves in hand** (§12) |
| Spin (turntable) decay | 10 rad/s² | 5–15 | consistent / untested |
| Ball–ball restitution e_b | 0.95 | 0.92–0.98 | consistent / untested |
| Ball–ball friction μb(v) | table, 25 mm/s step, max **abs.** error 9.84e−6 (rel. 8.4e−5 at the fast end) | band 0.03–0.08 **provisional, not portable** | **carried**, not fitted (§12) |
| Cushion `e_n` | **0.78** | 0.75–0.85 | **fitted**: TP B-6's measured retention `e_c` = 0.70 (0.75 → 0.6714, 0.80 → 0.7178) |
| Cushion friction μc | 0.20 | — | **weakly identified**: bounded below, never pinned in value; whole retention share +0.0490 over the closed form 0.6224; sticking from ≈0.05 (retention) / ≈0.10 (travel, rejected 2D) / ≈0.15 (ruled 3D) |
| Vertical recovery `e_slate` | 0.60 | **no measured band** | **carried**, not fitted: pinned by the ball-drop test (§12); every rail result reports the value it ran at |

## 6. Fitting and validation ladder

Five stages, each isolating one parameter group, then the break hook. **Stage 5's end-to-end corpus and
stages 2–4 are curve-gated only**: no open dataset in hand provides spin, cue speed, or cue
identification (`benchmark-data.md` §6).

| Stage | Observable | Gate as amended | Basis |
|---|---|---|---|
| 1. Long straight stop / draw / follow | draw–follow distances, tangent-line persistence | **Pure-spin branch** (exact, 0.012 % vs TP B-8) **plus the TP B-5 rolling direct-hit ratio** (7.781 vs the file's re-derived 7.559). The full-row comparison is **reported, not gated**: TP B-8's relation is a free-space spin-only upper bound, 85 of its 120 digitized rows are outside its own domain, and the ~50 % mean error on the 35 in-domain rows is structural (−26 % spin damping + ~2.5 % forward residual in the collision) | measured; the relation must not be read as a simulation error |
| 2. Cuts at known angles | throw vs cut angle and speed | TP B-3's six measured points, with the **μb stick/slip parametrisation pinned first** and the **sign convention stated** (§3.2); compare magnitudes. Max throw ≈ 5°, zero at gearing | the shipped μb table's six-point residual is **4.167°** — the structural offset the stick/slip re-pin is expected to remove, not a bug; the refit (a, b, c = 0.0010 / 0.2732 / 0.620) takes it to **0.908°**, and the gate is only meaningful once the formulation is fixed |
| 3. Cushion | retention and rail travel | **Gate on** the single square-hit retention (the `e_n` fit, 0.70 ± the source's scatter) **and** TP B-6's travel anchors **as a reported residual with its measured structural cause** (mean \|err\| 0.337 at the ruled constants). **The WPA 4–4.5-table-length acceptance test is dropped**: at the measured coefficients it needs ≈9 m/s (a 4 m/s stroke runs 2.14 lengths; no `(μr, e_n)` inside the measured ranges reaches 4.0) — a gate the constants cannot pass is worse than no gate. The **bank grid is not gated**: TP B-28 records aim and through-diamonds only, with no start position and no speed | measured; see §12 for what reopens the anchors |
| 4. Pivot-length test per cue | squirt angle | Platinum's band **at the reference tip offset (8.07 mm ≈ 0.28 R)** (prototype 1.29–2.39°). The published bands do not agree (Shepard 0.5–2.3 vs Platinum's quoted 1.3–2.3; the table's own computed band at that offset is 1.34–2.31), and a comparison at 0.5 R offsets is void (4.23°, above both) | measured |
| 5. Full-shot replay | outcome agreement; rest-position deviation | **Zhang 9-ball pro set**: outcome agreement ≥ 90 %; rest-position deviation median ≤ 25 mm / p90 ≤ 100 mm kept as a **simulator-vs-simulator reproducibility bound**, and every claimed data fit must **state the input precision it recovered**. The prototype maps 25 mm to ≈±0.03° of aim or ≈±3 % of speed — both on a smooth two-ball cut only; on the break, 0.05° of aim error already moves rest positions by 592 mm median. Zhang supplies neither speed nor spin, so the Zhang gate is not a rest-position gate until the input precision is recovered | measured; the corpus is 180 rounds / 1,006 tracked shots, not the paper's 2,082 (`benchmark-data.md` §1) |

**Before any real-data fitting**, the differential stage runs against **pooltool** on synthetic shots
(`benchmark-data.md` §4). It caught the cushion-friction dimension once already; its own defaults differ
from the spec's profile (7 ft table, `e_c` 0.85, `u_b` 0.05, `g` 9.81), so the spec's values must be
pushed in explicitly for a meaningful comparison.

**Break acceptance hook (keep — the only gate that exercises the whole model).** Stable rack propagation
(no tunneling, bounded energy error), a reproducible ≥ 4-distinct-object-balls-to-rails predicate, and
the rest state. It must (a) apply the vertical-channel ceiling rule (§3.4), (b) **report the `e_slate` it
ran at** — its rail counts, event count and pocketed set move with it — and (c) note that a level-cue
break does not off-table a ball at the default. The prototype's instance: 5.556 s to rest, 2,154 events
in 2,125 simultaneity groups, KE 3.27 J → 0 J at rest, the largest relative energy rise between
consecutive timeline segments 0.000 %, maximum cushion penetration 0.000 mm, minimum ball–ball gap
12.72 mm, 12 distinct object balls reaching rails, and a bit-identical rerun. **M1's instance**
(fixture seed 1, cue (−800, 0), 6200 mm/s level, the committed profile): 7.472 s to rest, 1,372 events
in 1,333 groups, penetration 0.000000000 mm, max hop 5.08 mm, 16 rail contacts over **11 distinct object
balls**, nothing pocketed, no off-table ball, and a bit-identical rerun — the numbers move with the
profile and the rack, which is why every rail result reports the `e_slate` it ran at.

## 7. The fact stream

The simulation reports facts and never knows about fouls, groups, or turns. The container and stamps are
`architecture.md` §4.5; the kinds, their fields, and the count semantics are below, and `rules.md` §10
states what the rules layer needs from them:

- **Contacts** — ball–ball, ball–cushion, ball–jaw, each with participants and the simultaneity group.
  Every **rail contact carries the pair** `frozen_at_shot_start` / `left_since_shot_start` as first-class
  fields (`rules.md` §10.2).
- **Pocket** events carrying the pocket's identity, **off-table** events, and **depenetration** events.
- **Motion-mode transitions** (sliding/rolling/spinning/stationary/airborne) so a consumer can see the
  mode history without re-deriving it.
- **The rest block** — final per-ball state and modes, the pocketed and off-table sets, the frozen
  annotations, and `supported_over_mouth` for balls at rest inside a mouth without a drop.

**Count semantics (state this, or the corpora will be read wrongly).** The simulation's
`distinct_object_balls_to_rails` is the **physical** rail-contact count: distinct object balls that
actually touched a rail, **excluding** balls pocketed or driven off the table. Rule 2.7 (and 4.3(d) on
the break) *adds* pocketed and off-table balls to that count. The rules layer is the only place the two
are combined, and any corpus field or report must say which of the two it is.

## 8. The break corpus: dispositions

`physics-break.json` holds the physics tier's 14 requirement rows (`rules-break.md` §"Corpus contract").
The prototype's gate produced the parameters for 8 of them and the dispositions below; the rows stay
requirements — **the pattern is the requirement, the parameters are the pin**, and a re-aim that
reproduces the pattern is acceptable.

**Re-pinned at M1.** The parameters in the table below are the prototype's instance, minted on
phantom-era physics: its ball–ball root solver proposed contacts between balls a metre apart (a crossed
Newton step clamped to `t = 1e-9`), and those impulses contributed rail counts. On the repaired core the
prototype's parameters no longer reproduce their rows, so `physics-break.json` carries **M1's re-aims**
(the corpus's `generator.derivation` records them; the targets are unchanged, so `corpus_version` does
not bump). All eight produced rows now reproduce their `target_facts.expected`, and the six rows §8
records as unproduced keep null parameters and their dispositions.

| Row | Produced | Pinned parameters (prototype) | Note |
|---|---|---|---|
| pb-01 four to rails | **yes** | aim −2.000°, 1400 mm/s, level, no spin | 4 distinct / 4 contacts / nothing pocketed / legal 4.3(d) |
| pb-02 four distinct, five contacts | **yes** | aim −1.000°, 1400 mm/s | 4 / 5 / nothing pocketed / legal 4.3(d) |
| pb-03 pocketed ball + three to rails | no | — | the pattern is reached with ball **3** pocketed (3/3, 4.3(c)); the row pins ball 12 — **amend the row's ball number or re-pin it** |
| pb-04 total miss | **yes** | aim −4.721°, 550 mm/s | 0 contacts / illegal break |
| pb-05 three to rails, illegal | **yes** | aim −1.679°, 2400 mm/s | 3 / 3 / illegal-break tree |
| pb-06 three to rails + scratch | **yes** | aim +1.821°, 2400 mm/s | 3 / 3 / cue ball pocketed / illegal-break tree |
| pb-07 off-table object ball | no | — | no off-table event exists at the default `e_slate`; **conditional** (§3.4) |
| pb-08 legal break + scratch | **yes** | aim +1.000°, 2700 mm/s | 4 / 4 / cue ball pocketed / break-foul tree |
| pb-09 off-table object ball | no | — | as pb-07 |
| pb-10 8 on the break, no rails | no | — | the **8 is pocketed with the right tree**, but 11 distinct balls reach rails, not 0 — **the row is over-constrained**: "zero rail contacts" fights the rack and adds nothing to the rule under test |
| pb-11 8 on a foul break | no | — | no run with the 8 pocketed *and* the cue ball pocketed *and* zero rails — **over-constrained as written** |
| pb-12 8 off the table | no | — | needs an off-table event; **conditional** (§3.4) |
| pb-13 frozen ball leaves and returns | **yes** | aim −1.007°, 3100 mm/s; frozen pin: ball 4 from slot 5.0 to (700, 606.4) on the right long cushion | 3 / 3 / frozen contact suppressed / `left_and_returned` false — this row is what made the rail-contact field pair (`rules.md` §10.2) a requirement |
| pb-14 one ball, two rails | **yes** | aim −3.282°, 3100 mm/s | 3 distinct / 4 contacts / illegal break |

Reachability of the three off-table rows is **reported per ruling**: with the committed 3D tangential
channel at `e_slate` = 0.9 the level-cue break sends ball 10 off the table (hop 124.31 mm against the
36.3 mm clearance); under the rejected 2D projection the same `e_slate` produces a 167.87 mm hop and no
off-table event. The rows are conditional on the pinned `e_slate` and on the tangential ruling, and the
spec records both.

## 9. Physics testing strategy

The map's fog item graduates here. The fitting harness and the physics goldens run **locally/offline
with committed results** — the data lives outside the repository (`benchmark-data.md`) — while the
deterministic-replay goldens ride in `cargo test` and CI (`architecture.md` §11).

1. **Golden-shot corpus** — `goldens.json` (`architecture.md` §11), first entries pinned here: the four
   pocket-drop cases (a chord-crossing pot, a disc-entry pot, a near-parallel approach that must *not*
   drop, and a ball at rest in the rack that must not drop — the last two are the prototype's own two
   silent-pocket failures), the frozen-ball rail-contact case (pb-13), and the break hook's rest state.
   Each entry carries its declaration, its expected fact stream, and its expected rest-state hash. The
   hashes are pinned by the implementation at M1 (`README.md` §5): the prototype's numbers are a first
   measured instance, not the spec's values, because the prototype is a different implementation.
2. **Property tests** (the invariants the prototype measured at 0.000 / 0.00000): no cushion
   penetration, minimum ball–ball gap ≥ 0, monotone energy dissipation between timeline segments (max
   rise 0.000 % on the committed break), bit-identical rerun, and the sleep-threshold plateau.
3. **CI: no fitting.** The repo's `ci` check is a checkout-only placeholder by design (ADR 0001); the
   fitting harness and the dataset never enter CI. Wiring real CI is a separate effort, outside this
   spec's scope.

## 10. Out of scope, with reasons

| Deferred | Why |
|---|---|
| Cushion compliance beyond a fitted effective COR (Han 2005, Mathavan 2010, Stronge) | a fitted COR already reproduces the measured retention; the compliant models are a later upgrade behind the same interface |
| Cloth compression/bunching | below the resolution of any measurement in hand |
| Tip–ball mechanics beyond endmass; tip restitution as a constant | no stick is simulated — the strike is post-impact cue-ball state |
| Chalk, cling and dirt beyond fitted constants; ball wear; aerodynamics (drag, Magnus) | stated simplifications with their direction of error |
| Mathavan 2014 ball–ball | deferred upgrade behind the same interface; the locked model is fitted to TP B-3 |
| Flexible-shaft cue modelling, tip-offset/elevation coupling, masse-specific spin transfer | the pivot scalar and the launch angle are enough for the measured squirt band |
| Elevation widening of the miscue envelope | no measurement identifies it; the fixed cone is a recorded **simplification** (not a WPA departure — `CONTEXT.md` reserves *Deviation* for rule text), with its direction of error and settling measurement in `ux-cue.md` §10.5 |
| The landing friction impulse | rolling resumes on landing; the impulse at the landing instant is omitted |
| Coupling of spin decay to the normal force | the constant decay applies uniformly whenever the ball touches cloth |
| Shelf/slate-edge pocket support, the below-the-surface predicate | superseded by the mouth predicate (§3.5); the annotation belongs to pocket geometry work |
| Jaw-tip radius and jaw-face compliance | free parameters in reality; sharp points already produce rattle |
| Stage 5 replay at scale | the corpus needs per-game orientation rotation, scale normalisation, and velocity inference from ~33 Hz tracks |
| Any spin-ignorant shortcut beyond a debug baseline | it would silently become the model |

## 11. The bar this section is held to

> A physics formulation is spec-ready when (a) **every coefficient is labelled** *measured* / *fitted* /
> *carried* with the curve or experiment that pins it named — or is marked *not identifiable* together
> with the measurement that would settle it; (b) **every ladder stage names its observable, its gate
> quantity, and the structural offset it is expected to show** — a stage whose gate the calibrated
> constants cannot pass is removed, not carried; (c) **every corpus row the model cannot produce is
> recorded as a model limitation with the mechanism it would need** (and, where a constant decides
> reachability, conditional on that constant); (d) **every deliberate simplification is listed with its
> reason and its direction of error**, so a later fit knows what it is absorbing.

By that bar this section carries three declared limitations (the WPA cushion test dropped; the B-6
travel anchors reported not gated; the off-table rows conditional) and two honest holes (`e_slate`,
`μc`) plus two un-identifiable rows (`μs`, `μr`) — each with its settling measurement below.

## 12. Provisional, with the measurement that settles each

| Item | State | Settling measurement |
|---|---|---|
| `e_slate` (vertical recovery) | carried at 0.60; no measured band — the model-side drop test (M1) reproduces the constant to rounding, but that verifies the model, not the world | **ball-drop test** on the slate: `e = √(h_rebound / h_drop)` — one phone camera and a metre rule |
| `μc` (cushion friction) | bounded below, not pinned: 0.20 sits above the measured knee and is inert there | rail-interaction measurement (speed *and* spin either side of a cushion), or a check-up measurement |
| `μs`, `μr` | not identifiable from the curves in hand: the pure-spin branch is exact at any value, and the draw rows move 52 % → 79 % → 177 % across the survey's own corners | a draw/follow measurement with the **input** pinned (speed known), not the relation |
| `μb(v)` band | not portable | pin the stick/slip parametrisation, then refit against TP B-3 |
| TP B-6 travel anchors | reported residual 0.337 table lengths at the ruled constants (0.554 at the range top) | re-derive the anchors with the stroke speed pinned, or accept the structural shortfall explicitly |
| The cushion tangential channel | ruled 3D; the rail hop couples `μc` and `e_slate` | the check-up measurement: a rolling ball's post-cushion speed *and* spin at low speed |
| Bank grid (TP B-28) | not gated | a re-derivation that pins start position and speed per row |
| Stage 5 input precision | bound stated, not met | velocity inference from Zhang's ~33 Hz tracks (+ spin/speed from a future annotated set) |
