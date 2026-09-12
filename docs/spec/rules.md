# The 8-ball rules layer

Status: spec section, decided at #6 (2026-09-11), merged into the assembled spec by #12. The rack and
break clauses it hands off are in `rules-break.md`; the facts it consumes are in `physics.md` §7; the
machine's inputs are `input-log.schema.json`; the crate that holds it is `architecture.md` §1.
Vocabulary follows `CONTEXT.md`, and every departure from the WPA text has a line in §9 or in
`rules-break.md` §4 — none is silent.

## Scope and sources

This section specifies the rack-scoped rules state machine: what it awaits, what it decides from the
simulation's facts, what it emits, and what it deliberately does not model.

| Source | Version | Used here for |
|---|---|---|
| WPA *Rules of Play* | effective 2025-09-15 | §4 (8-Ball) as the spine; General §1–§3; the cited Regulations |
| WPA *Playing Regulations* | effective 2025-09-15 | referee procedure and frozen-ball declaration, read as simulation truth (§9) |
| CSI *Official Rules* | 2025-08-12 | rule 2-3-3 and 2-10(c), cited only where WPA is silent and always by number |
| APA *Team Manual* | — | the same-stroke loss condition, cited with CSI |

The framing decision is recorded in `docs/adr/0002-wpa-rules-spine-machine-decidability.md`; the
source-by-source fact sheet is the asset on #6.

## 1. Machine shape

The rules layer is a pure function `(state, input) → (adjudication record, state')`. Every input is
explicit and logged, so replay is the initial rack plus the input log (`architecture.md` §6). It is
rack-scoped; a thin match layer wraps it (§8).

| State | Awaits | Input |
|---|---|---|
| `AwaitingPlacement{domain}` | cue-ball coordinates; optionally a 1.6 ¶2 spot request | `domain ∈ {AboveHeadString, Anywhere}` |
| `AwaitingShot{shooter, target}` | one atomic shot declaration | `{call, aim, speed, spin, elevation}`; on the break the call is `Break` (nothing called) |
| `AwaitingChoice{chooser, options}` | one option | break-foul tree, illegal-break tree, 8-on-break tree (owner varies, `rules-break.md` §3), stalemate offer |
| `InFlight` → `Adjudicating` | the simulation's fact stream | internal |
| `RackOver{winner \| drawn}` → `MatchOver{winner}` | — | — |

A shot is **one atomic declaration** — there is no separate call state — and a *safety* is a call type,
unavailable on the break (4.3(b)). The declaration is also the AI's action space (`ai.md` §2) and the
UX's authored object (`ux-cue.md` §1).

The variant seam is a trait over the same `(state, input) → (record, state')` shape: the machine is
8-ball's, and a future discipline replaces the machine, not the crate graph.

## 2. Targets and the open-table claim (4.4)

`target ∈ {Open, Group, OnTheEight}`. While the table is open, any object ball may be struck first
**except the 8**; striking the 8 first is a foul (3.2) **unless a group is already completely pocketed
and the shooter claims it**.

