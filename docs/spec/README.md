# The pool-game spec

Status: assembled at #12 (2026-09-11). Every wayfinder ticket on the map (#2) is resolved; this document
is the spec's entry point, the record of its cross-section rulings, and the handoff to implementation
sessions. The map's destination — *an implementation session can build the game from the spec with no
further design decisions* — is met from this commit onward.

**How this spec is assembled.** Each section below is one file, decided at its ticket and merged here;
the machine-checked artifacts beside them are the acceptance tests. Decisions live in the sections,
rationale lives in the cited issue threads, and the throwaway prototypes are evidence (`prototypes/README.md`).
This file holds only what has no single home: the index, the cross-section rulings, the milestones, and
the handoff.

## 1. Destination and scope

A top-down 8-ball pool game in Rust + Bevy: tournament-grade physics fitted to measured data, full cue
control with a 3D cue-ball strike marker and masse, hot-seat two-player plus a learned-policy AI,
desktop only.

| In scope | Out of scope |
|---|---|
| The seven-crate workspace of `architecture.md`; a deterministic headless sim | Web/WASM and mobile targets; online multiplayer/networking |
| 8-ball by the WPA spine; a variant-generic rules machine | 9-ball and other variants (the seam stays generic; no variant specs) |
| Learned-policy AI: train in Python, serve in Rust | Audio, animation, and menu design beyond the polish stub (§6) |
| Fitting to measured data and replay of a real pro corpus | The map's planning scope — building the game is the follow-on effort (§5) |

## 2. The section map

| File | Section | Decided at | What it fixes |
|---|---|---|---|
| `README.md` | this document | #12 | index, rulings, milestones, handoff |
| `physics.md` | physics | #3 (survey), #7 (lock), #8 (prototype) | the locked model, its constants with labels, the fitting ladder, the fact stream, the testing strategy |
| `rules.md` | the rules layer | #6 | the rack-scoped state machine, targets, fouls, deviations, the observation contract |
| `rules-break.md` | rack and break | #14 | the frozen rack function, break classification and option trees, the corpus contract |
| `architecture.md` | crate architecture | #11 (+ #5) | workspace, purity and numeric discipline, sim facade, seeding, replay, the headless CLI, the Bevy shell, CI |
| `ux-cue.md` | cue input and UX | #10 | the authored object, the gesture chain, the strike widget, guides, HUD |
| `ai.md` | the AI seat | #9 (+ #4) | the macro/micro pipeline, action space, training plan, difficulty, runtime stack, gates |
| `ai-constants.md` | AI constants appendix | #16 | the measured cost envelope, by reference, with provenance |
| `benchmark-data.md` | fitting data | #15 | what the datasets and curves can and cannot support |
| `input-log.schema.json` | replay format | #10/#11 | the input log's contract, including the `spin` unit |
| `rules-break.json`, `physics-break.json`, `rack-fixtures.json`, `break-corpus.schema.json` | corpora | #6/#8/#14 | branch-exhaustive adjudication tests, physics requirement rows, rack fixtures |
| `CONTEXT.md` | vocabulary | #6 | domain terms; the machine's vocabulary, not the spec's prose |

Read order for a first session: this file → `architecture.md` → the section of the milestone you are on
→ that milestone's artifacts.

## 3. Cross-section rulings

Tickets were decided independently; where two decisions met, the ruling is recorded here and carried
into the section it belongs to. The first four are the ones an implementer would otherwise have to guess
at.

