# prototypes/ — throwaway evidence, not the game

Status after #12: **all three are kept as evidence** (the user's call; the issue threads link to the frames
and result files here, and deleting them would re-break those links). None of this is spec, none of it
ships, and nothing in it is built by the workspace `architecture.md` §1 defines — each crate is a
standalone, dependency-light artifact with its own README. The decisions these prototypes informed are
in `docs/spec/`; where a number here disagrees with the spec, the spec's *merged* value is authoritative
and the prototype's is the measurement that produced it.

**Retention, per artifact** (they did not arrive with the same disposition, and the merge did not
flatten them):

- **`cue-ux/`** is the one with a schedule on the record: its README said it "will be deleted once
  `docs/spec/ux-cue.md` is settled". That section settled at #10 and merged at #12, so the deletion came
  due — and was **overridden at #12**, recorded in `ux-cue.md` §11 and in the prototype's README, because
  the frames here are the primary source for `ux-cue.md` §9's vision record and the #10 thread links to
  them.
- **`physics-core/`** carried no deletion schedule, only "everything here is disposable except the
  findings, which live in `RESULTS.md`". It is the most heavily cited of the three: `physics.md` §8 pins
  eight corpus rows against its runs, §9's testing strategy takes its measurements, and §12's provisional
  table names its evidence — so deleting it would cost the spec its provenance, not just a link.
- **`ai-spine/`** carried no deletion schedule either ("nothing here ships"). `ai-constants.md`'s
  measured rows and `ai.md` §9/§10 cite its `results/*.json` and the ONNX artifacts by sha256.

Deleting any of them later is therefore a spec-provenance change: the citations above would have to move
first, not after.

| Directory | Ticket | Question it answered | What it fed |
|---|---|---|---|
| `physics-core/` | #8 | Does the locked model fit measured break-shot behaviour, and is the fitting approach tractable? | `docs/spec/physics.md` — the whole section: the amended constants (`e_n` 0.78, `e_slate`, the `μb` table), the 3D tangential-channel ruling, the drop predicate, the ladder's gates, the corpus rows' pinned parameters and dispositions, the property invariants. Key artifacts: `RESULTS.md`, `results/`, `shots/` |
| `cue-ux/` | #10 | How does shot authoring (aim, power drag, spin, masse, elevation) read top-down? | `docs/spec/ux-cue.md` — its §10's nine rulings and the vision record of §9. Key artifacts: `shots/` (the 15 frames, the primary evidence), `README.md` |
| `ai-spine/` | #16 | What does the decision pipeline's spine cost, and what does it pin? | `docs/spec/ai-constants.md` (the AI section's constants appendix) and `docs/spec/ai.md` §9/§10 — `B`, `R`/`C`, the σ curve, the serve path's cost and binary-size delta, the eval seeds. Key artifacts: `results/*.json`, `results/onnx/policy-smoke.onnx`, `python/runs/` |

Each prototype's README states its own build/run commands and its toolchain pins. The toolchain pins that
are spec material (the Python training stack) are also carried in `docs/spec/ai.md` §9, so they survive
independent of these directories.

Reproduction caveat, recorded in #8's amendments: the prototype measurements that *decided* the cushion
rulings were run on a throwaway patched copy of `physics-core` (a one-line tangential projection plus
env-var overrides), not on the committed tree — the committed tree reproduces the base measurements, and
the gate's config banner prints the effective profile on every run.
