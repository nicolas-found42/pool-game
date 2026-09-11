# 0003 — Crate workspace and the determinism contract

Date: 2026-09-11

## Status

Accepted. Formalises the numeric-discipline handoff of #7 §8 and the crate-layout and determinism handoffs of #9 §8 and #11's resolution.

## Context

The map requires a headless, deterministic simulation with Bevy as a rendering/input shell only. #7 locked an event-driven analytic core behind an exact `state_at(t)` facade and handed the numeric discipline to the architecture ticket; #9 fixed the AI crate's interface requirements and handed the workspace layout and the replay contract to the same ticket.

Replayability is the load-bearing property: a match must replay exactly from its rack and its input log, on any machine, or the rules corpus, the AI evaluation, and bug reports lose their ground truth. Alternatives considered: a fixed-step integrator (rejected in #7 — tunneling at break speed; the analytic facade makes a stepped API unnecessary), platform-libm floats (rejected — glibc/macOS libm rounding differs), a binary replay format (rejected — off-convention with the JSON corpus artifacts, un-diffable), and putting policy inference inside the bit-identity contract (rejected — ONNX Runtime documents no CPU bit-identity guarantee; arch-dispatched kernels; fact-check recorded in #11's resolution).

## Decision

Seven crates under `crates/` in a virtual workspace — `pool-rng`, `pool-sim`, `pool-rules`, `pool-match`, `pool-ai`, `pool-headless`, `pool-app` — with one-way dependencies, `bevy` only in the app, `ort` only in `pool-ai` behind a non-default feature, no `rand` anywhere, `unsafe_code` forbidden, and pedantic clippy with a documented per-lint allow-list. CI enforces the bans with a `cargo tree` purity grep.

The determinism contract:

- f64 throughout the simulation; IEEE basic operations and `sqrt` only; no fused multiply-add (Rust emits unfused `fmul`/`fadd`; asserted by a codegen test); transcendentals banned from the core with the pinned `libm` crate as the recorded escape hatch; `f32` only at the ONNX boundary; the shell's display math exempt because the contract is logged-input equivalence.
- Shots are computed to rest in one call behind `rack`/`place`/`strike`/`state_at`; there is no fixed timestep, no `advance(dt)`, and no `FixedUpdate` in the app; facts are event-stamped `{seq, t, group}`.
- Randomness lives in `pool-rng` only: `match_seed` for racks (per #14) and `noise_seed` for execution noise, which applies to policy declarations only.
- The input log is the free-choice sequence (placements, declarations, spot requests, option picks) in JSON, schema-checked, with no wall-clock values; everything else is derived on replay.
- `Session` in `pool-match` is the single drive loop shared by the app and the headless harness; `pool-sim` owns placement predicates and `pool-rules` owns the spot search and every rules decision.
- Determinism is checked by canonical `state_hash()` (FNV-1a over `f64::to_bits`) against `docs/spec/goldens.json` on macOS arm64 and ubuntu x86-64 with a pinned toolchain; no auto-update tooling. Policy inference is explicitly outside the contract.

## Consequences

- Replay is a pure function of rack arrangement + input log + profile + pinned code; the corpus and the golden replays gate every change to the sim or the rules layer.
- The Bevy shell cannot influence simulation state except through logged inputs, and no ECS type crosses into the simulation crates.
- Windows and linux-arm64 bit-identity are argued by the arithmetic of the contract, not yet tested; adding runners is a CI change.
- Prototype #8 fills the golden-shot corpus and reports any formulation needing a pinned `libm` function; #16 may revise the pipe payload fields; the placeholder CI workflow of ADR 0001 becomes the two-OS matrix when the first code lands.
