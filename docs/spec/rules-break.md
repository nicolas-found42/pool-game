# Rack construction and break legality

Status: spec section, decided at #14 (2026-09-11), merged into the assembled spec by #12 (`rules.md` is the machine it plugs into; `README.md` indexes both). The corpus files beside it cite its clauses by anchor.

## Scope and sources

This section specifies the two items that #6's resolution §11 graduated: **rack construction** — how the 15-ball triangle is built, reproduced, and audited — and **break legality** — how a break shot is classified and what each classification offers. The rules state machine, the legal-shot definition, the ball-in-hand domains, and the match layer are #6's section; this section references them and never restates them. Vocabulary follows `CONTEXT.md`.

| Source | Version | Used here for |
|---|---|---|
| WPA *Rules of Play* | effective 2025-09-15 | §4.2 (rack), §4.3 (break), §4.7 (spotting), §4.8 ¶2 (loss conditions exclude the break), §4.9–§4.10 (foul penalty), General §1.5, §1.6, §2.7, §2.13, §3.1–§3.3, §3.5 |
| CSI *Official Rules* | 2025-08-12 | rule 2-3-3, cited only because WPA is silent and always by number (§4) |
| APA *Team Manual* | — | not needed by this section |

Every departure from the WPA text has a line in §4 or in #6 §9; none is silent.

**Numbering.** The headings below are unnumbered so the corpus files' `rules-break.md#<anchor>` cites stay
stable; the `§N` cites in this file and in `rules.md` read as follows. **§1** Scope and sources. **§2** Rack
construction — §2.1 The rack function, §2.2 Constraints (WPA 4.2), §2.3 Uniformity obligation, §2.4 Slots
and geometry, §2.5 Clear-table invariant, §2.6 Generator (pinned), §2.7 Seed and state, §2.8 Uniformity
(proof obligation), §2.9 Fixture seeds and audit, §2.10 Rack invariants. **§3** Break legality — §3.1 Driven
to a rail, §3.2 Classification precedence, §3.3 Illegal break, §3.4 Break foul, §3.5 Accepting the table in
position, §3.6 Eight on the break, §3.7 Re-rack transition. **§4** Deviations. **§5** Corpus contract.

## Rack construction

### The rack function

A rack is a **frozen function** `seed → arrangement`: a pure, deterministic map from a 64-bit seed to an assignment of the 15 numbered balls to the rack's 15 slots. Its behaviour is fixed for the life of this spec; changing it is a spec change with the consequences of §5.