| # | Conflict | Ruling | Where |
|---|---|---|---|
| R1 | **The spin unit is declared twice, differently.** `input-log.schema.json` and `ux-cue.md` §7 pin `(a, b)` as *fractions of the miscue envelope* (`1.0` = the limit, `|(a, b)| ≤ 1`, `ρ_max = R·μ/√(1+μ²)` = 14.70 mm); `break-corpus.schema.json`'s `spin` def said *tip radii* (`a_tip_radii` / `b_tip_radii`), and `architecture.md` §6's declaration table said the same | One unit: **fractions of the miscue envelope**, in every artifact that declares a spin type. The corpus schema's def is corrected to `{a, b}` with the envelope wording, matching the input log; no corpus entry ever used the old shape, so no entry changes meaning and `corpus_version` does not bump for this | `break-corpus.schema.json`, `input-log.schema.json`, `architecture.md` §6, `physics.md` §4, `ai.md` §2 |
| R2 | **Whether CI loads the ONNX artifact.** #4 §3.3 requires "CI validates it loads in `ort`"; #9 §9 puts ONNX load + golden vectors in CI; `architecture.md` §2 says "`pool-headless` and CI never do" enable `ort` | A **dedicated CI job** runs `cargo test -p pool-ai --features ort` — the ONNX load, the golden vectors, and the hash check. The *default* workspace build and test stay ORT-free: the purity grep's statement is about the default feature set, and no other crate ever depends on `ort`. #16's rule stands: the golden vectors are a contract on the artifact's sha256, which `difficulty.checkpoint` carries | `architecture.md` §2/§11, `ai.md` §9/§10 |
| R3 | **Two kinds of golden, one CI rule.** `architecture.md` §11 puts `goldens.json` in `cargo test` and CI; #8 §9 runs "the golden corpus" locally with committed results | They are different corpora. **Determinism goldens** — rack fixtures, shot rest-state hashes, replay final hashes — are in-repo and run in CI. **Fitting goldens** — the ladder's curve gates, the pooltool differential, the dataset replay, the break hook — run **locally/offline with committed results**, because their data lives outside the repository (`benchmark-data.md`). `goldens.json`'s mechanism ships with M0; its *contents* grow from M1 (physics) and M2 (replay) | `architecture.md` §11, `physics.md` §9 |
| R4 | **The fixed timestep.** #3 and #5 recommend an event core *behind a fixed-step facade*; #7 §1 and `architecture.md` §4 rule that no fixed step exists — `state_at(t)` is exact and the sim exposes no `advance(dt)` | `architecture.md` §4 stands; the supersede notes stay where they are written. The 240 Hz tick is the shell's scheduling quantum only if a render-rate sampler needs one, and it is never the sim's | `architecture.md` §4, `physics.md` §1 |
| R5 | **The strike marker's dimensionality.** #5 §4: "a Gizmos circle + dot suffices — no 3D scene"; `ux-cue.md` §3/§10.2 rules a dedicated **3D cue-ball widget** | #10's widget stands — it is the gated decision, and the top-down view cannot carry the spin offsets. It is a UI overlay on a top-down table, which is exactly the map's constraint: "camera stays top-down; the cue ball is rendered 3D only as a strike-point marker widget" | `ux-cue.md` §3, `architecture.md` §10 |
| R6 | **What `e_n` is.** #3/#7's table called the cushion restitution an *effective* COR; #8's gate ruled it is the **normal-channel coefficient** (0.78), whose effective rebound (0.70 measured) differs by 0.06 | `physics.md` §3.3 carries the semantics and the trap; the constants table says which. Never copy Mathavan's internal `e_n` = 0.98 / μ = 0.14 as either kind of COR | `physics.md` §3.3, §5 |
| R7 | **What "pocketed" is.** #7 §3 said "comes to rest below the playing surface (2.2)"; #8's gate ruled the mouth-crossing predicate plus the `supported_over_mouth` annotation | The predicate replaces the phrasing as a recorded deviation; `CONTEXT.md`'s *Pocketed* already says what the rules layer must do with the annotation | `physics.md` §3.5, `rules.md` §10.7 |
| R8 | **"Frozen" meant two tolerances** (the prototype used 0.5 mm for rails, 0.05 mm for balls) | One constant, **surface gap ≤ 0.5 mm**, for rail and ball frozen status; the rack's exact-contact lattice is unaffected | `physics.md` §5, `rules.md` §10.2 |
| R9 | **Which rail count the rules use.** The simulation's `distinct_object_balls_to_rails` is physical; 2.7/4.3(d) add pocketed and off-table balls | The split is stated in the sections that consume it; every corpus field and report must say which count it is. The prototype's findings on the corpus rows (pb-03's ball number, pb-10/pb-11's zero-rail demands, the conditional off-table rows) are the physics section's §8 | `rules.md` §3, `physics.md` §7/§8 |
| R10 | **The deviations table's conditional row.** `rules.md` §9 listed jump / off-table / 4.8(d) as *conditional* on the physics scope | Resolved: the simulation carries full 3D ball state, so the row is **live** | `rules.md` §9 |
| R11 | **Checkpoint fractions.** #9's constants table carried 10 / 50 / 100 %; #16 measured that they degrade and dropped them | The **monotone-skill rule** replaces the fractions; the table's rows keep their dispositions | `ai.md` §6, `ai-constants.md` §1 |
| R12 | **The WPA cushion acceptance test.** #3/#7's stage 3 gated on the WPA 4–4.5-table-length test; #8 measured it unreachable at the fitted constants (needs ≈9 m/s) | **Dropped, with its reason**: a gate the constants cannot pass is worse than no gate. Stage 3 gates on the retention fit and reports the travel anchors as a residual | `physics.md` §6 |
| R13 | **Policies and the asset server.** #5 §6 routes art through Bevy's `AssetServer`; `architecture.md` §12 loads policy artifacts by path with a hash check | Both: art via `AssetServer`, policy artifacts by path from `pool-ai` (they must load without Bevy and without a window). #5 §6 is not read as universal | `architecture.md` §12 |
| R14 | **Section status lines.** The sections carried "draft spec section… ticket #12 merges" | All say *spec section, merged at #12*; this file is the index that owns assembly | every section's status line |

