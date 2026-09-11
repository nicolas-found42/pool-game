# physics-core — throwaway prototype (wayfinder ticket #8)

A headless, dependency-free Rust prototype of the locked physics model in the
physics resolution of **#7** and the crate/determinism contract of **#11**. It
exists to answer one question from ticket #8 — *can the locked model fit
measured break behaviour, and does the fitting approach look tractable?* — and
to hand the spec-assembly ticket (#12) the deltas its §-by-§ skeleton needs.

**It is not the game.** No rules engine, no Bevy, no shell, no rack seeding
beyond the fixtures, no performance work. Everything here is disposable except
the findings, which live in `RESULTS.md` and the resolution comment on #8.

## Build and toolchain

Rust 1.98.1 (stable) on the campaign machine (Apple M5, macOS 26.4, arm64).
The crate has **no dependencies** — no crates.io fetch, no numba, no Python.

    cargo build --release          # ~4 s cold, no network

The binary is `target/release/physics-core`.

## What is implemented

- **Event-driven analytic core** (`src/sim.rs`): every candidate event time
  (ball–ball, ball–cushion, ball–jaw, ball–tip, pocket drop, landing, motion
  mode transition) is solved in closed form; the earliest wins; events inside
  ε = 1e-9 s form one simultaneity group applied in a canonical order
  (contact type, then ball ids). Contacts are *solved*, not sampled, so the
  core cannot tunnel.
- **Exact `state_at(t)`**: each ball carries an analytic law per segment and
  the core keeps a segment timeline, so `state_at(t)` evaluates the mode in
  force at `t` — no interpolation anywhere.
- **Ball–cloth** (`src/sim.rs::law_for`): piecewise Coulomb sliding (quadratic
  contact-point decay), linear rolling resistance, constant spin decay, with
  sliding → rolling → spinning → stationary solved as analytic transitions.
- **Ball–ball** (`hit_pair`): impulse-based frictional inelastic with a
  speed-dependent μb(v) (piecewise-linear in the core, fitted from the
  exponential outside it).
- **Cushion** (`impulse_surface` + `src/table.rs`): frictional-inelastic
  impulse whose contact normal carries the 63.5 % nose height, plus jaw faces,
  pocket mouths as geometry and capture by mouth crossing.
- **Cue** (`src/strike.rs`): post-impact cue-ball state from
  `{aim, launch speed, spin (a, b) in tip radii, elevation}` with the miscue
  envelope (circle of radius 0.5 R) enforced at the input boundary; squirt from
  the pivot length, evaluated algebraically (no transcendental in the core).
- **Airborne** ballistic motion with landing restitution, plus the diagnostics
  of `src/diag.rs`.

## Gate commands (all measured on the campaign machine)

| Command | What it produces | Runtime |
|---|---|---|
| `./target/release/physics-core break --out out` | break acceptance hook: energy, tunnelling, ≥4-to-rails, bit-identical rerun; writes `out/break.facts.csv`, `out/break.traj.csv`, `out/break-acceptance.txt` | ~1 s |
| `./target/release/physics-core shot --preset draw --out out` | one stroked shot: facts + trajectory dump (presets: break, draw, follow, cut, bank) | <1 s |
| `./target/release/physics-core rows --spec ../../docs/spec/physics-break.json --out out --budget 700` | drives all 14 corpus rows, pins the null parameters, writes `out/rows.json`, `out/rows.md` | ~10 min |
| `./target/release/physics-core ladder --stage all --curves ../../data/curves --out out` | ladder stages 1–4 against the digitized curves; writes `out/stage*.txt/.csv` | ~40 s |
| `./target/release/physics-core diag --out out` | break acceptance + sleep-threshold pinning | ~2 s |
| `./target/release/physics-core tolerance --out out` | rest-position tolerance pinning (perturbation → rest deviation) | ~3 s |
| `./target/release/physics-core diff --out out` | the four synthetic shots against pooltool's side (`out/rust-diff.json`) | <1 s |
| `./target/release/physics-core render --preset all --out out` | PNG frames under `out/shots/` (8 per preset, indexed PNG) | ~3 s |
| `~/pool-game-data/pooltool-venv/bin/python tools/pooltool_diff.py --out out` | pooltool's side of the differential (`out/pooltool-diff.json`) | ~40 s (numba compile included) |

Debug helpers used while building: `spin`, `cushion`, `drawtest`, `probe`,
`ke`, `rest`, `debug`.

## Layout

    src/consts.rs    #7 §5 constants + the profile record + the μb table
    src/table.rs     frame, cushions, jaws, pocket mouths/drop regions, rack slots
    src/sim.rs       the event-driven core, laws, solvers, impulses
    src/strike.rs    cue input model, miscue envelope, squirt
    src/facts.rs     fact derivation (2.7 rail predicate, frozen clause) + the
                     break-legality precedence table (for judging rows only)
    src/diag.rs      break acceptance, sleep pinning, tolerance pinning, renders
    src/ladder.rs    fitting-ladder stages 1–4
    src/rows.rs      the 14 corpus rows: search for the pinned parameters
    src/diff.rs      the synthetic-shot differential (our side)
    src/render.rs    palette PNG writer + top-down table renderer
    tools/           pooltool driver (Python, uses the venv)

Runtime artefacts (`out/`) are regenerated by the commands above; the committed
subset is `shots/` (frames), `results/` (the measurement dumps quoted in
`RESULTS.md`) and nothing larger.