The production path is always the seeded generator of §2.6 — hand-authored layouts exist only as fixtures (tests, screenshots, the AI harness) and are never a source of production racks. The constructor is a kernel the rules layer invokes, not a rules decision: it is deterministic, side-effect free, and makes no legality judgement. The physics crate owns the geometry frame and constants (#7 §2); the match layer owns seed derivation (§2.7); the rules layer owns what a rack means — 4.2's constraints and §3's break semantics.

### Constraints (WPA 4.2)

4.2's sentences are the constraint set, clause by clause:

1. "racked as tightly as possible in a triangle" — exact contact: adjacent balls at centre distance exactly 2R;
2. "the apex ball on the Foot Spot";
3. "the 8-ball as the first ball that is directly below the apex ball" — ball 8 in the middle of the third row, slot (3,1);
4. "One from each group of seven will be on the two lower corners of the triangle" — slots (5,0) and (5,4) hold one ball from 1–7 and one from 9–15;
5. "The other balls are placed in the triangle without purposeful or intentional pattern."

Rows parallel to the foot string are a geometry clarification implied by the triangle, not an extra constraint.

### Uniformity obligation (operational "no pattern")

A machine has no intent to inspect, so clause 5 is specified operationally: the arrangement is a **uniform draw over all arrangements satisfying clauses 1–4** — every legal arrangement equally likely. §2.8 proves the construction meets this; there is no statistical test of it. No rejection rule exists: the generator never re-draws or repairs an arrangement because a human might call it a pattern, and WPA's intent-based offence (4.2 read with 3.16, dropped for machine play, #6 §9) has no machine counterpart.

### Slots and geometry

Rows are numbered 1–5 apex-first; row r has r slots. Slot `(r, k)` carries in-row index k counted from the **y-negative end** of its row, so slot `(r, 0)` is always on the same side of the table. Corpus files write slot ids as `r.k` (`3.1`, `5.0`, …).

| Row r | Slots | 4.2 content |
|---|---|---|
| 1 | (1,0) | apex ball, on the foot spot |
| 2 | (2,0), (2,1) | drawn |
| 3 | (3,0), (3,1), (3,2) | ball 8 at (3,1) |
| 4 | (4,0) … (4,3) | drawn |
| 5 | (5,0) … (5,4) | corners (5,0) and (5,4): one ball from each group |

Positions come from one closed form, evaluated once per construction, in this pinned order:

    x(r)    = x_foot + ((r − 1) · R) · √3          r ∈ 1…5
    y(r, k) = (k − (r − 1) / 2) · 2R                k ∈ 0…r−1

with `x_foot = +635 mm` (the foot spot; #7 §2), `R = 28.575 mm` (the ball radius; #7 §5), the row step R√3 down-table (towards the foot rail, +x), and `√3` the spec literal `1.7320508075688772` — never a runtime `sqrt` call (#7 §8's numeric discipline, formalised by #11). Row r is centred on the long string, in-row neighbours are 2R apart, and the row spans y = ±(r − 1) R.

- (1,0) = (635, 0): the apex on the foot spot;
- (3,1) = (635 + 2R√3, 0): the third row's middle, directly below the apex;
- (5,0) = (635 + 4R√3, −4R) and (5,4) = (635 + 4R√3, +4R): the two lower corners.

No literal 15-entry coordinate table exists in the spec or the implementation; positions are recomputed from the closed form.

The lattice is exact contact: every adjacent pair — in-row neighbours and neighbours in the two adjacent rows — is exactly 2R apart. The rack therefore begins as one dense contact graph, and the first break event is a single large simultaneity group; #7 §1's canonical order resolves it and reports the group, so #6 §3's legality-favouring tie-break applies to the group, never to the simulator's internal order.

### Clear-table invariant

A rack is constructed only from a cleared table: no object ball outside the rack's slots, and the cue ball above the head string (4.3(a)). Construction asserts on anything else — a violation is a bug, not a rules situation — and the spec defines **no relocation or nudge geometry**, because there is nothing to compute: construction is reachable only from a cleared table, including every re-rack path (§3.7), where the abandoned rack's balls are ended first.

### Generator (pinned)

`generate(seed) → arrangement` consumes one SplitMix64 stream seeded with the rack's u64 seed, in exactly this order. The fixture generator implements exactly this, and `rack-fixtures.json` is its pinned instance.

1. **PRNG.** SplitMix64 with its published constants: the state starts at the seed, each output advances it by γ = 0x9E3779B97F4A7C15, and mixes:

       state += γ                                    (u64, wrapping)
       z      = state
       z      = (z XOR (z >> 30)) · 0xBF58476D1CE4E5B9
       z      = (z XOR (z >> 27)) · 0x94D049BB133111EB
       output = z XOR (z >> 31)

2. **Bounded draw.** `next_below(n)` is Lemire's multiply-shift with rejection: take the next output `x` and form the 128-bit product `m = x · n`; let `l` and `h` be its low and high 64 bits. If `l < n`, compare `l` against the threshold `t = (2⁶⁴ − n) mod n` and re-draw while `l < t`; otherwise return `h`, an unbiased value in `0…n−1`. Rejection belongs to the draw, never to the rack: the generator never re-draws an arrangement.
3. **Corner sides.** One `next_below(2)` draw: 0 ⇒ group A (balls 1–7) takes (5,0) and group B (balls 9–15) takes (5,4); 1 ⇒ the reverse.
4. **Corner balls.** Two further draws, in slot order (5,0) then (5,4): a slot's draw is `next_below(7)` over that slot's group's seven balls in **ascending ball-number order**, and the selected ball takes the slot. The two pools are disjoint, so neither shrinks between the draws.
5. **Free shuffle.** The 12 balls left — all 15 except the 8 and the two corner balls — are listed in ascending ball-number order and shuffled by Fisher–Yates descending: for i = 11 down to 1, j = next_below(i + 1), swap the elements at i and j.
6. **Placement.** Ball 8 takes slot (3,1). The shuffled 12 are dealt in order into the remaining 12 slots in canonical slot order — rows 1–5, in-row index ascending from the y-negative end, skipping (3,1), (5,0), (5,4): (1,0), (2,0), (2,1), (3,0), (3,2), (4,0), (4,1), (4,2), (4,3), (5,1), (5,2), (5,3).

The settled description "Fisher–Yates downward from index 14" is the generic form of the descending loop over a 15-element sequence; the rack generator's shuffle is the 12-element one of step 5 plus the pinned assignments of steps 3–4 and 6, written that way so the 8 can never leave slot (3,1).

### Seed and state

- **Rack state holds both the seed and the frozen 15-slot arrangement snapshot.** The snapshot is the identity source: "the same positions" always means this array.
- **Re-rack restores the snapshot by construction**, seed unchanged — which is what makes a stalemate re-rack (#6 §7) and the break-option re-racks (§3.7) reproduce identical positions.
- **Seed agreement is asserted, never repaired:** a separate assertion regenerates the arrangement from the seed and compares it to the snapshot as array equality over the 15-slot array with a first-difference diagnostic (slot id, expected, actual). A mismatch is a bug; the assertion never auto-updates the snapshot, the seed, or the generator.
- Arrangement identity is exactly that array equality; there is **no hash field**.
- **Per-rack seed derivation lives in the match layer, never in the rack:** rack i (0-based) of a match with seed `match_seed` gets `rack_seed(match_seed, i)` = the output of a SplitMix64 stream seeded with `match_seed` after i + 1 steps. Re-racks reuse the rack's seed and never re-derive it. The match seed is persisted in the input log (#6 §1), so the match's racks are reproducible from it.

### Uniformity (proof obligation)

The construction of §2.6 discharges clause 5's obligation by proof, not test:

- the side draw is uniform over the two orientations; each corner ball is uniform over its group's seven balls and the two draws are independent — so the corner assignment is uniform over the 2 · 7 · 7 = 98 legal ordered corner pairs;
- the free shuffle is a uniform permutation of the 12 leftover balls, drawn from a disjoint run of the same stream, and it is dealt into the 12 free slots in a fixed order: uniform over the 12! fills, independent of the corners;
- the product is uniform over the 98 · 12! legal arrangements, so every arrangement satisfying 4.2's clauses is equally likely.

There is deliberately **no statistical uniformity test**, here or in the corpus; the sweeps of §2.9 audit invariants, and the golden of §2.9 catches derivation drift.

### Fixture seeds and audit

- The fixture seed list is **0–15**. One fixture seed's arrangement is the **hand-derived golden**, derived on paper from §2.6 and cross-checked digit-for-digit; `rack-fixtures.json` carries it and names the seed.
- Audit, not proof: a per-seed invariant sweep over 0–15, and a generator property sweep over a wider fixed range (0–255) asserting §2.10's invariants for every seed plus generate-twice byte-equality.
- AI training and evaluation seed ranges are disjoint — a constraint on the AI/eval harness (#9), recorded as a handoff, never a rules-layer concern.

### Rack invariants

Every generated arrangement must satisfy all of these; a violation is a bug:

| # | Invariant | Source |
|---|---|---|
| 1 | the apex ball is in slot (1,0) | 4.2 |
| 2 | ball 8 is in slot (3,1) | 4.2 |
| 3 | slots (5,0) and (5,4) hold one ball from each group | 4.2 |
| 4 | the 15 slots hold 15 distinct ball numbers | 4.2 |
| 5 | every adjacent pair of slots is exactly 2R apart | 4.2 ("as tightly as possible") |
| 6 | the arrangement is one of the 98 · 12! legal arrangements | §2.8 |

Failure of generation is impossible by construction: every group has seven balls, so both corner slots are always coverable, and the 12-ball free pool always fills the 12 free slots. Generation therefore never returns a fallback or a partial rack — it asserts; a fired assert is a generator bug, not a rules outcome.

## Break legality

The break is an ordinary shot except where 4.3 says otherwise: the cue ball begins in hand above the head string (4.3(a)); no ball is called and **no particular ball must be struck first** (4.3(b)), so 3.2's wrong-ball-first foul is not evaluated on the break; and a ball pocketed without a foul lets the breaker continue with the table open (4.3(c), #6 §2). Everything after the break — target, call, continuation — is #6's section.

### Driven to a rail

The break count uses 2.7's per-ball predicate, evaluated from the shot's facts:

- a ball that was not touching a rail and then touches it is driven to that rail;
- a ball frozen to a rail at shot start is not driven to it unless it leaves and returns;
- a ball pocketed or driven off the table (2.6) counts as driven to a rail;
- the count is over **distinct object balls**, not contacts: several rails by one ball count once, and a ball never counts twice;
- the predicate is per ball, not an after-contact predicate: when a ball reached a rail relative to ball–ball contacts is irrelevant to 4.3(d) (contrast 3.3's after-contact rule for ordinary shots, #6 §3);
- the cue ball is never one of the four — 4.3(d) requires four object balls;
- frozen status is read from the pre-shot state (#6 §10 fact 8) as simulation truth, per #6 §9's deviation of 2.7/3.7/Reg 26.

### Classification precedence

Each break is classified once, from the fact stream, in this pinned order:

| # | Condition | Classification | Consequence |
|---|---|---|---|
| 1 | the 8 was pocketed or driven off the table | 8-on-break tree (§3.6) | the rail count is not consulted |
| 2 | else, a ball was pocketed | legal break (4.3(c)) | no foul: the breaker continues, table open; with a foul: §3.4 |
| 3 | else, fewer than four distinct object balls reached a rail | illegal break (4.3(d)), §3.3 | this tree governs, whether or not a foul occurred |
| 4 | else — nothing pocketed, at least four to rails | clean legal break | the break ends: play passes, the table stays open |

The rail requirement sits behind 4.3(d)'s own chapeau ("If no object-ball is pocketed"), so it is never evaluated when a ball was pocketed. Fouls are classified alongside the break, not instead of it: the break-reachable fouls are 3.1 (cue ball pocketed or driven off the table), 3.3 (no rail after contact — a total miss of the rack included) and 3.5 (object ball driven off the table); 3.2 is not evaluated (4.3(b)), and the placement faults 3.10/3.11 belong to the placement validator. A foul never adds ball-in-hand anywhere on a break; it routes to §3.3 or §3.4.

### Illegal break

4.3(d): "If no object-ball is pocketed, at least four object-balls must be driven to one or more rails, or the shot results in an illegal break, and the incoming player has the option of: (1) accepting the table in position, or (2) re-racking and breaking, or (3) re-racking and allowing the offending player to break again."

- The offer is exactly those three options, in WPA's order; the chooser is the **incoming player**.
- Option (1) is cue-ball-aware (§3.5): with the cue ball on the table the incoming player shoots it as it lies (`awaiting_shot`, target `open`); with the cue ball off the table — a scratch, so the shot is also foul under 3.1 — acceptance places it in hand **above the head string** (`awaiting_placement`). This tree grants no ball-in-hand anywhere: a scratch's usual 4.9 remedy is subsumed (§4).
- Options (2) and (3) reconstruct the rack (§3.7): (2) puts the incoming player at the break — the next breaker in the match's rotation; (3) puts the offending breaker back on the break.
- **An illegal break that is also a foul keeps this tree** (CSI 2-3-3; WPA states no precedence — §4). The ordered foul reasons are still recorded in the adjudication record, but they change neither the offer nor the cue ball's handling. A total miss of the rack is both a 3.3 foul and an illegal break, so it lands here too.
- An illegal break is not itself one of 4.9's standard fouls.

### Break foul

4.3(f) is the 8-on-the-break case and lives in §3.6. The other break fouls are:

- **an object ball driven off the table (4.3(g)):** a foul (3.5); the ball stays out of play, except the 8, which is spotted (4.3(g), 4.7); the incoming player may (1) accept the table in position or (2) take the cue ball in hand above the head string;
- **any other break foul (4.3(h)):** the incoming player may (1) accept the table in position or (2) take the cue ball in hand above the head string.

Neither option is ever ball in hand anywhere: after a break foul the cue ball's domain is `above_head_string` (4.3(f)(g)(h)), never 4.9's "anywhere" penalty. Off-table object balls stay off (4.3(g)); only the 8 is ever spotted, and only from the break (4.7). The two options differ exactly when the cue ball is on the table: acceptance (§3.5) leaves it where it lies, while the in-hand option grants the placement above the head string even then; when the cue ball is already off the table they coincide in effect, and the offer still enumerates both, as WPA does, with the corpus recording the chosen option rather than a collapsed set.

### Accepting the table in position

Acceptance leaves everything on the table exactly where it lies: every object ball stays where it rests — pocketed balls stay down, off-table balls stay off — and **the cue ball's rest position is carried into the next state whenever it is still on the table**. With the cue ball on the table the accepting player shoots from there: `awaiting_shot`, shooter per the tree, target `open` (the table remains open after a break, #6 §2). Acceptance stays executable with the cue ball off the table — pocketed or driven off (3.1) — but then the cue ball must be placed: `awaiting_placement` with `domain: above_head_string`. Acceptance never grants ball in hand anywhere; `above_head_string` is the break's only restricted domain (4.3(f)(g)(h)).

An option whose text grants the cue ball instead (4.3(f)(1), 4.3(g)(2), 4.3(h)(2)) always yields `awaiting_placement` / `above_head_string`, even when the cue ball is still on the table — that option's text grants the placement.

Two acceptances are cue-ball-specific and are pinned here once:

- the **illegal-break tree** (4.3(d)(1), §3.3) follows the rule above, and the tree contains no ball-in-hand option at all;
- **4.3(e)(1)** — spot the 8 and accept the balls in position — is the breaker continuing, not a foul remedy: cue ball at rest ⇒ `awaiting_shot`, shooter the breaker, target `open`, `spotted` = [8]; cue ball off the table ⇒ `awaiting_placement` / `above_head_string`.

### Eight on the break

- The 8 pocketed or driven off the table on the break is **never a win and never a loss**: 4.8's loss conditions do not apply to the break (4.8 ¶2). It is spotted, or the balls are re-racked, at the chooser's option below; 4.7 keeps spotting exclusive to the 8 and to the break.
- **Legal break, 8 pocketed (4.3(e)):** not a foul. The **breaker** chooses (1) spot the 8 and accept the balls in position — §3.5: the breaker continues from the position the break left, `spotted` = [8], `awaiting_shot`, target `open` — or (2) re-break: the rack is reconstructed (§3.7) and the breaker breaks again.
- **8 pocketed on a foul (4.3(f)):** the **opponent** chooses (1) re-spot the 8 and shoot with the cue ball in hand above the head string — always `awaiting_placement` / `above_head_string`, even if the cue ball is still on the table, because this option's text grants the placement — or (2) re-break: the rack is reconstructed and the opponent breaks. WPA names no breaker for this option; the model reads it as the option's chooser taking the break (§4).
- **The 8 driven off the table** takes the same branch by §3.2's precedence: it is a foul (3.5), the 8 is spotted (4.7), and the 4.3(f) choices govern.
- **Spotting position** is 1.5's total algorithm, transcribed in #6 §4: on the long string between the foot spot and the foot rail, as close as possible to the foot spot, without moving a ball; if the foot-spot position is unavailable, in contact (if possible) with the interfering ball; never in contact with the cue ball — a small separation is kept; if all of the long string below the foot spot is blocked, above the foot spot as close as possible to it. Two machine-precision points are pinned in §4 because the human text is not quantitative.

### Re-rack transition

Choosing a re-rack option — 4.3(d)(2), 4.3(d)(3), 4.3(e)(2), 4.3(f)(2) — transitions immediately:

1. the abandoned rack's balls are ended and the table is cleared (§2.5);
2. the arrangement is restored from the rack's frozen snapshot, seed unchanged (§2.7) — positionally identical;
3. the seed-agreement assertion runs (§2.7): regenerating from the seed must reproduce the snapshot; a mismatch is a bug and never rewrites anything;
4. the breaker flag is set per the option — 4.3(d)(2): the incoming player, next in the match's break rotation; 4.3(d)(3): the offending breaker, breaking again; 4.3(e)(2): the breaker again; 4.3(f)(2): the opponent;
5. the rack's shot counter resets to 0;
6. the state becomes `awaiting_placement` with `domain: above_head_string` for that breaker (#6 §1's `AwaitingPlacement`), and no placement from the abandoned rack survives.

## Deviations

| WPA | This model | Reason |
|---|---|---|
| 4.3(d) + 4.3(h): illegal break and a foul on one shot | the illegal-break tree governs | WPA states no precedence; CSI 2-3-3 makes the legal break decisive — the game cannot continue until the break is legal |
| 4.3(d) vs 4.3(e)/(f): rail count when the 8 leaves the table | the 8-on-break trees govern; the rail count is not consulted | WPA does not order the two; resolving the 8 beats re-racking for it |
| 4.2 "without purposeful or intentional pattern" | a uniformity obligation — uniform over all legal arrangements, no rejection rule | intent is not machine-readable; the obligation states what a pattern-free rack means operationally |
| 1.5 spotting, "in contact" / "a small separation" | in contact means gap ≈ 0 within the simulation's contact tolerance; small separation means at least the spec constant δ — where the two collide, the cue-ball separation wins | the human text is not quantitative (#6 §4) |
| 4.3(e)/(f) "re-breaking" | the option's chooser takes the break | WPA names no breaker for these two options |
| 4.3(d)(1), (g)(1), (h)(1) acceptance with the cue ball off the table | the accepting player places it in hand above the head string | WPA does not say where a cue ball that is not on the table goes when acceptance is chosen, and the tree grants no ball in hand |

Deviations already recorded in #6 §9 (frozen status as simulation truth, dropped intent rules, the stalemate path) apply to this section unchanged and are not repeated.

## Corpus contract

| File | Tier | Content |
|---|---|---|
| `rules-break.json` | rules (facts) | break scenarios in the observation-contract vocabulary: ordered `ball_ball`, `rail`, `pocket` (carrying the pocket's identity) and `off_table` facts, a rest block, and the cue's impulse at t0 as the break declaration |
| `physics-break.json` | physics (meet-in-the-middle) | requirement rows — named scenario, target fact pattern, acceptance = a recorded run that produces it; shot parameters are `null` and marked "pinned by #8" |
| `rack-fixtures.json` | fixtures | the 0–15 seed list, the hand-derived golden arrangement, and the invariants' expected results |
| `break-corpus.schema.json` | structure | validates the two corpora |

- **Citation.** Corpus entries cite this section by anchor as `rules-break.md#<anchor>` — `#break-legality`, `#illegal-break`, `#break-foul`, `#eight-on-the-break`, `#driven-to-a-rail`, and the rest of the headings above.
- **Facts.** The rules tier mirrors #6 §10 and #7's observation contract exactly: the four kinds `ball_ball`, `rail`, `pocket`, `off_table`, ordered, with `pocket` events carrying the pocket's identity, plus a rest block and the declaration of the cue's impulse at t0. Fixtures declare their simultaneity groups explicitly; the assertion pins the legality-favouring resolution inside a group (#6 §3).
- **Shape.** One shot per entry; branch follow-ups are explicit, chained by `follow_up` and `chosen_option`; every option of every declared tree has at least one follow-up entry; entries run independently from their declared pre-state (composition by replay, never by shared mutable state). A meta-test asserts coverage against the declared trees and the named scenario list.
- **Assertion depth.** Each entry asserts the full adjudication record: verdict, ordered foul reasons, the option set and chooser per tree, the chosen option, the applied action, balls pocketed, off-table balls, spotted balls, and the resulting state.
- **Envelope and versioning.** Each file carries `corpus_version` (integer, monotone), a generator note, and the seed fixture list it assumes. The version bumps only when an existing entry's meaning changes — never for additions or key reordering. A generator change is a spec change: it must bump the version and re-derive the golden in the same commit. No auto-update tooling ships: nothing regenerates the golden or the corpus from the implementation.
- **Schema strictness.** `break-corpus.schema.json` validates structure and routing only — required keys, event kinds from the pinned vocabulary, inline-vs-reference exclusivity, arrangement shape, offer-list non-emptiness and order. Rules semantics and coverage are not the schema's job.
- **Formatting.** Canonical: 2-space indent, LF endings, stable key order as written, one entry per object.

**Named scenario list** (coverage target): cue ball misses the rack; scratch on the break; 8 on the break legally; 8 on the break on a foul; object ball off the table; fewer than four balls to rails without a foul; fewer than four balls to rails with a foul; the illegal-break option tree; and the two clean legal breaks (a ball pocketed; at least four balls to rails with nothing pocketed) that complete §3.2's branches.

**Handoffs.** #8 pins the physics rows' shot parameters; #9 keeps AI training and evaluation seed ranges disjoint and reads the fixture list; #10 never decides its own rack seed; #11 freezes the rack-function contract; #12 merges this section into the assembled spec.