- **The claim is carried by the call**: calling the 8 while the table is open and exactly one group is
  completely pocketed claims that group for the shot and makes the 8 the target — pocketing it legally
  wins the rack (4.4's "possibly for a win"). Claim ⟺ calling the 8; there is no separate flag.
- A failed claim shot passes the turn and leaves the table open; the opponent may claim in turn.
- Degenerate case, specified rather than left undefined: if *both* groups are completely pocketed while
  the table is open (reachable when balls go down on fouls or uncalled shots), either group may be
  claimed; the shooter is on the 8.
- Assignment happens only by legally pocketing the **called** ball (4.4), never on a safety.

## 3. Legal shot, fouls, tie-breaks

**Legal shot** (3.2/3.3): first contact is a legal ball for the target; if nothing is pocketed, at least
one ball is driven to a rail after contact.

- **Driven to a rail** (2.7) is a per-ball predicate. A ball frozen to a rail does not count unless it
  leaves and returns; a ball pocketed or driven off the table counts. The simulation carries the two
  facts that decide the frozen exception as first-class rail-contact fields (§10.2); the frozen
  predicate itself is one stated tolerance for both rail and ball frozen status (`physics.md` §5).
- **The physical rail count and the 2.7 count are different numbers.** The simulation reports the
  *physical* count — object balls that actually touched a rail, excluding balls pocketed or driven off
  the table. Rule 2.7 (and 4.3(d) on the break) then *adds* pocketed and off-table balls. Corpus fields
  named `distinct_object_balls_to_rails` are the physical count unless the entry says otherwise
  (`physics.md` §8), and the rules layer must never compare the two without saying which it means.
- **Continuation is decided by the called ball** (read with 4.4/4.6). WPA 4.5's looser phrasing ("balls
  of his group are pocketed legally") is read as the called ball; recorded as a clarification.
- **Simultaneity.** The simulation reports a deterministic total order plus **simultaneity groups**
  (contacts it cannot order within ε — exact ties, pre-existing frozen chains). Within a group the rules
  layer resolves in favour of legality: legal ball assumed first (3.2), legal-ball/cushion tie (3.3), and
  any undecidable determination defaults to legal (Reg 25).
- **Multi-foul:** evaluate every foul on the shot, enforce the single most serious (3 chapeau). Severity
  order here: loss of rack (4.8) > standard foul; all standard fouls carry the same penalty.

| Rule | Foul | Machine evidence |
|---|---|---|
| 3.1 | cue ball pocketed or off the table | pocket/off-table event naming the cue ball |
| 3.2 | wrong ball first | first legal contact not a legal target (claim-aware) |
| 3.3 | no rail after contact | contact occurred, nothing pocketed, no rail contact after it |
| 3.5 | ball driven off the table | off-table event |
| 3.9 | balls still moving | structural — one impulse per shot; the shot ends at the stop condition |
| 3.10 | bad cue-ball placement | placement validation, on/below the line while restricted |
| 3.11 | bad play from above the head string | first contact above the line and the cue ball never crossed it |
| 3.4, 3.6, 3.7, 3.8, 3.12, 3.14, 3.15 | foot on the floor, touched ball, double hit, push shot, out of turn, slow play, rack template | adopted by 4.9 but unreachable in this input model — excluded by construction (no body, hands, stick, turn violations, clock, or template) |
| 3.13 | three consecutive fouls | **not applicable** — WPA itself excludes it from 8-ball |
| 3.16 | unsportsmanlike conduct | dropped — no conduct, no referee discretion |

## 4. Break (4.3)

The break is an ordinary shot except where 4.3 says otherwise: cue ball in hand above the head string,
no call, no required first contact. `rules-break.md` §3 owns the classification precedence, the option
trees, the acceptance semantics, and the re-rack transition; the summary is:

| Situation | Options (and whose) |
|---|---|
| Legal break (4.3(c)/(d)) | a ball pocketed, or **≥ 4 distinct object balls** driven to one or more rails — a per-ball predicate, not a contact count. Nothing pocketed: the turn passes, table open |
| Illegal break (nothing pocketed, < 4 to rails) | **no ball-in-hand**; *incoming player*: accept in position / re-rack & break / re-rack and the offender re-breaks (4.3(d)) |
| Illegal break that is also a foul | illegal-break tree governs (CSI 2-3-3; WPA silent) |
| Break foul, cue ball pocketed or driven off the table (scratch) | *incoming player*: accept in position, or cue ball in hand **above the head string** (4.3(f)(g)(h)) — never anywhere |
| 8 pocketed, legal break | *breaker*: spot the 8 and accept balls in position, or re-break (4.3(e)) — not a win |
| 8 pocketed on a foul break | *opponent*: spot the 8 and play with cue ball in hand above the line, or re-break (4.3(f)) |
| Object ball driven off the table on the break | foul (3.5); the ball stays off except the 8, which is spotted (4.3(g)); *incoming player*: accept in position or cue ball in hand above the line |
| Any other break foul | *incoming player*: accept in position or cue ball in hand above the head string (4.3(h)) |

**Spotting (4.7): only the 8 is ever spotted, and only from the break.** The one exception in the whole
game is 1.6 ¶2's deadlock provision (§6), which spots the nearest legal object ball. The *position* is a
total, deterministic algorithm (1.5), not the implementer's choice:

1. Place the ball on the long string, between the foot spot and the foot rail, as close as possible to
   the foot spot, without moving any ball.
2. If the foot-spot position is unavailable, place it **in contact** (if possible) with the interfering ball.
3. Never place it in contact with the cue ball — a small separation must be maintained.
4. If the whole stretch of the long string below the foot spot is blocked, spot it **above** the foot
   spot, as close as possible to it.

The spot happens before the next shot is played. Two machine-precision points, recorded as
clarifications because the human text is not quantitative: *in contact* means gap ≈ 0 within the
simulation's contact tolerance, and *a small separation* means at least a spec constant δ pinned by that
same tolerance; where clauses 2 and 3 collide, the cue-ball separation wins and the next valid position
is taken. `architecture.md` §9 assigns the search order to `pool-rules` and the geometry predicates to
`pool-sim`.

## 5. The 8-ball phase and losing the rack (4.8)

The shooter is `OnTheEight` when his group is completely pocketed or claimed. **Loss conditions** (4.8,
exhaustive, and none applies to the break): (a) 8 pocketed plus any foul; (b) 8 pocketed before his group
is cleared; (c) 8 pocketed in an uncalled pocket; (d) 8 driven off the table.

- Pocketing the 8 on the same stroke as the last group ball is an explicit **loss** (CSI 2-10(c)/APA;
  WPA is silent — deviation).
- Fouling while shooting the 8 **without** pocketing it is a standard foul only — the 8 stays (CSI
  2-9-2; WPA implies it by omission from 4.8).
- **Win:** the 8 legally pocketed in its called pocket with the group cleared.

## 6. Ball in hand and placement (1.6, 3.10, 3.11)

Domains are `Anywhere` (standard foul) and `AboveHeadString` (rack start, break fouls). The rules layer
validates submitted coordinates: inside the playing surface, no overlap with any ball, in-domain — a ball
*on* the line is not above it (2.13). Restricted placements keep 3.11 (first contact must not be a ball
above the line unless the cue ball crossed it first). **1.6 ¶2 deadlock provision kept:** with a
restricted placement, if every legal object ball is above the line, the shooter may require the nearest
legal ball to be spotted (ties by his designation; a ball resting on the line is playable). A validated
placement is the only legal way a player affects a stationary ball — that replaces 3.6.

## 7. Stalemate (1.13/4.11) — a declaration, not a detection

Either player may propose; on **mutual agreement** the rack is re-racked (same rack seed, so the same
positions) and the **original breaker** breaks again. The referee-mediated path (observe no progress,
then three turns each) is dropped as a judgment rule, and there is **no automatic detector and no
shot-count constant**: a re-rack needs the opponent's consent, so a player who is behind cannot
unilaterally reset. Training/evaluation termination is the harness's step cap — a truncation, outside
the rules layer. The AI seat's policy is to accept and never propose (`ai.md` §8).

## 8. Match layer

Race to **5 racks**, breaks alternating, first breaker drawn from the match seed. Concession (1.12), shot
clocks (3.14/Reg 18), and time-outs (Reg 14) are dropped: no referee, and nothing in a hot-seat or AI
game stalls. The match layer owns the race target, the breaker, and per-rack seed derivation
(`architecture.md` §1, `rules-break.md` §2.7).

## 9. Deviations table (spec-ready)

| WPA | Deviation | Reason |
|---|---|---|
| 1.7/4.6, call "if not obvious" | explicit call on every non-break shot | "obvious" is a referee judgment |
| 2.7/3.7/Reg 26 frozen, "assumed not frozen unless declared" | simulation truth | exact geometry; the declaration protects against unmeasurable gaps |
| 3 chapeau, uncalled foul never happened | moot | auto-detection |
| 1.13 referee-mediated stalemate | mutual-agreement declaration | no referee |
| 1.3 alternate breaks (names 9-ball) | adopted for 8-ball | WPA's direction for rack-scored disciplines |
| 4.3(d)+(h) illegal break with foul | CSI 2-3-3 precedence | WPA silent |
| 4.8 same stroke as last group ball | explicit loss (CSI 2-10(c)/APA) | WPA silent |
| 4.5 continuation wording | read as the called ball (4.4/4.6) | WPA phrasing is looser than its call rules |
| 3.4/3.6/3.7/3.8/3.12/3.14/3.15, 2.11, 3.16, 1.4 | dropped | unreachable or judgment-only |
| 2.2 five-second hang, 1.8 settling | moot | balls do not fall by themselves in a deterministic sim; the supported-ball predicate is kept |
| 1.10/1.9/Reg 11, 1.12, 1.11, Reg 23 | dropped | no external actors, no official, no appeals |
| 2.9 jump, 3.5 off table, 4.8(d) | **live** — the simulation carries full 3D ball state (`physics.md` §1) | resolved by #7/#8: the conditional row is settled in favour of the 3D model |

The rack and break deviations (4.2 uniformity, 4.3(d)/(e)/(f) ordering, 1.5's quantitative readings) are
in `rules-break.md` §4.

## 10. Observation contract

The rules layer can only adjudicate what the simulation reports. Required facts (`physics.md` §7 gives
the event kinds, the fields each kind carries, and the count semantics; `architecture.md` §4.5 gives
their container and stamps):

1. **Ordered contact stream** — every ball–ball, ball–cushion, ball–jaw, ball–pocket event with
   participants and the cue's impulse at t0, in a deterministic total order, with **simultaneity groups**
   for contacts the sim cannot order within ε (§3).
2. **Per-ball rail contacts, each carrying two first-class fields**: `frozen_at_shot_start` (was this
   ball in contact with *this* rail in the state at shot start) and `left_since_shot_start` (has the ball
   separated from this rail at any time since shot start). A contact counts toward "driven to a rail" iff
   `¬frozen_at_shot_start ∨ left_since_shot_start`. This is a field, not a convention: the frozen
   exception is unjudgeable from a bare contact list.
3. **Pocket events carrying the pocket identity** — called-pocket checks need *which* pocket.
4. **Off-table events.**
5. **Stop condition** — zero linear **and** angular velocity; a shot ends only then (2.19), and spinning
   counts as moving (3.9).
6. **Rest state** — positions and spin of every ball after the shot.
7. **Rest-state annotation** — the 2.2 supported-ball predicate: for every ball at rest whose centre lies
   inside a pocket mouth without having dropped, `supported_over_mouth` with the supporting ball ids. The
   simulation leaves the ball in place; the rules layer counts it as pocketed (`CONTEXT.md`'s
   *Pocketed*). This is the only route by which "pocketed" arises without a drop event.
8. **Pre-shot state** — ball array and placement geometry, for frozen and head-string predicates.

Plus table geometry (playing-surface bounds, head string, foot spot, pocket mouths) and deterministic
rack positions (`rules-break.md` §2).

Also required, input-side: a **deterministic placement-construction primitive** — an exact-contact
position against a named ball, and a position at a specified separation, both overlap-free and
reproducible — used by spotting (§4) and by ball-in-hand validation (§6).

## 11. Handoffs

- **Physics (`physics.md`):** the contract above is satisfied by the fact stream of §7 there; the 3D
  scope settles the last deviations row.
- **Architecture (`architecture.md`):** rules layer as a pure function with an input log; rack-scoped
  core plus thin match layer; the variant seam; determinism of contact order and the simultaneity ε.
- **AI (`ai.md`):** the shot declaration is the action-space skeleton — `{ball, pocket} | safety` plus
  aim/speed/spin/elevation — and legality masking comes from this layer, never from the policy.
- **Cue UX (`ux-cue.md`):** the call model is the shot-authoring input — a suggested call the player
  confirms.
- **Corpora:** `rules-break.json` asserts this machine's records branch by branch; its shape and
  assertion depth are specified in `rules-break.md` §"Corpus contract".
