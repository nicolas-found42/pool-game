# Crate architecture and determinism contract

Status: spec section, decided at #11 (2026-09-11), merged into the assembled spec by #12 (`README.md`
indexes it and owns the cross-section rulings that touch §2, §6, §10 and §11). The sections it indexes:
`physics.md` (the model this facade serves — #8's formulation findings are merged there),
`rules.md`/`rules-break.md` (the machine and its corpora), `ux-cue.md` (the `input.rs` seam of §10),
`ai.md`/`ai-constants.md` (the policy crate of §7 — #16's constants live in the appendix).

## Scope and sources

This section specifies the workspace the game is built from, the contract between its crates, the numeric discipline that makes replay exact, the simulation facade and time base, seeding, the replay format, the headless harness and the AI pipe built on it, the drive loop, placement-geometry ownership, the Bevy shell's consumption of the simulation, and the checks that keep all of the above true.

| Source | What it fixes here |
|---|---|
| #5 (Bevy survey) | `bevy = "=0.19.1"`, MSRV Rust 1.95.0; std-only sim crate + `Resource` bridge; `MinimalPlugins` headless; input as an explicit state machine |
| #6 (rules) | pure `(state, input) → (record, state')` machine; variant seam; observation contract; adjudication record |
| #7 (physics) | event-driven analytic core; exact `state_at(t)`; mm frame; constants table; no RNG in the sim; the numeric discipline is handed here |
| #9 (AI) | crate interface requirements (sim + rules deps, no Bevy, ORT behind the trait, ORT-free core); no wall-clock inputs; seeded shell RNG; pinned single-threaded ORT; shot-granularity env over the headless CLI |
| #14 (racks) | frozen `seed → arrangement`; the pinned SplitMix64/Lemire/Fisher–Yates procedure; snapshot restore; per-rack seed derivation in the match layer; the match seed persisted in the input log |

**Supersedes, recorded not silent:** #5 §1's "step the pool sim exactly once per `FixedUpdate` tick" and #5 §3's single `sim.step()` system → §4; #6 §5's "tick-stamped" fact stream wording → §4.5; the map's original "fixed-step sim" phrasing → §4, per #7 §1's reframing (the tick is a scheduling quantum, and here it disappears entirely).

## 1. Workspace

A Cargo virtual workspace; no root crate. Seven crates under `crates/`:

    pool-game/
    ├── Cargo.toml                  [workspace] members + [workspace.dependencies] + [workspace.lints]
    ├── rust-toolchain.toml         channel = "1.95.0"   (#5)
    ├── crates/
    │   ├── pool-rng/               seeded randomness: SplitMix64, Lemire next_below, Fisher–Yates, polar Gaussian
    │   ├── pool-sim/               physics: frame, constants, profile, rack generator, event-driven core, facts, state_at
    │   ├── pool-rules/             the 8-ball machine, adjudication record, variant seam, spot search
    │   ├── pool-match/             match layer + Session drive loop + input-log types
    │   ├── pool-ai/                encoding, candidate generator, scripted planner, micro, policy trait (ort feature)
    │   ├── pool-headless/          clap CLI: replay | strike | pipe
    │   └── pool-app/               Bevy shell (bin target `pool`)
    ├── training/                   Python: SB3 PPO and the Gymnasium env over the pipe (#9); never built by Cargo
    ├── config/profiles/            profile JSON records (§12)
    ├── assets/                     textures/, shaders/, policies/ (§12)
    └── docs/spec/, docs/adr/

| Crate | Role | Depends on |
|---|---|---|
| `pool-rng` | SplitMix64 stream, `next_below` (Lemire multiply-shift with rejection), Fisher–Yates shuffle, Marsaglia-polar Gaussian; pinned `libm` for `ln` (§5) | `libm` |
| `pool-sim` | mm frame and constants (#7 §2/§5); `Profile`; `Arrangement` + the frozen rack generator (#14); ball state; event-driven core; the **fact/event types** (the observation contract's vocabulary); exact `state_at(t)`; geometry predicates; `state_hash` (§11) | `pool-rng`, `serde` |
| `pool-rules` | the rack-scoped `(state, input) → (record, state')` machine; the `Adjudication` record; the variant seam trait (#6 §1); the spot search over sim predicates (§9) | `pool-sim` |
| `pool-match` | match bookkeeping (race target, breaker, per-rack seed derivation, #14); the `Session` drive loop (§8); the input-log types (§6) | `pool-sim`, `pool-rules`, `pool-rng` |
| `pool-ai` | observation encoding, typed candidate generator, scripted planner, analytic micro, policy trait; `OnnxPolicy` behind the non-default `ort` feature | `pool-sim`, `pool-rules`; optional `ort`; `rayon` with canonical aggregation (§4) |
| `pool-headless` | the CLI: `replay`, `strike`, `pipe` (§7) | `pool-sim`, `pool-rules`, `pool-match`, `pool-ai`, `clap` |
| `pool-app` | the Bevy shell (§10): input, playback, bridge, AI host, render, UI | all of the above + `bevy` + `ort` (feature on) |

Conventions: `[workspace.dependencies]` carries every shared pin; `[workspace.lints]` carries the lint baseline of §2; edition 2024; library crates are version `0.1.0`, unpublished; the two binaries are `pool` and `pool-headless`; the sim and rules crates contain no file I/O — binaries read files (§12).

## 2. Dependency and purity contract

The crate graph is acyclic and one-way, with exactly the edges of §1's table. On top of that:

- **`bevy` appears in no dependency tree except `pool-app`'s.** The simulation, rules, match, AI, and harness crates are plain Rust; the ECS never reaches below the shell.
- **`ort` appears only in `pool-ai`, behind the non-default `ort` feature.** The app enables it; the
  default workspace build never does, and `pool-headless` never does. `pool-ai`'s core — encoding,
  candidates, planner, micro — is ORT-free and tests without the runtime (#9 §8). **One exception,
  ruled at #12 (R2 in `README.md` §3):** the artifact contract — ONNX load, the golden vectors, the
  sha256 check — is machine-checked by a dedicated CI job running `cargo test -p pool-ai --features ort`.
  That job is the only place CI enables the feature; the default `cargo test --workspace` and the purity
  grep below still see an ORT-free graph.
- **`rand` appears nowhere.** All randomness is `pool-rng` (§5); the simulator itself consumes none (#7 §1).
- **`unsafe_code = "forbid"` workspace-wide** (`[workspace.lints.rust]`). ORT's FFI lives inside its dependency, not in this workspace.
- **Lints:** `clippy = { all = "deny", pedantic = { level = "deny", priority = -1 } }` in `[workspace.lints.clippy]`, with an explicit, commented allow-list for the lints that fight the numeric discipline or the domain vocabulary (for example `cast_precision_loss` at the deliberate f64→f32 boundary of §3, `similar_names` for `mu_s`/`mu_r`). Every allow is a reviewable line in the manifest, never a blanket exception.
- **Serde:** the contract types — facts, `Adjudication`, log entries — carry `serde` derives in `pool-sim`/`pool-rules`; `serde_json` is built with the **`float_roundtrip`** feature so an `f64` survives serialize → parse bit-exactly, and the version is pinned wherever serialized bytes are compared. Golden hashes hash state bits, never JSON text (§11).
- **Enforcement:** CI runs a purity grep — `cargo tree` per crate must not show `bevy` outside `pool-app` or `ort` where the feature is off — alongside the commands of §11.

## 3. Numeric discipline

The determinism contract's core, formalising #7 §8's handoff. It binds `pool-sim`, `pool-rules`, `pool-match`, and `pool-ai`'s numeric core.

- **`f64` everywhere in the simulation.** Positions mm, velocities mm/s, angular velocity rad/s, event times s; plain `f64` fields with unit-documented names — no newtypes, no `f32`, no fixed-point. Constants are literals from #7 §5 (and the pinned `√3` of #14), never runtime `sqrt` calls where a literal is written.
- **Only IEEE-754 basic operations and `sqrt`** — `+ − × ÷`, comparisons, and the IEEE-required correctly-rounded square root; these are bit-identical on every conformant target.
- **No fused multiply-add: Rust emits unfused `fmul`/`fadd` (its LLVM IR carries no `contract` fast-math flag), and the workspace never writes `mul_add` in the sim.** There is no `-C fp-contract` flag to cite — the discipline is the emitted behaviour, so a codegen test asserts it (the fact-check behind this wording is recorded in the #11 resolution).
- **Transcendentals are banned from the simulation core.** Every locked formulation of #7 must reduce to algebra and `sqrt`: event-time solvers are algebraic, `μb(v)` is a fitted piecewise curve evaluated with basic operations, and mode transitions are threshold comparisons. If a formulation genuinely requires a transcendental, the escape hatch is the **pinned `libm` crate** — pure Rust, version-locked (`=0.2.16` at spec time), with the function and its use recorded in the constants table; the platform `libm` (glibc/macOS libm differences) is never called from the core. Prototype #8 reports any formulation that needs this.
- **The rules layer does no float arithmetic beyond predicates.** Reachability, counters, and adjudication are integer/enum logic; geometry questions (head string, clearance, frozen, spot) are answered by `pool-sim` predicates with exact comparisons — never epsilon-guessed by the machine.
- **`f32` exists only at the ONNX boundary**, inside `pool-ai`: f64 → f32 when encoding the observation, f32 → f64 when a policy output becomes a declaration (`as` casts are round-to-nearest-even and deterministic). Those declarations are logged inputs, so the cast never re-runs in replay.
- **The Bevy shell's display math is exempt.** The shell's aim/hover/HUD math may use platform functions because its outputs enter the game only as logged inputs; the contract is **logged-input equivalence** — replay consumes the log, never the shell's float history. The AI core stays under the discipline so that its decisions reproduce across machines.
- **Scope and cross-target claim:** bit-identity is claimed for identical builds on the desktop targets (macOS arm64, linux x86-64; Windows by the same argument, see §11). Denormals are not flushed (Rust sets no FTZ/DAZ), and the sim asserts finite state at every event step and at rest — a NaN is a bug, never a game state.

## 4. Simulation facade and time

**A shot is computed to rest in one call; there is no stepped integration.**

    Rack::generate(seed) -> Arrangement                     // frozen function (#14)
    Sim::new(table: Table, arrangement: Arrangement) -> Sim // Table carries the profile (#7 §5)
    sim.place_cue(pos) -> Result<(), PlacementError>        // geometry predicates only (§9)
    sim.strike(decl: StrikeDecl) -> Result<Shot, StrikeError> // miscue envelope enforced here (#7 §4)
    shot.state_at(t) -> [BallState; 16]                     // exact for any t ∈ [0, rest_t]
    shot.events() -> &[Fact]                                // the facts stream (§4.5)
    shot.rest() -> RestBlock                                // final state, pocketed, off-table, frozen pairs

- **`state_at(t)` is exact** — an analytic evaluation of each ball's motion mode at `t`, not an interpolation between samples. Playback samples it at render rate; nothing in the design interpolates (#7 §1).
- **No fixed timestep exists.** The 240 Hz tick of #5/#7 discussion is retired with a supersede note: the app runs no `FixedUpdate`, keeps no accumulator, and the sim exposes no `advance(dt)`. The map's "fixed-step" phrasing is satisfied by the facade contract (#7 §1).
- **Error policy:** input-boundary problems — a placement outside its domain, a strike past the miscue envelope — are `Result`s the caller must handle; internal invariants (rack construction from a cleared table, group resolution, geometry consistency) are asserts. Nothing silently falls back: per #14, a violated invariant is a bug, never a repair.
- **The shell calls `Session::request` synchronously** (§8): the batch is milliseconds, and the frame may hitch for that time at a strike; inference never runs inside the request (it happens before, in the worker of §10).
- **AI candidate verification** (pool-ai) may run on `rayon`, restricted to independent sim evaluations whose aggregation is canonical — results are sorted by candidate index before any decision, so parallel and serial execution agree exactly. `pool-sim`, `pool-rules`, and `pool-match` are single-threaded.

### 4.5 The fact stream

The physics layer reports facts only (#6 §5). This section fixes their stamp and container; the fact kinds themselves are #7's event vocabulary.

    struct Fact { seq: u64, t: f64, group: Option<u32>, kind: FactKind }

- `seq` is a monotonically increasing index within one shot; `t` is the exact event time in seconds; `group` is the simultaneity-group id where #7 §1's grouping applies (ε is a spec constant, #7 §5).
- **This supersedes #6 §5's "tick-stamped" wording** — there are no ticks to stamp with. Order is `seq`; simultaneity is `group`; the rules layer's legality-favouring tie-break operates on groups (#6 §3), never on the simulator's internal order.
- `RestBlock` carries the shot's final per-ball state and motion modes, the pocketed and off-table sets, and the frozen-pair annotations read from the simulation (`CONTEXT.md`'s frozen definition).

## 5. Seeding and randomness

`pool-rng` is the single implementation of the pinned RNG kit — SplitMix64 as the stream, `next_below` as Lemire multiply-shift with rejection, Fisher–Yates for shuffles, and a Marsaglia-polar Gaussian (`sqrt` plus one pinned `libm::ln`) for execution noise. #14's rack generator consumes exactly this kit.

Two `u64` seeds live in the input-log header (§6):

| Seed | Consumer |
|---|---|
| `match_seed` | rack construction: rack *i*'s seed is the SplitMix64 stream seeded with `match_seed` after *i+1* steps (#14); re-racks restore the stored snapshot and never re-derive |
| `noise_seed` | the execution-noise stream: a σ draw is consumed for every **policy** declaration, in log order |

- **Execution noise applies only to policy declarations.** Each logged declaration carries `from_policy`; the `Session` perturbs those (never a human's) with the σ spec for the match's difficulty and the corresponding checkpoint (#9 §7). The aim σ curve and the level assignment are measured and merged (`ai.md` §7); the speed/spin/elevation σ set is an assumption with a named re-pin, the correlation structure is unmeasured by construction, and the checkpoint identities are per-run artifacts — `difficulty.checkpoint` carries the artifact's sha256 (`ai.md` §9, `ai-constants.md` §5).
- The log stores the **intent plus `from_policy`**, not the perturbed strike: replay re-derives the same perturbation from the same stream, so what is recorded stays readable ("what the player/policy meant") and what is simulated stays exactly reproducible.
- The simulator itself holds no RNG; nothing in `pool-sim` or `pool-rules` may call `pool-rng`.

## 6. Replay and record formats

**Principle: the input log is the free-choice sequence; everything else is derived.** Racks, seeds' expansions, adjudications, event logs, race scores, and re-rack snapshots are all recomputed on replay.

An input-log entry mirrors the rules machine's input vocabulary exactly — four kinds:

| Entry | Fields | Notes |
|---|---|---|
| `placement` | `domain` (`above_head_string` \| `anywhere`), `pos` | #6 §1's `AwaitingPlacement` |
| `spot_request` | — | the 1.6 ¶2 request; legal only when every legal object ball is above the head string (#6 §9) |
| `declaration` | `from_policy`, `call` (`{ball, pocket}` \| `safety` \| `break`), `aim` (unit 2-vector), `speed` (mm/s), `spin` (`a`, `b` as **fractions of the miscue envelope**, `|(a, b)| ≤ 1` — the input-log schema's contract, ruled at #10 and confirmed at #12's R1), `elevation` (rad) | one atomic declaration (#7 §4); the call carries the open-table 8-claim (#6 §2) |
| `option` | `option_id` | one option of the current `AwaitingChoice` tree — break-foul, illegal-break, 8-on-break, stalemate offer/accept; ids are defined by the rules corpus |

Header: `format_version`, `profile` id, `match_seed`, `noise_seed`, `difficulty` (`level` + `checkpoint` — inert when both seats are human), `race_target`. **No wall-clock value ever appears in the log** (#9); the AI's 1 s SLO is calibration, never content.

- The format is JSON, validated against `docs/spec/input-log.schema.json`; `format_version` bumps only when an entry's meaning changes, never for additions or key reordering (the #14 corpus discipline).
- Event logs, adjudication records, and candidate dumps are **emitted artifacts** — for fitting, the corpus, debugging, and the debug overlay — and are never required to replay.
- Golden replays (recorded logs for canonical matches) ride in CI per §11.

## 7. Headless CLI and the AI pipe

`pool-headless` is the one binary that runs the game without Bevy; it is what CI, the corpus, and the Python training env drive.

    pool-headless replay <input-log.json> [--emit records.jsonl] [--emit-events events.jsonl]
    pool-headless strike --rack-seed <u64> [--placement x,y] --strike-file <strike.json> [--emit-events ...]
    pool-headless pipe

- `replay` runs a whole match through `Session` and asserts the final state; `--emit` writes adjudication records or the facts stream as JSONL for inspection.
- `strike` computes one shot from a seed and a strike declaration — the physics debugging and #8 hook.
- `pipe` serves the AI environment: JSONL over stdio, a versioned handshake line first, then one request/response per line. **One session per process** — N parallel training envs are N processes, matching the usual vectorized-env process model; no multiplexing.
- **The pipe carries the `pool-ai`-computed observation, candidate set, and legality mask**, not raw state alone: encoding and candidate generation have exactly one implementation (the Rust one the served policy also uses), and Python hosts only the policy network. The step contract is #9's — rest state + context in, declaration out, sim to rest, adjudication record back — and the payload is pinned at #16: the 64-float observation, the 16-float candidate row with a dynamic candidate axis, and opset 17 (`ai.md` §9, `ai-constants.md` §2.4).
- Corpus validation (`rules-break.json`, `physics-break.json`, #14) stays in `cargo test`; it is test-shaped, not a CLI mode.

## 8. Session and the drive loop

`pool-match` exposes one drive loop, and both binaries use it — there is no second implementation to drift.

    Session::new(config: MatchConfig) -> Session   // profile, match_seed, noise_seed, difficulty, race_target
    session.state() -> SessionView                 // rules state + the derived view the shell renders
    session.request(input: Request) -> Result<Adjudication, InputError>

- `Session` owns the machine, the simulation, both seeded streams, and the input log; it validates the input against the machine's current state, applies execution noise to policy declarations, simulates the shot to rest, adjudicates, appends the log entry, and returns the record.
- The match layer above it sequences racks, derives per-rack seeds (#14), tracks the race, and owns the re-rack transitions; the machine itself remains rack-scoped per #6.
- Because the loop is shared, the app and the headless harness produce byte-identical games from identical logs — the property §11's golden replays check.

## 9. Rules/sim boundary: placement and spotting

#7 §8's ruling, made concrete:

| Question | Answered by | Examples |
|---|---|---|
| Is point `p` on/above the head string? | `pool-sim` | containment with the centre-over-line semantics (2.13), #6's on-line cases |
| Is `p` clear of the other balls? | `pool-sim` | distance > 2R to every ball not excluded |
| Are two balls frozen? | `pool-sim` | read from the simulation, per `CONTEXT.md` |
| Where may the 8 be spotted? | `pool-sim` geometry, `pool-rules` order | candidate positions along the long string; the search order, contact preference, and fallback above the foot spot are rule decisions (1.5) |
| Which placement domain applies, and is this placement legal in it? | `pool-rules` | `AboveHeadString` vs `Anywhere` (#6), the 3.10/3.11 predicates composed from sim answers |

The spot algorithm is the only geometry-shaped procedure that lives in `pool-rules` because its ordering *is* rule vocabulary; it calls sim predicates and never touches ball state directly.

## 10. The Bevy shell

The shell consumes the simulation without leaking ECS into it: `bevy` types stop at the `pool-app` boundary, and the only channel from shell to simulation state is a logged input through `Session`. #5's `Resource` bridge survives; the tick does not.

`pool-app/src/` — nine modules:

| Module | Responsibility |
|---|---|
| `main.rs` | app setup and plugins; `clap` flags (`--profile`, `--policy`, `--difficulty`, `--debug-candidates`); startup asset load |
| `session.rs` | the `Session` resource; turn resolution — whose turn, human vs policy seat, choice-state presentation hooks |
| `input.rs` | cue aim/placement state machines (the #10 seam); writes declarations into the session |
| `playback.rs` | the shot timeline: presentation clock, exact `state_at(t)` sampling per frame, snap to rest, next-state handoff |
| `bridge.rs` | entity spawn, `BallIndex` links, one-way sim→ECS sync of ball transforms, table geometry |
| `ai_host.rs` | the worker thread owning the policy; request/response channels; ORT confined here |
| `render.rs` | table mesh, ball sprites, gizmos (aim line, ghost ball, strike marker) per #5 §4 |
| `ui.rs` | HUD: turn, group, foul messages, ball-in-hand prompt, choice menus, difficulty select, degraded-mode badge |
| `debug.rs` | the candidate-set/per-candidate-score overlay behind `--debug-candidates` (#9 §8) |

- **Turn flow:** `session.rs` sees an awaiting state for a policy seat → `ai_host` is asked for a declaration (observation, candidates, mask; the worker runs encode → generate → score → select → micro) → the declaration goes to `Session::request` marked `from_policy`. Human seats flow through `input.rs` instead. Both paths converge on the same `request` call.
- **Sync is one-way:** after `request`, `playback` samples the `Shot` into ball transforms; nothing copies ECS state back into the simulation, and no rule outcome is re-derived in the shell — UI reads `Adjudication` records.
- **No `FixedUpdate`;** systems run in `Update`/`PostUpdate` as plain functions of session state and the presentation clock. Losing focus pauses presentation only; game state is already computed.
- The exact cue-input machine and the strike-marker look are **ruled in `ux-cue.md` §2/§10** (the declaration they emit is `input-log.schema.json`'s `declaration`); this section fixes only their seam and module.

## 11. Determinism checks and CI

- **`state_hash()`** (`pool-sim`): FNV-1a 64 over the canonical byte stream of a position — per ball, in id order (cue, 1–15), `f64::to_bits()` of position, velocity, and angular velocity, plus motion mode and pocketed/off-table flags. Stable, dependency-free, and compares bits, never decimal text.
- **`docs/spec/goldens.json`**: named entries of the form `{name, kind: rack | shot | replay, input, expected}`. Racks are #14's fixtures; shot entries pin a strike's rest `state_hash`; replay entries pin a recorded input log's final state hash and adjudication record. This is the **determinism-golden corpus** — in-repo, running in `cargo test` and CI. The **fitting goldens** (`physics.md` §6/§9: the ladder's curve gates, the pooltool differential, the dataset replay, the break hook) are a different corpus: they run **locally/offline with committed results**, because their data lives outside the repository (`benchmark-data.md`). #8's prototype measured the physics numbers; its hashes are a first measured instance, not this spec's values — the implementation pins the shot entries' hashes at M1 (`README.md` §6), and the corpus grows from there.
- **Tests recompute and compare**; on mismatch they fail and dump the actual state beside the golden for diffing. **No auto-update tooling ships** (the #14 discipline).
- **CI** (the placeholder workflow of ADR 0001 becomes this when code lands): `rust-toolchain.toml`-pinned 1.95.0; `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` (goldens, corpus, and replay fixtures included); the §2 purity grep; on **macOS arm64 and ubuntu x86-64**, both verifying the same committed goldens. Windows/Linux-arm64 identity is claimed by §3's arithmetic argument, not yet tested; adding the runners is a CI change, not a contract change. **Plus the R2 job:** `cargo test -p pool-ai --features ort` — the ONNX load, the golden vectors, and the sha256 contract of `ai.md` §9.
- **Policy numerics are outside this contract.** A fact-check for #11 found no CPU bit-identity guarantee in ONNX Runtime (arch-dispatched MLAS kernels, no documented determinism promise), so: replay determinism rests on the sim, the rules layer, and the logged inputs; per-machine reproducibility is best-effort under #9's pinned single-threaded ORT; cross-platform AI evaluations are not bit-comparable and the spec says so.
- The physics *testing strategy* is `physics.md` §9 — the golden-shot corpus' contents, the property invariants, and the local-not-CI rule for fitting; the mechanism above is what the determinism half plugs into.

## 12. Files, pins, and versioning

- `config/profiles/*.json` — profile records (values, provenance, condition metadata; #7 §5). Binaries read them with `std::fs` + serde; `pool-sim` exposes the `Profile` type and the default-profile const, and does no file I/O.
- `assets/policies/*.onnx` + `.sha256` — the shipped policy artifacts; `pool-ai` loads and hash-checks them at startup (#9 §8), by path, without Bevy's asset server.
- `assets/textures/`, `assets/shaders/` — per #5 §6.
- Pins: Rust 1.95.0 / edition 2024; `bevy = "=0.19.1"` (#5; budget a version-bump pass per Bevy release); `libm = "=0.2.16"`; `serde_json` with `float_roundtrip` pinned where bytes are compared; `ort` v2, single intra-op thread, version pinned with its opset (#9); `clap` 4; `rayon` in `pool-ai` only. Exact versions are locked by `Cargo.lock`; this table is the spec's floor.

## 13. Handoffs and fog

- **#8 (physics prototype):** its findings are merged into `physics.md` (§8 pins the corpus rows' parameters and dispositions); the golden-shot corpus' first entries are named there (§9) and their hashes are pinned by M1's implementation.
- **#12 (spec assembly):** done — this section is merged; the input-log schema ships beside it, and the §2/§6/§11 rulings it triggered are recorded in `README.md` §3.
- **#16 (AI prototype):** its cost envelope is merged into `ai-constants.md` and `ai.md` (§9/§11 there); the σ and work-unit constants are pinned there.
- **Fog closed at #12:** the "physics testing strategy" mechanism is fixed here and its corpus contents
  in `physics.md` §9; "spec document structure" is `README.md`.
