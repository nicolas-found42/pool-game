# Cue input and shot-authoring UX

Status: **gate resolved at #10** (2026-09-11). The nine open questions are answered in §10 and the
`spin` unit + sign convention are pinned in `docs/spec/input-log.schema.json`. §1–§9 stand as the
decided UX/controls section except where §10 overrides them: §3 (the strike card is face-on), §4
(the gauge gains a tick scale), §5 (the tangent line goes default-off) and §7 (the unit is the
envelope fraction) are the four places it does. Ticket #12 merges the section into the assembled
spec; ticket #11 owns the `input.rs` seam it plugs into. Evidence: the throwaway prototype in
`prototypes/` and the resolution on #10.

## 1. The authored object

The UX edits exactly the declaration of #7 §4 — `{aim, speed, spin (a, b), elevation}` — and
nothing else. The read-out prints the input-log body verbatim:

```json
{"aim":{"x":+0.900,"y":+0.436},"speed":3500,
 "spin":{"a":+0.00,"b":-0.42},"elevation":0.000}
```

Four degrees of freedom, three input gestures plus one modal:

| declaration field | gesture | affordance |
|---|---|---|
| `aim` | mouse move | cue stick on the cloth + aim line |
| `speed` | press on the table, drag **back**, release | the cue pulls back; `pull N mm` printed at the gap |
| `spin (a, b)` | drag inside the strike-marker card (`A/D`, `W/S` keys) | the contact dot on the 3D cue ball |
| `elevation` | wheel / `Up`-`Down` | elevation gauge + the cue's projection + the card's shaft |

## 2. The authoring flow (and why it is one gesture chain)

Aim is *not* a click: the pointer always aims, so the first click is unambiguous and can be
spent on power. Pressing on the cloth locks the aim and starts the drag; the cue follows the
pointer backwards; release commits. The prototype prints the pull distance on the cloth
precisely because the gap alone is not self-calibrating — the vision pass confirmed the gap
is visible between frames but does not by itself say *how much*.

Spin is a separate, non-modal gesture on a separate surface (the strike card). It never
competes with aiming for the same pixels, and the keyboard equivalent (`A/D`, `W/S`) exists
so the declaration is reachable without the mouse.

`Enter` commits the same declaration the release would, so the flow is keyboard-completable.

## 3. Spin, masse, and the 3D cue ball

**The top-down view cannot carry the spin offsets.** The `(a, b)` plane is perpendicular to
the *cue axis*; projected onto the cloth, a pure draw offset collapses onto the aim axis —
draw, follow, and a centre hit draw the same top-down dot. Nor can the cloth show the
*handedness* of a side offset. Therefore:

- **The strike point lives in a dedicated 3D cue-ball widget**, not on the table. The cue
  ball is rendered once, 3D, as a strike-point marker (#5's framing); the table's cue ball
  stays a flat disc.
- The widget shows: the ball, the tip contact marker, the **miscue-envelope ring** on the
  ball face, a **face-centre crosshair**, the incoming cue shaft, and the **spin axis**
  (the direction of the angular velocity the strike imparts, `contact x cue axis`), drawn
  as a double-headed arrow because it is an axis, not a direction.
- The shipped card's *shape* is ruled at §10.2: face-on, with no shaft drawn across the ball
  face, and elevation shown by the gauge rather than by the card's tilt.
- Because the widget's cue frame tilts with elevation, the widget is also where the
  *masse* insight is visible: at 70 deg a large `b` offset is mostly a horizontal world
  offset, so the same declaration yields a different spin than at 0 deg. Masse is not a
  separate control; it is `elevation` + off-centre `(a, b)`. Under the gate's face-on card
  (§10.2) that insight is carried by the labelled gauge and the read-out rather than by the
  card's tilt.

Sign convention (pinned in `docs/spec/input-log.schema.json` and §10.3):
`a > 0` is the shooter's **right**, `b > 0` is **above centre** (follow side at zero
elevation). The prototype shipped the mirror first and the vision pass caught it as a
"right english" frame with the dot on the left — the convention has to be written down
somewhere normative, and the schema's `spin` description is that place.

## 4. Elevation reads badly top-down — that is the finding

A top-down camera sees the cue's elevation only as **foreshortening**: a 70 deg cue projects
to ~34 % of its length. The prototype draws that honestly and labels it, and the vision pass
still read it as "a shorter cue", not a raised one — the first two frames of elevation that
were queried produced "difficult to tell that the cue is elevated" and "it looks mostly like
a short, flat 2D stick". Foreshortening alone does not communicate elevation.

Three signals were drawn; the prototype's draft answer is that the **first is required, the
second and third are supporting**:

