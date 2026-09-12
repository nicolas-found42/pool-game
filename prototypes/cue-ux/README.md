# cue-ux prototype — shot-authoring UX (wayfinder ticket #10)

**Throwaway prototype. Not the game.** It answers one question: *how does shot authoring
actually feel* — the drag-back cue, the spin/masse affordances, and the readability of cue
elevation in a top-down view. There is **no physics**: committing a shot echoes the
declaration and nothing moves. It owes `docs/spec/architecture.md` nothing and is not part
of the crate workspace. This README originally scheduled its own deletion "once `docs/spec/ux-cue.md` is settled";
that section settled at #10 and merged at #12, and the assembly **overrode the deletion** — the prototype
stays as indexed evidence (`prototypes/README.md`), because the issue threads and `ux-cue.md` §9's vision
record link to the frames here.

Decision inputs it obeys (not re-designed here):

- `#7` (physics resolution) §2 frame — SI millimetres, playing surface 2540 x 1270 mm,
  ball radius 28.575 mm, top-down, origin at the surface centre;
- `#7` §4 — a strike is `{aim direction, cue-ball launch speed, spin offsets (a, b) in tip
  radii, elevation}`, with the miscue envelope rejected at the input boundary;
- `#7` §5 — tip–ball friction mu = 0.6, and the ~0.5 R lateral limit for a level cue;
- `docs/spec/input-log.schema.json` — the declaration the read-out prints.

## Toolchain

| | |
|---|---|
| Rust | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1` (repo MSRV 1.95.0+) |
| Bevy | `bevy = "=0.19.1"` (the version locked on #5), default features |
| Machine measured on | macOS 26.4.1 / Darwin 25.4.0, Apple M5 (10 cores, 16 GiB), Metal backend |

Build (first build compiles the Bevy dependency graph at `opt-level = 2`; ~10 min cold on
the machine above, seconds thereafter):

```sh
cd prototypes/cue-ux
cargo build
```

`Cargo.lock` is deliberately not committed — the dependency graph is pinned by the
`=0.19.1` Bevy requirement plus the toolchain above, which is all a throwaway needs.

## Run it (interactive)

```sh
cd prototypes/cue-ux
cargo run
```

Controls, as shipped:

| Input | Action |
|---|---|
| mouse move | aim (no button) |
| press on the table, drag **back**, release | drag-back cue: sets power, then commits |
| drag inside the strike-marker card | spin offsets `(a, b)` |
| `A` / `D` | `a` (across the ball face), `Shift` = fine |
| `W` / `S` | `b` (up the ball face, + = follow side), `Shift` = fine |
| `C` | centre the strike point |
| wheel, or `Up` / `Down` | cue elevation, 5 deg steps (`Shift` = 1 deg) |
| `1`–`9`, `Tab` | load a scripted state / cycle all 15 |
| `0` | reset the declaration |
| `Enter` | commit (keyboard alternative to the release) |
| `Esc` | quit |

The top-down cue foreshortens by `cos(elevation)`, the drag-back gap is drawn to scale, and
`pull N mm` is printed on the cloth at the gap.

## Screenshot mode

Deterministic: fixed 1600x920 window at scale factor 1.0, fixed state list, no interaction.
`--frames N` is how many frames each state settles for before the capture.

```sh
cd prototypes/cue-ux
cargo run -- --screenshot shots --state all --frames 30     # all 15 states -> shots/*.png
cargo run -- --screenshot shots/07-spin-draw-max.png \
             --state 07-spin-draw-max --frames 30           # one state -> one named file
```

The output directory (dir form) or the file's parent directory (file form) is created if it
does not exist.

State names (`--state <name>`, or `all`; an unknown name exits 2 and lists the valid ones):

| file | what it is for |
|---|---|
| `01-overview-level` | whole table + widget, level cue, mid drag-back |
| `02-overview-elevated-70` | whole table + widget, 70 deg cue (foreshortening) |
| `03-aim-guide-cut` | aim guide on a 27 deg cut, cue at the ball, zero power |
| `04-power-low` / `05-power-mid` / `06-power-max` | the drag-back power ladder (60 / 175 / 350 mm) |
| `07-spin-draw-max` | max draw, offset at the envelope edge |
| `08-spin-follow-max` | max follow, offset at the envelope edge |
| `09-spin-side-right-max` / `10-spin-side-left-max` | max english, offset at the envelope edge |
| `11-masse-45` / `12-masse-70` | elevated masse (45 deg / 70 deg) |
| `13-envelope-edge` | offset exactly on the envelope — **legal** |
| `14-over-limit-rejected` | 1.84x envelope — **REJECTED**, red, not committable |
| `15-commit-readout` | the post-commit read-out state |

Output: 1600x920 PNGs, ~260 KB each. Raw view on a branch:

```
https://raw.githubusercontent.com/nicolas-found42/pool-game/<branch>/prototypes/cue-ux/shots/<file>.png
```

## Vision pass

The frames were read back with a vision model (the `?q=` form routes to one when the active
model has no image input; the tool did not report which model answered):

```sh
# from the repo root, one call per frame
read prototypes/cue-ux/shots/07-spin-draw-max.png?q="Describe the cue/spin/elevation affordances you see. Is the strike point and power readable? What is confusing or ambiguous?"
```

Answers are summarised, with my accept/rebut reasoning, in `docs/spec/ux-cue.md`
("Vision feedback"). The pass found two real defects — an exact-envelope declaration read as
rejected, and `+a` (right english) drawn on the left of the widget — both fixed and
re-measured; the remaining findings are design input.

## Numbers the prototype uses

| quantity | value | source |
|---|---|---|
| ball radius | 28.575 mm | #7 §2 |
| playing surface | 2540 x 1270 mm | #7 §2 |
| tip friction mu | 0.6 | #7 §5 |
| miscue envelope | `R * mu / sqrt(1 + mu^2)` = 14.70 mm = 0.5145 R | derived, matches #7 §4 "~0.5 R" |
| envelope in "tip radii" | 2.315 tr **assuming a 12.7 mm tip** | prototype assumption, see below |
| max launch speed | 7000 mm/s at 350 mm of drag (linear) | prototype assumption |
| elevation range | 0–80 deg | prototype assumption |
| tip-radius unit | 6.35 mm (12.7 mm tip) | prototype assumption |

`#7` §4 states offsets in **tip radii** but the limit in **ball radii** ("~0.5 R"). Those are
different units: 0.5 R = 14.29 mm is ~2.25 tip radii of a 12.7 mm tip. The prototype
therefore enforces the limit geometrically (14.70 mm in the cue-face plane) and prints the
tip-radius reading alongside it. The unit is an open question for the gate.

## Frame conventions the prototype had to pick (not given by #7)

- `a > 0` = offset toward the **shooter's right**; `b > 0` = contact **above centre**
  (follow side with a level cue); elevation > 0 = butt raised, so the cue pushes *down*.
- The `(a, b)` plane is perpendicular to the **cue axis**, not to the table. At 70 deg the
  same `b` is mostly a horizontal offset in world terms, which is why the same declaration
  produces very different spin at different elevations — the widget draws this.
- A declaration exactly on the envelope is legal (1e-3 mm tolerance); past it is rejected.

## Known limits of the prototype

- No physics, no rules, no opponent, no persistence: state is in memory, the shot is echoed.
- The top-down HUD panel is a development read-out, not a shipping HUD design.
- The aim guide is input-direction geometry only (no squirt/swerve in the preview).
- Elevation does not change the drawn impact point on the object ball (the sim would).
