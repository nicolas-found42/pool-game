# 0002 — WPA World Standardized Rules as the rules spine, filtered by machine decidability

Date: 2026-09-11

## Status

Accepted

## Context

The game adjudicates every turn from simulation telemetry, with no referee and no human in the loop. Two questions framed the rules work: which rule text the game answers to, and what to do with the many WPA clauses that exist only because a human official interprets them.

Alternatives considered: a simplified bar/house ruleset (ambiguous by design — every precise rule would have to be invented anyway, with no authority to check against), a bespoke ruleset designed for the game (no parentage, so no reference cases), and verbatim WPA transcription (impossible without a referee).

## Decision

- The **WPA World Standardized Rules** (*Rules of Play*, effective 2025-09-15: §4 8-Ball, General §1–3, and the cited Regulations) are the spine; the spec transcribes them by rule number.
- **Machine decidability is a hard filter.** Rules the input model cannot produce (a shot is aim direction, cue speed, spin offsets, and cue elevation — no stick, no body, no hands) are excluded by construction; rules that require human judgment are dropped. Every exclusion is recorded in a **deviations table** with its rule number and reason.
- A mechanical substitute is invented **only** where dropping a rule would prevent termination or deadlock: stalemate becomes a mutual-agreement declaration (WPA 1.13's by-agreement path with 4.11's remedy), and WPA 1.6 ¶2's spotting provision prevents a deadlocked ball-in-hand position.
- Where the WPA text is silent or ambiguous, the gap is filled from a **named secondary source** (CSI/BCAPL, APA) and recorded as a deviation — no gap ships unspecified.

## Consequences

- The rules section is a transcription plus a deviations table, so future WPA amendments are a diff against a known text.
- Some rules become dead letters in this implementation (double hit, push shot, scooping, foot-on-floor, marking, rack templates, shot clocks, restoration and interference); others are conditional on the physics scope decided in the physics ticket — jump, massé, airborne, and off-the-table rules exist only if the simulation carries vertical ball state.
- Procedures that exist to communicate with an official are replaced by either declarations modelled as inputs (calls, safety, stalemate agreement) or simulation truth (frozen balls): the game is referee-free 8-ball.