1. **A side-view elevation gauge next to the cue ball** — a cloth baseline, the cue at its
   real angle, an arc between them, labelled `elevation 45 deg`. Without the label the vision
   pass read the gauge as "another aiming or power affordance"; with it, the angle is read
   correctly in every frame, including 70 deg. The gate adds a coarse tick scale
   (0/30/60/90, §10.1).
2. **The widget's shaft** angled off the ball, with the widget's cloth plane as the datum — *dropped
   at the gate* (§10.2): the card is face-on, so the tilt is not the card's job.
3. **The projected cue's length** on the cloth (foreshortening), plus the numeric read-out.

Alternatives rejected at the gate (§10.1): a dedicated side-view *inset* of the cue ball and cue
(replacing the inline gauge) — it duplicates the gauge — and a numeric-only read-out. Both would
avoid the gauge's pixels, but the gauge is the only signal that read correctly.

## 5. Aim guides

Drawn from the aim ray only (input geometry; no squirt/swerve in the preview): the **aim
line**, the **ghost ball** at first contact, the **object-ball path**, and the **tangent
line** the cue ball leaves on for a stun hit. With no ball on the line, the first **cushion**
and its reflection are shown instead. The read-out states the cut angle and the guide's
first contact.

The vision pass consistently reported the guide cluster as needing the legend and rarely
resolved it without one; every frame that drew the cluster drew the same complaint. Draft
consequences: keep a colour-keyed legend, and the tangent line goes default-off (§10.8) — it is
the line most often confused with the ghost ball.

## 6. Miscue envelope at the input boundary

`#7` §4 makes a past-envelope offset an **input error, not a simulated event**. Three UI
routes: clamp (unselectable), reject (selectable, visibly refused), or warn-then-allow.
The prototype implements **visible rejection**:

- the offset is authored freely; the contact marker turns **red**;
- the ring on the ball face is the limit itself; the read-out prints
  `offset 27.0 mm | miscue envelope 14.7 mm (0.514 R, mu 0.6) = 2.32 tr`;
- the status line turns red and names the overage: *"offset 27.0 mm is 12.3 mm past the
  miscue envelope ... the declaration cannot be committed"*;
- **commit is refused** — release and `Enter` do nothing while the declaration is illegal.

Exactness matters: a declaration authored *exactly* at the limit must be **legal**. The
prototype's first cut compared with `margin <= 0` and read the 0.514 R edge as rejected —
caught by the vision pass as a contradiction between the read-out and the status line, then
fixed with a tolerance (`1e-3 mm`). The spec should state the comparison, not inherit it.

The envelope used is the pure friction cone, `rho = R * mu / sqrt(1 + mu^2)`, independent of
elevation. Whether elevation *should* modulate it (tip weight helps a downward offset,
hurts an upward one; extreme elevation adds a shaft-clearance constraint) is not settled by #7;
§10.5 rules on it — the fixed cone stays, and the direction is recorded as a deviation.

## 7. The unit of `(a, b)` — resolved: fraction of the miscue envelope

`#7` §4 says offsets are "in tip radii" but gives the limit as "~0.5 R". Those disagree by
roughly 4.5x for a 12.7 mm tip (0.5 R = 14.29 mm = 2.25 tip radii). Resolved at #10: `(a, b)`
are **fractions of the miscue envelope** — dimensionless, `1.0` = the limit, so the dot's
distance from the centre of the card *is* the authored value. The geometric cone has no
tip-radius input at all (tip radius and tip elasticity are not modelled), the fraction is
scale-free across profiles, and it bounds the AI's action space (#9) to the unit disc. The
sim converts once at the boundary: `offset_mm = rho_max * value`, with
`rho_max = R*mu/sqrt(1+mu^2)` = 14.70 mm at `mu = 0.6`. The schema's `spin` description carries
the unit and the sign convention; printing tip radii stays a dev-panel derived reading.

## 8. Read-out split

