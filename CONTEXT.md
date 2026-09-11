# pool-game

Domain vocabulary for the top-down 8-ball pool game. The rules vocabulary follows the WPA World Standardized Rules — the spine of the rules layer — and the terms below are the ones the spec, tickets, and code use.

## Language

### Table

**Cue ball**:
The ball struck by the player's cue; the only ball a player may place or move by hand.
_Avoid_: white ball, CB

**Object ball**:
Any of the 15 numbered balls other than the cue ball.
_Avoid_: coloured ball, numbered ball

**Head string**:
The line bounding the quarter of the table nearest the head rail. "Above the head string" means between that line and the head rail, *not including the line*; a ball is on the line when its centre is directly over it.
_Avoid_: kitchen

**Foot spot**:
The point on the long string one quarter of the table length from the foot cushion, where the rack's apex ball is placed. It is the only spot this game uses.

**Pocketed**:
A ball that has crossed into a pocket. A ball resting on another ball over a pocket mouth such that removing the support would drop it also counts as pocketed.

**Frozen**:
Balls at rest in contact. Unlike WPA's declared procedure, frozen status here is read from the simulation, not declared by a player.

### Match and rack

**Rack**:
One game of 8-ball: the 15-ball triangle, the break, and the play that follows until the rack is won, lost, or stalemated. A match is a race of racks.
_Avoid_: game (ambiguous with the whole product)

**Breaker**:
The player who plays a rack's break shot. For a rack that stalemates, the *original breaker* breaks again.

**Break**:
A rack's first shot, played with the cue ball in hand above the head string. No ball is called on the break.

**Open table**:
The state before either group is assigned. Any object ball may be struck first except the 8; the table stays open until a called ball is legally pocketed.

**Group**:
One of the two sets of seven object balls (1–7 and 9–15). A shooter's group must be completely pocketed before the 8 becomes his target.

**Temporary claim**:
While the table is open and one group is already completely pocketed, calling the 8 asserts that group for the shot, making the 8 the target — possibly for a win.

**Stalemate**:
A rack abandoned by mutual agreement of both players; it is re-racked and the original breaker breaks again.

### Shot

**Shot**:
The span from the cue's impulse on the cue ball until every ball has stopped moving *and spinning*.

**Turn**:
A player's time at the table: from when he is entitled to shoot until he no longer is.
_Avoid_: inning

**Call**:
The declaration a shot carries: a ball plus a pocket, a safety, or — on the break — nothing. Required on every shot except the break; the shot's details (cushions, kisses, other balls pocketed) are irrelevant.
_Avoid_: nomination

**Safety**:
A call that passes the turn at the end of the shot. Balls pocketed on a safety stay down, and no group can be established.

**Legal shot**:
A shot on which no foul occurred.

**Foul**:
A standard foul: a listed violation whose penalty is cue ball in hand anywhere on the playing surface. Only the most serious foul on a shot is enforced.

**Loss of rack**:
The four conditions (WPA 4.8) under which the shooter loses the rack outright. They outrank any standard foul, and none applies to the break.

**Ball in hand**:
The incoming player's right to place the cue ball — anywhere on the playing surface after a standard foul, or above the head string after a break foul. The rules layer validates the placement.

**Driven to a rail**:
Per-ball predicate for a legal shot: the ball touched a cushion after contact. A ball frozen to a rail does not count unless it leaves and returns; a ball pocketed or driven off the table counts.

**Spotting**:
Placing a ball on the foot spot. Only the 8 is ever spotted, and only from the break.

### Adjudication

**Rules layer**:
The rack-scoped state machine that turns a shot declaration plus the simulation's facts into an adjudication record and the next state. It is the only place that knows rules vocabulary.

**Simulation**:
The headless, deterministic physics crate: ball motion and contacts only. It reports facts and never knows about fouls, groups, or turns.

**Adjudication record**:
The verdict the rules layer emits for one shot: the foul reasons with the facts they were decided from, the balls pocketed, the state transition, and the terminal result when there is one.

**Observation contract**:
The set of facts the simulation must expose for adjudication to be possible at all: contacts in order, pocket and off-table events, the resting state, and the state at shot start.

**Deviation**:
A departure from the WPA text, listed with its rule number and reason. Deviations are never silent.

### Architecture

**Session**:
The drive loop that owns the rules machine, the simulation, the seeded streams, and the input log. Both the app and the headless harness drive it, so a match runs identically wherever it executes.

**Match layer**:
The thin layer above the rack-scoped rules machine that sequences racks and owns match bookkeeping: the race target, the breaker, and per-rack seed derivation.
_Avoid_: game layer, match state machine

**Input log**:
The recorded free-choice sequence of a match — placements, declarations, spot requests, and option picks — from which the match replays exactly. Everything else (racks, adjudications, event logs, scores) is derived; no wall-clock value appears in it.

**Execution noise**:
The seeded per-parameter perturbation applied to a policy's committed declaration for a difficulty level. Human declarations carry none.
