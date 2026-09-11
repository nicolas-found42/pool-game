# Prototype results — ticket #8

Everything below was produced by the commands in `README.md` on the campaign
machine (Apple M5, macOS 26.4, arm64, Rust 1.98.1). The measurement dumps are
committed under `results/`; the frames under `shots/`.

**Answer in one paragraph.** The locked model *does* reproduce the measured
behaviour it can be checked against, and the fitting approach is tractable but
not as written: the cloth model matches the published draw relation to 0.012 %
on the branch where that relation is exact, the ball–ball model reproduces the
published TP B-5 rolling direct-hit ratio to 2.9 %, and the cushion reproduces
TP B-6's measured rail speed retention once the nose-height tilt is in the
contact normal (e_n ≈ 0.78, not the spec's 0.75). What does *not* hold up is
the ladder's gating: TP B-8's draw relation is a free-space, spin-only bound
(~50 % optimistic on real shots), the WPA 4–4.5-table-length cushion test is
unreachable with the measured cloth coefficients, the rest-position tolerance
of ≤ 25 mm/p90 ≤ 100 mm demands a strike input precision that no data in hand
supplies, and the prototype's vertical channel never lifts a ball over a rail,
so the corpus's off-table rows cannot be produced.

## 1. The 14 requirement rows (`docs/spec/physics-break.json`)

`rows --spec docs/spec/physics-break.json --budget 700` searches the free
parameters (aim about the apex line, speed, spin, elevation) for each row and
records the first parameter set whose fact pattern matches. The search is
deterministic: a fixed coarse grid, then a refinement around the four best
near-misses, then a spin/elevation pass.

**8 of 14 produced.** Full table in `results/rows.md` / `results/rows.json`.

| row | produced | pinned parameters (the row's nulls) | observed fact pattern |
|---|---|---|---|
| pb-01 four distinct balls to rails | **yes** | aim −2.000°, 1400 mm/s, no spin, level | 4 distinct / 4 contacts / nothing pocketed / legal clean, 4.3(d) |
| pb-02 four distinct, five contacts | **yes** | aim −1.000°, 1400 mm/s | 4 / 5 / nothing pocketed / legal clean, 4.3(d) |
| pb-03 pocketed ball + three to rails | no | — | closest run: 3 / 3 / pocketed **[3]** / legal, 4.3(c) — the pattern is reachable, but with object ball **3**, not the row's 12 |
| pb-04 total miss of the rack | **yes** | aim −4.721°, 550 mm/s | 0 / 0 / nothing pocketed / illegal break, 4.3(d), cue-ball contacts 0 |
| pb-05 three to rails, illegal | **yes** | aim −1.679°, 2400 mm/s | 3 / 3 / nothing pocketed / illegal break, 4.3(d) |
| pb-06 three to rails + scratch | **yes** | aim +1.821°, 2400 mm/s | 3 / 3 / nothing pocketed / cue ball pocketed / illegal-break tree, 4.3(d) |
| pb-07 three to rails + object ball off the table | no | — | 3 / 3 / nothing pocketed / **nothing off the table** — see §5 |
| pb-08 legal break + scratch | **yes** | aim +1.000°, 2700 mm/s | 4 / 4 / nothing pocketed / cue ball pocketed / break-foul tree, 4.3(h) |
| pb-09 legal break + object ball off the table | no | — | 4 / 4 / nothing pocketed / nothing off the table; closest run has the cue ball pocketed instead |
| pb-10 8 pocketed, nothing to rails | no | — | closest run: the **8 is pocketed**, tree `eight_on_break`, rule 4.3(e), legal — but 11 distinct balls reached rails, not 0 |
| pb-11 8 pocketed on a foul break | no | — | no run with the 8 pocketed *and* the cue ball pocketed *and* zero rail contacts |
| pb-12 8 driven off the table | no | — | needs an off-table event, unreachable (§5) |
| pb-13 frozen ball leaves and returns | **yes** | aim −1.007°, 3100 mm/s; frozen-ball pin: ball 4 moved from rack slot 5.0 to (700, 606.4) frozen to the right long cushion | 3 / 3 / nothing pocketed / illegal break; the frozen contact is suppressed, `frozen_ball_left_and_returned` false |
| pb-14 one ball touches two rails | **yes** | aim −3.282°, 3100 mm/s | 3 / 4 / nothing pocketed / illegal break, 4.3(d) |

Findings from the row work (all of them inputs to the spec):

- **The corpus's `distinct_object_balls_to_rails` is the *physical* rail-contact
  count.** pb-03 explicitly excludes its pocketed ball 12 from the three, pb-10
  reports 0 with the 8 pocketed, pb-12 reports 0 with the 8 driven off. Rules
  2.7 then *adds* pocketed and off-table balls to the count. The assembly ticket
  should state that split, or the field will be read as the 2.7 count and every
  pocketed/off-table row will be judged wrongly.
- **Two rows are over-constrained as written.** An 8-on-break row that also
  demands *zero* rail contacts (pb-10, pb-11) fights the rack: any break hard
  enough to move the 8 to a pocket spreads other balls to rails. The 8-on-break
  tree is decided by the 8 alone (precedence 1), so the zero-rail clause adds
  nothing to the rule under test and makes the row nearly unreachable. Row
  pb-03's pattern *is* reachable with a different object ball; pinning the ball
  *number* is a search-cost choice, not a physics statement.
- **The frozen-ball row needs a pre-state the corpus does not pin.** pb-13 says
  "the rack of arrangement D, plus one object ball frozen to a rail", but the
  arrangement maps all fifteen balls to slots. The prototype pinned it as: the
  ball in slot 5.0 leaves the rack and sits frozen to the right long cushion at
  x = 700 mm. The spec must either state this pre-state or hand it to the rack
  layer, because "frozen at shot start" is simulation truth (rules 2.7).
- **A rail contact is only counted when the ball actually meets the cushion.**
  The prototype suppresses a contact with a rail the ball was frozen to until
  the ball has separated (measured: `frozen_contacts_suppressed` in the row
  dump), which is what pb-13 requires.

## 2. Fitting ladder stages 1–4

### Stage 1 — draw / follow / tangent line (`results/stage1-draw-follow.txt`)

| Check | Result |
|---|---|
| Pure-spin branch (ball launched with backspin, no translation) vs TP B-8's closed form | **0.012 % relative error** at ω = −50, −100, −150, −200 rad/s (892.1 vs 892.0 mm … 14273.6 vs 14272.0 mm) |
| TP B-5 rolling direct-hit travel ratio (object ball ÷ cue ball after impact) | prototype **7.781**; the curve file's re-derived constant gives 7.559; TP B-5's printed ratio is 6.08 |
| The 120 draw-distance rows vs the relation | only **35 are inside the relation's domain** (pre-impact spin negative and free-space draw ≤ 2.5 m); on those the mean |error| is 461 mm / 50 % |
| Cloth sensitivity (μs, μr) | (0.20, 0.010) → 78.8 % mean relative error; (0.15, 0.005) → 52.0 %; (0.40, 0.015) → 176.6 % |

- The pure-spin agreement is the clean validation: on the branch where TP B-8
  is exact, the piecewise slide/roll/spin model is exact to 1 part in 8000.
- The 50 % gap on the full-shot rows is **structural, not a coefficient
  problem**: TP B-8 assumes the collision leaves the cue ball with (v ≈ 0,
  ω unchanged), while the impulse-frictional ball–ball model both damps the
  backspin (measured: 78 → 57.6 rad/s at the 12.7 m/s row, i.e. −26 %) and
  leaves a ~2.5 % forward residual. Draw distance scales with ω², so a 26 %
  spin loss is a ~45 % distance loss. The relation is an *upper bound*; the
  ladder's stage-1 gate must say so or the model will look "wrong" for being
  more physical than the relation.
- **The digitized curve file has a domain bug**: `draw_distance_m` is
  `2R²/(49g)(1/μs+1/μr)ω²`, unsigned, so rows whose pre-impact spin has gone
  positive (natural roll or better, which happens on the long-drag rows) report
  a *draw* distance where the physics is a follow. 85 of 120 rows are outside
  the relation's domain.
- The TP B-5 check is the one that decides who to trust: the prototype lands on
  the file's re-derived 7.559 (2.9 % high), not on the printed 6.08, which
  corroborates the note in `data/curves/draw-follow.json`.

### Stage 2 — throw vs cut angle and speed (`results/stage2-throw.txt`)

| Check | Result |
|---|---|
| 2,130 contour points (TP A-28, cut 5–75°, 0.5/1.5/4.5 m/s, roll 0/50 %) | mean |error| **1.413°**, max 10.76°; roll = 0 rows 1.706°, roll > 0 rows 1.120° |
| TP B-3's six measured points vs the file's own A-28 friction fit | mean |error| **4.167°** (the file's residual table agrees: max 2.88° on the softest 30° point) |
| Refit of μb(v) = a + b·e^(−c·v) to those six measured points | a = **0.0010**, b = **0.2732**, c = **0.620** → mean |error| **0.908°** |
| Prototype max |throw| over the grid | **4.41°** (published family: max throw ≈ 5°, zero at gearing) |
| Gearing | reproduced: the frictional impulse goes to zero at the curve file's gearing english (checked against `gearing_english_percent`) |

The fitted μb is the interesting number: at 1 m/s it evaluates to 0.148, five
times the spec's 0.03–0.08 band. **The fit is absorbing the prototype's
tangential-impulse formulation, not measuring the cloth.** Until the spec fixes
the stick/slip parametrisation (the cap is `μ·J_n` here, the curve family's is
`min(μ·v·cosφ/v_rel, 1/7)`), a fitted μb is not portable — this is trap #1 for
the assembly ticket.

**Sign convention trap.** The published tables (TP A-28 and the TP B-3
measurements) are all positive; the prototype's signed deviation for a
no-english cut is *negative* — the object ball departs closer to the cue ball's
incoming line, which is the physically correct cut-induced-throw direction
("away from the tangent line"). The comparison above is on magnitudes. The
ladder's stage-2 gate must state the convention, or two correct implementations
will disagree by a sign.

### Stage 3 — bank grid, cushion retention, the WPA acceptance stroke (`results/stage3-cushion.txt`)

| Check | Result |
|---|---|
| Single square cushion hit, speed retention at e_n = 0.75 | **0.6714** at 0.8, 1.5 and 3.0 m/s (speed-independent, as theory says) |
| Closed form from the nose-tilted normal alone, ignoring the friction and vertical channels | 0.6224 — i.e. the naive formula *under*-predicts, so `e_effective = n_x²(1+e_n) − 1` is a lower bound, not the answer |
| Retention fit to TP B-6's measured e_c = 0.70 | e_n = 0.75 → 0.6714; **e_n = 0.80 → 0.7178** → the fitted value is e_n ≈ **0.78** |
| TP B-6 anchored travel (printed 0.903 / 1.684 / 3.005 / 3.690 / 4.715 table lengths at 1.5 / 3 / 7 / 12 / 20 mph) | prototype 0.760 / 1.121 / 1.922 / 3.431 / 4.086 → **−16 %, −33 %, −36 %, −7 %, −13 %** |
| WPA acceptance stroke (firm centre-ball stroke from the head spot must run 4–4.5 table lengths) | needs ≈ **9 m/s**; a 4 m/s stroke runs 2.14 lengths |
| Scan for a profile that meets the WPA test at a firm 4 m/s | μr = 0.010, e_n = 0.85 → 2.92 lengths; μr = 0.005, e_n = 0.85 → 3.27; **no combination within the measured ranges reaches 4.0** |
| Bank grid (TP B-28's 30 measured rows) | cannot be simulated like-for-like: the file records aim/through diamonds only, with no start position and no speed — a gap the stage-3 gate must fill |

- The nose-height tilt is the spec's locked formulation and it does most of the
  work: without it the measured retention is exactly e_n = 0.75; with it, 0.75
  gives 0.6714 and matching the measured 0.70 needs e_n ≈ 0.78. **The spec's
  e_n range (0.6–0.9, default 0.75) should become 0.75–0.85 with default 0.78,
  or the table should state e_n is the normal-channel coefficient and the
  effective rail COR is not e_n.**
- The residual travel shortfall (−7 % to −36 %) is the cushion's tangential
  friction and the post-rebound skid: the TP B-6 algorithm applies a speed
  multiplier and then an idealised "skid back to roll", with no friction at the
  cushion contact. The prototype loses more at every rail. This is *not*
  fixable by e_n alone: raising e_n to fit the travel pushes the retention above
  the measured 0.70. **One coefficient cannot satisfy both the published rail
  speed retention and the published travel curve**; the stage-3 gate must say
  which one e_n is fitted to.
- **The WPA test as written is not meetable by a physically-calibrated
  profile.** With μr at TP B-2's measured 0.01 and a rail COR of 0.7, a stroke
  must be ~9 m/s (20 mph) to run 4–4.5 lengths; a "firm" stroke of 4 m/s runs
  2.1. Reaching 4.0 lengths at 4 m/s needs μr ≲ 0.005 or a rail retention above
  0.85, both outside the measured ranges. Either the test is being read with the
  wrong stroke speed, or it should be dropped from the ladder; the spec should
  not carry a gate that its own constants cannot pass.

### Stage 4 — squirt / pivot length (`results/stage4-squirt.txt`)

| Check | Result |
|---|---|
| Platinum's 46 shafts under the pivot definition tan α = a / L | self-consistent at an implied reference tip offset **a = 8.07 mm (0.28 R)**, sd 0.10 mm |
| Published squirt band | Platinum 1.34–2.31°, Shepard 0.5–2.3° |
| Prototype at that reference offset with the table's own pivots | **1.29–2.39°** — inside the union band |
| Prototype at 0.5 R (maximum english) | 2.29–4.23° — *above* the band, because the published angles were measured near 0.28 R |

The squirt model is one scalar per cue (the pivot length) and it reproduces the
only measured table in hand exactly by construction. Two things the spec must
fix: the **reference tip offset** (0.28 R here) has to be stated or the pivot
column and the squirt column cannot be compared, and the pivot↔endmass map
(Shepard) is the only route from a cue's mass to the scalar.

## 3. Fitted coefficient picture

| Coefficient | Spec default | What the data supports | Verdict |
|---|---|---|---|
| Cloth sliding μs | 0.20 | pure-spin branch: no constraint (exact at any value); the draw rows move 52 % → 79 % → 177 % across the survey's (μs, μr) corners | **not identifiable** from the curves; the relation is the wrong instrument |
| Cloth rolling μr | 0.010 | TP B-5 rolling ratio 7.781 vs 7.559 → within 3 % at the default | **consistent**, not sharply identified |
| Spin decay | 10 rad/s² | no curve tests it; it only ends micro-motion | not testable here |
| Ball–ball restitution e | 0.95 | TP B-5 ratio + throw contours | consistent, weakly identified |
| Ball–ball μb(v) | 0.06 nominal | refit to TP B-3's six points: a 0.0010 / b 0.2732 / c 0.620 (0.908° residual vs 4.167° for the published fit) — but the value lands 5× outside the spec's band | **not portable** without a pinned stick/slip parametrisation (trap #1) |
| Cushion e_n | 0.75 | measured retention 0.70 needs **e_n ≈ 0.78** | **fitted** (this ticket's contribution) |
| Cushion μc | 0.20 | binds only in the tangential channel; the retention fit above is with μc = 0.2 | weakly identified; the travel curve suggests it is too high |
| Cloth/slate restitution | 0.60 | governs the cushion pop (max hop 15.8 mm on the break) and landings | no curve tests it; it drives the vertical channel (trap #2) |

## 4. The two numbers #7 §6 deferred to this ticket

**Sleep thresholds — pinned to 1 mm/s and 0.01 rad/s** (`results/sleep-pinning.txt`).

Rest positions on the break, measured as the maximum deviation from the
strictest run (0.01 mm/s, 0.0001 rad/s):

| linear / angular | events | rest time | max rest-position deviation vs strictest |
|---|---|---|---|
| 0.01 mm/s / 0.0001 rad/s | 2155 | 5.5558 s | 0 (reference) |
| 0.1 mm/s / 0.001 rad/s | 2154 | 5.5558 s | 0.000003 mm |
| **1 mm/s / 0.01 rad/s** | 2154 | 5.5558 s | **0.000106 mm** |
| 10 mm/s / 0.1 rad/s | 2147 | 5.5558 s | 7.33 mm |
| 50 mm/s / 0.5 rad/s | 1944 | 6.3237 s | shot still moving (outcome changes, +14 % rest time) |

The thresholds sit in a flat band: from 0.01 to 1 mm/s the rest *positions* move
by 0.1 µm and one micro-event disappears; 10 mm/s already moves a ball 7.3 mm
and 50 mm/s changes the shot. Pin: **1 mm/s / 0.01 rad/s**, chosen as the middle
of the plateau rather than the strictest value — the evidence is that a 100×
stricter threshold buys nothing and costs an event.

**Rest-position tolerance — not supportable as spec'd** (`results/tolerance-pinning.txt`).

Deviation of every ball's rest position when the *declaration* moves by a
plausible input error, on a two-ball cut (a smooth shot):

| aim error | median rest deviation | speed error | median rest deviation |
|---|---|---|---|
| 0.0005° | 0.47 mm | 0.001 % | 0.008 mm |
| 0.005° | 4.77 mm | 0.1 % | 0.84 mm |
| 0.02° | 17.6 mm | 1 % | 8.37 mm |
| 0.05° | 37.2 mm | 2 % | — |

So a **25 mm median** rest tolerance corresponds to knowing the aim to about
**±0.03°** (or the speed to ±3 %). On the break — a chaotic many-body shot —
even 0.05° of aim error and 0.5 % of speed error move rest positions by 590 mm
(median) with a 1.3 m max. Consequences:

- The tolerance is a *reproducibility* bound, not a data-fit bound: two correct
  simulators agree to machine precision (the prototype's reruns are
  bit-identical), but a simulator can only match a *measured* rest position if
  its input is known to ~0.03°/3 %.
- The Zhang set supplies neither (no cue speed, no spin, ~33 Hz tracks). The
  end-to-end gate therefore cannot be read as "median ≤ 25 mm against Zhang";
  the spec should state the tolerance as the achievable bound *given* the
  precision of the input, and stage 5 should report the input precision it
  actually recovered.
- Pin for §6: keep **median ≤ 25 mm / p90 ≤ 100 mm** as the simulator-vs-
  simulator tolerance, and add the requirement that any claimed data fit states
  its recovered input precision.

## 5. Energy, tunnelling, the break hook, and what the prototype cannot do

Break acceptance hook (`break --out out`, full output in
`results/break-acceptance.txt`; rack = `docs/spec/rack-fixtures.json` seed 1,
cue ball at (−800, 0), 6.2 m/s, level, centre):

| Quantity | Value |
|---|---|
| Rest | 5.556 s, 2154 events in 2125 simultaneity groups |
| Energy | KE₀ = 3.27 J; KE at rest = 0 J (everything dissipated by cloth friction); the largest relative rise between consecutive timeline segments is **0.000 %** |
| Tunnelling | max penetration into any cushion face **0.00000 mm**; minimum ball–ball gap **12.72 mm** (> 0, no overlap) |
| Vertical channel | max ball-centre rise above the cloth **15.79 mm** |
| Rail contacts | 16 contacts over **12 distinct object balls** → the ≥ 4-balls-to-rails predicate holds |
| Reproducibility | rerun is **bit-identical** (position/velocity bits and rest time) |
| Runaway guards | none hit; the stall guard exists for zero-progress event groups |

- **No tunnelling by construction.** Contacts are solved, not sampled, so a
  break-speed ball cannot skip through a cushion or a ball. The residual
  penetration metric exists to catch sign/geometry mistakes, and reads exactly
  0.000 mm at every measurement point.
- **The vertical channel is a trap.** The nose-height normal gives every cushion
  contact a downward impulse; the prototype absorbs it with the pinned
  cloth/slate restitution (0.60) and lets the ball hop (15.8 mm at break
  speed). That is what keeps balls from sliding along cushions forever, but the
  spec has **no coefficient for the vertical recovery**: it needs either a
  fitted vertical absorption or an explicit statement that post-cushion
  vertical launch is suppressed. With the ceiling at the cushion's top height
  (36.3 mm), a 15.8 mm hop never clears the rail — which is why the corpus's
  three off-table rows cannot be produced (below).
- **The corpus's off-table rows (pb-07, pb-09, pb-12) are unreachable in this
  prototype.** A ball leaves the table only if its underside clears the cushion
  top (centre > R + 36.3 mm). The strongest hop measured anywhere in the search
  is 15.8 mm, and ball–ball contacts add no vertical impulse (the contact
  normal is horizontal at equal heights), so no parameter set in the searched
  box produced an off-table event. Reaching those rows needs an explicit jump
  mechanism (elevated cue at speed, with the airborne ball–cushion geometry
  defined) or the spec must accept that "driven off the table" is not reachable
  by a level-cue break. This is a *documented model limitation*, not a search
  failure: the search covered ±12° of aim at 0.5° steps and 800–7000 mm/s in the
  focused pass (1,231 samples on pb-03 alone).
- **The pocket model is the fragile part.** Dropping a ball requires its centre
  to cross the mouth chord between the jaw tips (corner: the diagonal chord;
  side: the mouth disc on the cushion line). The first version of this
  predicate silently pocketed balls at rest in the rack: a near-parallel
  approach made the boundary root-finder return a spurious solution. After the
  root verification (the boundary must be reached to within 1 mm) a hard break
  pockets nothing, where the wilder model pocketed 4–7. **Pocket capture is
  decided by the drop predicate, not by the dynamics** — the single most
  trap-prone interface in the model, and worth a golden-shot test in the spec's
  testing strategy.

## 6. Differential check against pooltool (ladder pre-stage)

`tools/pooltool_diff.py` (pooltool 0.6.0, 9 ft table built from
`PocketTableSpecs(l=2.540, w=1.270)`, the spec's coefficients pushed in
explicitly: u_s 0.20, u_r 0.010, u_b 0.06, e_b 0.95, e_c 0.75, f_c 0.20,
g 9.80665, m 0.170) against `diff --out out`. Both sides in `results/`.

| shot | pooltool: events / ball–ball / cushion / pocketed | prototype |
|---|---|---|
| straight stun 1.4 m/s | 14 / 2 / 1 / none | 116 / 1 / 1 / none |
| 30° cut, follow 0.5 R | 13 / 1 / 2 / ball 1 | 150 / 1 / 2 / ball 1 |
| rolling ball into a cushion | 9 / 0 / 2 / none | 41 / 0 / 1 / none |
| full rack break 6.2 m/s | 222 / 118 / 26 / ball 7 | 2154 / 52 / 16 / none |

- **The qualitative behaviours agree**: the light stun leaves the cue ball
  parked, the 30° cut pockets the same object ball, and the cut's object-ball
  departure angles agree to ~1°.
- **The rest positions do not agree** (99–1995 mm apart on the break) and the
  event counts differ by an order of magnitude: pooltool's stack fires far more
  ball–ball contacts, keeps more speed through the cushions and spreads the
  rack across the table; the prototype loses more energy (its rolling ball
  rebounds 1.36 m where pooltool's travels 2.28 m) and pockets nothing.
- The largest single cause is the cushion's tangential friction impulse. The
  prototype's contact-point velocity has a vertical (*z*) component whenever
  the ball has roll, so the tangential friction at a cushion does work the
  TP B-6 algorithm never models. That is physically defensible — it is the
  "check-up" a rolling ball gets off a cushion — but it makes the prototype's
  rail behaviour more dissipative than both TP B-6 and pooltool.
- **Verdict for the ladder**: the differential stage is doing its job. Before
  any real fitting the spec must state (a) whether the cushion's tangential
  friction is two-dimensional or three-dimensional, and (b) how the vertical
  channel is absorbed, because those two choices move a break's rail count and
  travel by tens of percent and neither is in the #7 constants table.

## 7. Simplifications versus #7 (the cut list)

Kept from #7 as locked: event-driven analytic core; exact `state_at(t)`; full
3D ball state; piecewise cloth modes with analytic transitions; impulse
frictional ball–ball with speed-dependent μb; cushion impulse with the nose
height in the contact normal; mouths as geometry with jaw colliders; endmass
cue model with the miscue envelope; airborne ballistics; SI-mm frame; no RNG;
simultaneity groups with canonical order.

Simplified or omitted, with the consequence:

| # | Simplification | Consequence |
|---|---|---|
| 1 | Pockets: capture when the centre crosses the mouth chord (corner) / enters the mouth disc (side); no shelf, no slate-edge support, no "comes to rest below the surface" predicate, no supported-over-mouth annotation | the drop predicate dominates the pocketed count (see §5); the annotation #7 §3 asks for is absent |
| 2 | Jaw tips are sharp point colliders; jaw faces are straight segments at the pinned cut angles | jaw rattle is plausible but the tip radius is a free parameter in reality |
| 3 | μb(v) ships as a 25 mm/s piecewise-linear table fitted to the exponential (max error 1.0e−5) | keeps #11's no-transcendental rule; the spec must ship the table, not the `exp` |
| 4 | Squirt from a pivot-length scalar with the rotation evaluated algebraically; no flexible-shaft model | matches the published table by construction; the reference tip offset must be pinned (0.27–0.29 R) |
| 5 | Miscue envelope is the 0.5 R circle; elevation does not widen it | #7 §4 allows the elevation enlargement; not modelled |
| 6 | Elevation produces the launch angle only (no tip-offset/elevation coupling, no masse-specific side-spin transfer) | massé is only qualitatively reachable |
| 7 | Ball–ball contact normal is horizontal for balls at equal height | no vertical impulse from ball–ball contact — this is what makes the off-table rows unreachable |
| 8 | Airborne balls skip cushions/jaws/pockets when the centre is above the cushion top; landing ignores the friction impulse at the landing instant | #7 says "rolling resumes on landing"; the landing friction is omitted |
| 9 | Cloth vertical response is a single restitution (0.60) applied to the cushion pop | the vertical channel has no coefficient of its own (§5, trap #2) |
| 10 | Spin decay is applied whenever a ball touches the cloth, uniformly | #7's "constant spin decay" is kept, but its coupling to the normal force is not modelled |
| 11 | Depenetration: overlapping pairs are pushed apart along the line of centres when they are already separating | a robustness measure the spec's canonical resolution does not mention; without it a tight rack can lock |
| 12 | The fact-kind vocabulary is the prototype's, not the final one | #6/#7 must fix the fact types; this prototype's `RailContact{frozen_no_count}` shows the frozen clause wants a first-class field |
| 13 | Rules: only the break-classification precedence table (for judging rows) is implemented | no rules engine, by design |

## 8. Rendered frames and the vision pass

`shots/` (8 frames per preset, indexed PNG, 250–260 KB each): `NN-break`,
`NN-draw`, `NN-follow`, `NN-cut`, `NN-bank`. Final frames overlay every ball's
path and ring the balls that met a rail; mid-shot frames show only the cue
ball's path so the motion is readable. `shots/` is committed; `out/` is
regenerated.

Vision-model feedback (`read shots/<file>.png?q=…`) is quoted, with the
accept/rebut for each finding, in the **## Vision feedback** section of the
resolution comment on #8.