The prototype's panel is a **development** read-out (every intermediate quantity: aim, speed,
pull, both offsets in mm and as a fraction of the envelope, offset vs envelope, intent, guide,
declaration JSON, status). It is right for the prototype and wrong for the shipping HUD, which needs a
3-line player form — turn/group (from #6), the declaration currently being authored, and the
one-line validity state. The dev panel survives behind `--debug-candidates`-style flags
(#11 §10).

## 9. Vision feedback (the 15 committed frames)

Method: each committed PNG in `prototypes/cue-ux/shots/` was read back through the `?q=`
form (`read <path>.png?q="Describe the cue/spin/elevation affordances you see. Is the strike
point and power readable? What is confusing or ambiguous?"`), plus frame-specific probes. The
tool did not report which vision model answered. Machine: Apple M5 / macOS 26.4.1, the same
box that produced the frames. Frames are 1600x920, ~260 KB each, committed under
`prototypes/cue-ux/shots/`.

**What the model read correctly, frame after frame** (accepted):

- **Power.** `power 3500 / 7000 mm/s`, `drag-back 175.0 / 350 mm`, the bar, and the on-table
  `pull N mm` label were all transcribed correctly in `01`, `04`, `05`, `06`, `11`, `12`,
  `15`; the vision pass independently noticed the tip-to-ball gap grows on the power ladder
  (`04` 60 mm -> `06` 350 mm) and that the anchor label travels with it.
- **The miscue envelope.** `14` read as *"the panel makes the shot's rejection semantically
  very clear"*, with the red dot, the red status line, and the sentence naming the overage;
  `13` read as *"legal, not rejected"* and *"exactly on the miscue-limit envelope"*. The
  boundary case reads as intended once the tolerance bug was fixed.
- **The spin axes.** After the handedness fix, `07` reads the dot *below* the crosshair with
  a horizontal spin axis, `08` reads it *above*, `09` reads it *to the right* with
  `+2.32 tr right english`, `10` *to the left*. The 3D marker does communicate the contact
  point — given the crosshair as the reference.
- **Elevation, numerically.** `02` and `12` both report the gauge label and the read-out
  agreeing on 70 deg.

**What it found ambiguous** (and what I do with each):

| Finding | Where | Disposition |
|---|---|---|
| Elevation does not read from the top-down cue; the foreshortened cue "looks like a short, flat 2D stick" | `02`, `11`, `12` | **Accept.** The core finding; §4 makes the labelled side-view gauge mandatory. |
| The elevation gauge "has no scale, tick marks, or direction" and was taken for "another aiming or power affordance" before it had a label | `01`-`03` | **Accept.** Label + arc are the fix; a tick scale is added at the gate (§10.1). |
| The strike card is crowded: tip, dot, crosshair, ring, spin axis overlap, and the shaft partly covers the dot | `07`, `11`, `12`, `15` | **Partly accept.** The crosshair + halo fix landed mid-prototype (the later frames are markedly more readable), but the shaft still crosses the face. Ruled at the gate (§10.2): the card goes face-on, so no shaft can cross it. |
| `a`/`b` and the "across/up the ball face" labels are not self-evident, nor is the sign of `a` | `01`-`05`, `11`, `12` | **Accept.** §3 pins the convention; the widget should print axis glyphs (`R`, `L`, `T`/`D`). |
| The guide cluster "is not immediately obvious without the legend", and the ghost-ball ring is taken for a strike marker | `01`, `03`, `05`, `06`, `12`, `15` | **Accept.** The table-side strike marker does not exist by design (§3) — the ring is the ghost ball — so the legend, and possibly a default-off tangent line, carry it. |
| `pull 175 mm` sits next to the white cue ball and was read as "another ball / drag handle" | `05`, `10` | **Accept.** Move the label onto the cue butt side or draw it as a dimension bracket. |
| "No prominent player-facing power meter" / the panel "reads like a debug overlay" | `04`, `05`, `06` | **Accept, by design.** §8: the prototype's panel is the developer read-out; the shipping HUD is a separate, smaller design. |
| The read-out says "no physics" yet asks to drag again ("somewhat confusing") | `15` | **Rebut.** Artifact framing, not UX: the parenthetical exists so nobody reads the prototype as a simulation. It has no bearing on the shipped flow. |
| "The projected cue is not visibly connected to the cue ball" | `01`, `12` | **Partly accept.** The gap *is* the pull-back, so contact-at-rest is the wrong cue; but the missing at-rest reference is real — the pull label addresses it, a "home" tick would too. |
| `tr` units and `mm/s` need the panel to explain them | `02`, `05` | **Partly accept.** For the dev panel, fine; for the player HUD, show m/s or a qualitative label. |
| "The 2D marker's position and the a/b axes are not obviously related" | `14` | **Rebuttal to the implied fix.** There is no 2D strike marker; the ring near the object ball is the ghost ball. Adding a top-down strike dot would be *wrong* (§3: it collapses draw/follow/centre onto one point) — the widget is the honest home for it. |
| The elevation value in the card cannot be measured (no arc, no scale) | `12` | **Accept.** The card is a *spatial* affordance; the numeric angle stays in the gauge and the read-out. Do not duplicate a protractor into a 40 px widget. |

Every frame's raw answer is reproducible with the command in the prototype README; the
frames are the committed PNGs, so the gate can re-run the same queries.

Provenance of the quotes: 13 of the 15 frames were read on the **committed** render (all of
`01`–`12`, `15`). `13` and `14` were read on the render immediately before it, which differs
only by the widget's camera side-offset and the handedness fix — neither touches the read-out
text, the marker colours, or the status line those two claims rest on. The spin-direction
claims (`07`–`10`) were re-read **after** the handedness fix, on the committed frames.

## 10. Decisions (resolved at #10)

The nine questions this section opened, answered at the gate and recorded in full — rationale, basis and the
measurement that would settle each open item — on #10. They are cited elsewhere in this document as §10.1–§10.9.
#12 merges them as the section's text.

1. **Elevation affordance: the labelled inline side-view gauge, with ticks at 0/30/60/90 and the numeric label.**
   The dedicated side-view inset is rejected (it duplicates what the gauge carries, for pixels the table does not
   have) and numeric-only is rejected (invisible in the flow).
2. **Strike marker: exactly face-on.** The ball face is viewed along the cue axis, no shaft crosses it, and elevation
   is not drawn in the card — the gauge owns elevation. This overturns the prototype's near-face-on 3/4: its
   readability is elevation-dependent (the shaft obscures the face at 45°).
3. **Axis glyphs: `R`/`L` and `T`/`D` on the face edges.** `a > 0` = the shooter's right; `b > 0` = above centre (the
   follow side at zero elevation); `(a, b)` live in the plane perpendicular to the cue axis, with `b` the world-up
   direction projected into it. Elevation is capped at 75°: the frame degenerates at 90°, and a vertical cue is a
   jump stroke, which the physics does not model.
4. **Envelope: visible rejection with a refused commit.** The limit test is on the tip offset in millimetres,
   `|offset_mm| <= rho_max + 1e-3` (≈ 6.8e-5 in the envelope-fraction unit of §7) — exactly at the limit is legal. Rejection is authoring-time only: it never reaches the rules layer, and with no
   shot clock it cannot cost a turn.
5. **Envelope vs elevation: the pure friction cone, one radius at every elevation.** The sim never simulates a
   miscue, so there is no sim-side model to contradict; the known physical direction (a downward offset gains, an
   upward one loses, plus a shaft-clearance limit) is recorded as a deviation with the evidence that would settle it.
6. **Unit: fraction of the miscue envelope** (dimensionless, `1.0` = the limit) — §7, pinned in the schema.
7. **Power: release-to-commit stays** (`Enter` commits the same declaration, `Escape` abandons a drag), and the linear
   pull → speed map is **replaced by a convex one**: the 0–1500 mm/s band must occupy at least 40 % of the pull range
   (≥ 140 mm of 350 mm), because at ≈1.6 mm/px a linear map costs 4–10 % of a soft shot per pixel of drag error. A
   candidate that meets it: `v = 7000 × (pull / 350)²`. The exponent is a playtest constant; the requirement is spec.
8. **Guides: aim line + ghost ball + object path by default; the tangent line is default-off** behind a toggle;
   arrowheads on the departure lines only (object path, tangent); the legend stays on whenever guides are drawn. Once
   `#7` §3's squirt model exists, the aim line and ghost ball follow the **squirt-corrected** cue-ball path while the
   cue stick stays on the input aim line; **swerve is not previewed** (a curved path is a sim result, and the preview
   stays a preview).
9. **HUD split: three player lines** — turn/group, the declaration being authored, one-line validity, with speed in
   m/s or a qualitative label — and the **dev panel** behind a flag (offsets in mm and envelope fraction, envelope
   arithmetic, guide geometry, declaration JSON).

Residual uncertainty, recorded rather than hidden: the face-on card drops the 3/4 view's statement of how the cue
frame tilts against the world, and the flow itself has never been felt by a human hand — both are playtest items.

## 11. Handoff

- **#11 (architecture)**: the `input.rs` state machine is exactly §2 (aim-idle -> power-drag
  -> commit) plus §6's refusal path; the strike card is a separate surface that must not
  steal aim events. The declaration the machine emits is already the input-log `declaration`
  entry.
- **#6 (rules)**: rejection never reaches the rules layer (#7 §4 calls it an input error), so
  no adjudication is produced for it; confirm the machine's `(state, input)` contract has an
  equivalent "declaration refused before submission" path.
- **#9 (AI)**: the same `(a, b, elevation)` envelope bounds the action space, so §6's
  tolerance and §7's unit decision apply to the policy's output validation too.
- **#12 (spec assembly)**: merge as the cue-input/UX section; §3's sign convention and §7's
  unit must also land in the `spin` description of `docs/spec/input-log.schema.json`.
- **Prototype lifecycle**: `prototypes/cue-ux/` is throwaway. The judgement landed at #10: the
  validated decisions are folded in as §10, and the prototype comes off the branch when #12 merges
  the section (the frames are the primary source).