## 4. What is deliberately not decided here

- **The polish stub.** Audio, animation, and menus are a one-line "later" note. The settings/menu surface
  the shipping app needs is exactly what `architecture.md` §10 already fixes — the CLI flags
  (`--profile`, `--policy`, `--difficulty`, `--debug-candidates`) and the difficulty select in the HUD —
  and what `architecture.md` §12 already fixes for persistence: the profile record and the input log.
  Nothing else is in scope.
- **The two measurement-shaped holes.** `physics.md` §12's provisional constants (each with its settling
  measurement) and `ai.md` §11's re-pin obligations. They are decisions *pending a measurement*, named as
  such, and each carries the measurement that settles it.
- **The playtest-shaped UX items** (`ux-cue.md` §10's residual uncertainty): the face-on card's bet, the
  power map's exponent, the HUD's exact form. They are named for a human hand, with the requirement they
  must satisfy — not left open.

## 5. Milestones and build order

The order is physics core → rules → UX → AI → polish, per the map. Each milestone's exit is observable
and drawn from the gates the sections already define; nothing below invents a new gate. A milestone's
exit is **the evidence, not a claim**: the listed artifacts committed and the listed commands passing.

| M | Deliverable | Entry | Exit (all of) |
|---|---|---|---|
| **M0** | Workspace and determinism contract | — | `cargo test --workspace` green on macOS arm64 + ubuntu x86-64; the §2 purity grep; `state_hash` and the goldens mechanism (`architecture.md` §11); ADR 0001's placeholder CI replaced by the real workflow of `architecture.md` §11 (including R2's `ort` job) |
| **M1** | `pool-sim`: the physics core | M0 | Every locked sub-model of `physics.md` §3 implemented to its formulation; `rack-fixtures.json` and the rack invariants pass; **the fitting ladder's gates** — stage 1's two relations, stage 2 with `μb`'s parametrisation pinned, stage 3's retention fit, stage 4 at the pinned reference tip offset (8.07 mm ≈ 0.28 R) — run locally with committed results; **the break hook** holds (no tunneling, bounded energy, ≥ 4 distinct balls to rails, bit-identical rerun) **and reports the `e_slate` it ran at**; `physics.md` §9's property invariants pass; `physics-break.json`'s 8 produced rows produce their fact patterns (re-aiming is acceptable, §8); the **ball-drop test pins `e_slate`** and the row's label moves from *carried* to *measured* |
| **M2** | `pool-rules` + `pool-match` + `pool-headless` | M1 | The machine of `rules.md` §1 with every deviation of §9; **`rules-break.json`'s branch-exhaustive corpus passes** (every option of every tree); the observation contract is complete (`rules.md` §10) — rail-contact pairs, pocket identity, `supported_over_mouth`, the physical/2.7 split; the spot search and the 1.6 ¶2 request; `pool-headless replay` reproduces a recorded match byte-identically; the replay goldens are in CI |
| **M3** | `pool-app`: the Bevy shell and the UX | M0–M2 | A human plays a full rack hot-seat; the gesture chain, the face-on strike card, the gauge, the guides and the HUD of `ux-cue.md` are on screen; the miscue envelope rejects at the boundary with the ruled tolerance; **the playtest items of `ux-cue.md` §10 are felt by a human** and the §10 residual-uncertainty line is retired or amended; no ECS type reaches below `pool-app` |
| **M4** | `pool-ai` + `training/` | M2 (the ORT-free core may start as soon as M2's types freeze) | The generator, the scripted planner, the micro, the policy trait; `ort` serve with the artifact hash check; the pipe env drives `pool-headless`; imitation → drills → self-play per `ai.md` §6; **the eval bar's cheap gates** (≥ 95 % easy pot, zero illegal declarations, ≤ 1 s p99) hold; **the re-pin obligations of `ai.md` §11** are measured and the appendix's labels updated; **the rail-family selection share is reported** and `R`/`C` are confirmed or revised; the **rack gate** (`ai.md` §10) is reported once the opponent seat exists — or explicitly *not reported*, never proxied |
| **M5** | Polish stub | any time after M3 | The one-line stub note exists; nothing beyond it |

**Dependency notes.** `pool-ai`'s ORT-free core (encoding, candidates, scripted planner, micro) has no
dependency on the shell, so M4's first item overlaps M3 freely — its acceptance is M2's types plus the
headless pipe. The AI's *training* needs M2 (the rules layer) and M1 (the sim), and its *difficulty
calibration* needs the σ curve re-measured on the real sim. The difficulty levels are one policy with
checkpoints + σ (`ai.md` §7); the four levels are a shell concern (M3's HUD select) and a training
concern (M4's checkpoints), and neither invents a second policy.

## 6. The handoff point for implementation sessions

**This commit is the cut.** Before it, design lived in the wayfinder map's tickets; from it, `docs/spec/`
is the source of truth and implementation sessions begin. An implementation session's contract (the
compound is deliberate: `CONTEXT.md`'s *Session* is the game's drive loop, not a work session):

1. **Read in order:** `README.md` §2's map → the section(s) of your milestone → that milestone's
   artifacts (the corpora are the acceptance tests; `benchmark-data.md` is the fitting data).
2. **The spec is not amended by a session** that finds a *gap* — a decision it cannot make within the
   spec's constraints. A gap is recorded where design lives: a plain GitHub issue (referencing the
   section and the milestone), or an ADR under `docs/adr/` when it is architectural. The session then
   proceeds on the smallest decision that does not touch the spec's contracts, and says so in the issue.
3. **A deviation is never silent** (`CONTEXT.md`): a measurement or an implementation finding that
   contradicts a spec value is recorded in the section's deviations table with its reason and its
   measurement, and the section is amended by PR. A *spec change* is a PR that says what it changes and
   why the evidence moved.
4. **Re-pin obligations are work items, not notes.** `physics.md` §12 and `ai.md` §11 name every
   provisional row and the measurement that settles it; a session that measures one updates the row, its
   label, and every gate that depends on it, in the same PR.
5. **The corpora are the bar.** `rules-break.json` (branch-exhaustive), `physics-break.json`
   (requirement rows), `rack-fixtures.json` (pinned), and `goldens.json` (determinism, from M1) — a
   milestone's exit is its artifacts committed and passing, per §5.
6. **Determinism is a property of the code, checked by the tools.** The purity grep, the pinned
   toolchain, `state_hash`, and the replay identity of `architecture.md` §11 are CI's job from M0; a
   session never re-argues them.

**Definition of done for the spec itself:** every decision above has a section, every section names its
sources and its deviations, the provisional rows each name their settling measurement, and the
milestones' exits are observable. The two holes that remain are measurement-shaped, not decision-shaped
(§4) — the spec's remaining work is measurements and code, not more decisions.
