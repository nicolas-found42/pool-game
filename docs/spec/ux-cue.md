# Cue input and shot-authoring UX

Status: **draft spec section, pending the user's judgement at the #10 gate.** This is the
UX/controls section the map's "Cue rendering for masse" fog entry was waiting on. Ticket
#12 merges it into the assembled spec; ticket #11 owns the `input.rs` seam it plugs into.
Evidence: the throwaway prototype in `prototypes/`.
Only #7's locked decisions are treated as fixed here (frame, the strike declaration, the
miscue envelope); everything below is a proposal, and §10 lists what the judgement must
settle.

## 1. The authored object

The UX edits exactly the declaration of #7 §4 — `{aim, speed, spin (a, b), elevation}` — and
nothing else. The read-out prints the input-log body verbatim:

```json
{"aim":{"x":+0.900,"y":+0.436},"speed":3500,
 "spin":{"a":+0.00,"b":-2.31},"elevation":0.000}
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
- Because the widget's cue frame tilts with elevation, the widget is also where the
  *masse* insight is visible: at 70 deg a large `b` offset is mostly a horizontal world
  offset, so the same declaration yields a different spin than at 0 deg. Masse is not a
  separate control; it is `elevation` + off-centre `(a, b)`, and the widget is what makes
  that legible.

Sign convention (the prototype's choice, to be pinned in the schema description):
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
   correctly in every frame, including 70 deg. The gauge has no scale/ticks yet, and the pass
   asked for one.
2. **The widget's shaft** angled off the ball, with the widget's cloth plane as the datum.
3. **The projected cue's length** on the cloth (foreshortening), plus the numeric read-out.

Alternatives still on the table (§11): a dedicated side-view *inset* of the cue ball and cue
(replacing the inline gauge) and a scale/tick treatment on the gauge. Both are cheaper than
changing the camera, which #7/#5 have fixed top-down.

## 5. Aim guides

Drawn from the aim ray only (input geometry; no squirt/swerve in the preview): the **aim
line**, the **ghost ball** at first contact, the **object-ball path**, and the **tangent
line** the cue ball leaves on for a stun hit. With no ball on the line, the first **cushion**
and its reflection are shown instead. The read-out states the cut angle and the guide's
first contact.

The vision pass consistently reported the guide cluster as needing the legend and rarely
resolved it without one; every frame that drew the cluster drew the same complaint. Draft
consequences: keep a colour-keyed legend, and reconsider the tangent line's default-on
status (§11) — it is the line most often confused with the ghost ball.

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
hurts an upward one; extreme elevation adds a shaft-clearance constraint) is not settled by
#7 and is an open question (§11) — the prototype pins one behaviour and says so.

## 7. Unit ambiguity to resolve

`#7` §4 says offsets are "in tip radii" but gives the limit as "~0.5 R". Those disagree by
roughly 4.5x for a 12.7 mm tip (0.5 R = 14.29 mm = 2.25 tip radii). The prototype enforces
the limit geometrically in millimetres (14.70 mm) and prints both readings. The schema's
`spin` field needs one unit and one null-hypothesis example, or the AI action space (#9) and
the log will disagree with the sim.

## 8. Read-out split

The prototype's panel is a **development** read-out (every intermediate quantity: aim, speed,
pull, both offsets in tip radii and mm, offset vs envelope, intent, guide, declaration JSON,
status). It is right for the prototype and wrong for the shipping HUD, which needs a
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
| The elevation gauge "has no scale, tick marks, or direction" and was taken for "another aiming or power affordance" before it had a label | `01`-`03` | **Accept.** Label + arc are the fix; a tick scale is open (§11). |
| The strike card is crowded: tip, dot, crosshair, ring, spin axis overlap, and the shaft partly covers the dot | `07`, `11`, `12`, `15` | **Partly accept.** The crosshair + halo fix landed mid-prototype (the later frames are markedly more readable), but the shaft still crosses the face. Open question (§11): pull the shaft out of the face region or flatten the widget to face-on. |
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

## 10. Open questions for the judgement

1. **Elevation affordance**: labelled inline side-view gauge (prototype), a dedicated
   side-view *inset* with the ball and cue, or numeric + foreshortening only? Should the
   gauge gain a tick scale (0/30/60/90)?
2. **Strike marker shape**: near-face-on 3/4 ball (prototype — spin reads, shaft tilt reads
   weakly) vs. exactly face-on (spin perfect, elevation invisible in the card) vs. two
   widgets (face-on ball + elevation gauge)? In a 40 px card, which trade is right?
3. **Axis labelling**: does the widget print `R/L` + `T/D` glyphs on the ball face, and is
   `a > 0 = shooter's right` the convention to pin in the schema?
4. **Envelope behaviour**: visible rejection with a refused commit (prototype) vs. hard clamp
   (offset simply stops at the ring)? Does the rejection ever cost the player a turn in a
   timed/competitive context, or is it purely authoring-time?
5. **Elevation-dependence of the envelope**: keep the pure friction cone (one radius at every
   elevation, prototype), or modulate it with elevation (downward offsets gain, upward lose,
   plus a shaft-clearance limit at high elevation)? This is a #7-physics question the UX
   surfaces but cannot settle.
6. **Unit of `(a, b)`**: tip radii (needs a tip-diameter constant) or fraction of the miscue
   envelope (scale-free, matches what the player sees)? One must go in the schema.
7. **Power mapping**: linear 350 mm -> 7000 mm/s (prototype) or a curve with a finer low end;
   and is release-to-commit right, or should commit be a separate action so a power drag can
   be abandoned?
8. **Guide set**: aim + ghost + object path + tangent (prototype) — is the tangent line worth
   its clutter by default, and should guides have arrowheads? Does the preview owe the player
   a squirt/swerve indication once `#7` §3's squirt model exists?
9. **HUD split**: which quantities are player-facing (turn, group, declaration, validity) vs.
   dev-only (offsets in tr and mm, envelope arithmetic, guide geometry)?

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
- **Prototype lifecycle**: `prototypes/cue-ux/` is throwaway. Once the judgement lands, the
  validated decisions fold into this section and the prototype comes off the branch (the
  frames are the primary source).
