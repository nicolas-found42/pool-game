//! The `Session` drive loop (`architecture.md` §8): the one place where the rules machine, the
//! simulation, the noise stream, and the input log meet.
//!
//! Both binaries use this loop — there is no second implementation to drift — and because the loop
//! consumes the log's own vocabulary and nothing else, the app and the harness produce byte-identical
//! games from identical logs (`architecture.md` §8, checked by §11's golden replays).
//!
//! The loop's order is the spec's: validate the input against the current state, apply execution noise
//! to a policy declaration, simulate the shot to rest, adjudicate, append the log entry, return the
//! record. A request the machine refuses leaves the session exactly as it was.

use std::fmt;

use pool_rules::facts::Observation;
use pool_rules::offer::OptionId;
use pool_rules::record::{Action, Adjudication};
use pool_rules::state::{Group, Player, RulesState, Winner};
use pool_rules::{Input, PlacementDomain, Rack, Vec2};
use pool_sim::constants::BALL_RADIUS_MM;
use pool_sim::rack;
use pool_sim::state_hash::state_hash_hex;
use pool_sim::strike::{StrikeDecl, StrikeError};
use pool_sim::{BallState, Profile, Shot, Sim, Table};

use crate::log::{Declaration, Entry, FORMAT_VERSION, InputLog};
use crate::match_layer::{MatchConfig, breaker_of, rack_seed};
use crate::noise::Noise;

/// One request to the session: exactly the five kinds the input log carries (`architecture.md` §6),
/// because a request the log cannot express could never be replayed.
///
/// The stalemate agreement of `rules.md` §7 is one of them — the agreement is the input, not a shot,
/// so the log carries it as its own entry kind and a match re-racked by agreement replays like any
/// other. The re-rack that follows is the stalemate tree's single option, an ordinary option request.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// A cue-ball placement within the domain the state awaits (`rules.md` §6).
    Placement {
        /// The domain the placement claims.
        domain: PlacementDomain,
        /// The cue ball's centre (mm, table frame).
        pos: Vec2,
    },
    /// The 1.6 ¶2 request: spot the legal object ball nearest the head string (`rules.md` §6).
    SpotRequest,
    /// One atomic shot declaration, `from_policy` included (`architecture.md` §6).
    Declaration(Declaration),
    /// One option of the tree the state presents, by its corpus id (`rules-break.md` §3).
    Option {
        /// The option id, as `rules-break.json` writes it.
        option_id: String,
    },
    /// The stalemate agreement (`rules.md` §7): the input is the agreement itself.
    Stalemate,
}

impl Request {
    /// The request's kind, as the log's entry tags read.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Placement { .. } => "placement",
            Self::SpotRequest => "spot_request",
            Self::Stalemate => "stalemate",
            Self::Declaration(_) => "declaration",
            Self::Option { .. } => "option",
        }
    }

    /// The request a log entry carries — the only direction that exists: the log is the free-choice
    /// sequence, so every request in a replay comes from an entry.
    #[must_use]
    pub fn from_entry(entry: &Entry) -> Self {
        match entry {
            Entry::Placement { domain, pos } => Self::Placement {
                domain: *domain,
                pos: *pos,
            },
            Entry::SpotRequest => Self::SpotRequest,
            Entry::Stalemate => Self::Stalemate,
            Entry::Declaration(declaration) => Self::Declaration(declaration.clone()),
            Entry::Option { option_id } => Self::Option {
                option_id: option_id.clone(),
            },
        }
    }
}

/// Why a request was refused (`architecture.md` §4's input-boundary policy: input problems are
/// `Result`s; a violated invariant is a bug).
#[derive(Debug, Clone, PartialEq)]
pub enum InputError {
    /// The state does not await this request kind.
    NotAwaited {
        /// What the state awaits.
        awaited: &'static str,
        /// What arrived.
        request: &'static str,
    },
    /// The rules machine refused the input, in its own vocabulary.
    Machine(pool_rules::InputError),
    /// The option id is not one the rules vocabulary defines.
    UnknownOption(String),
    /// The declaration is not a legal strike (`physics.md` §4's miscue envelope and input domains).
    Strike(StrikeError),
    /// The log names a profile the caller did not load.
    ProfileMismatch {
        /// The id the log records.
        log: String,
        /// The id of the profile handed in.
        profile: String,
    },
    /// The log ended with the match unfinished: an input log is a whole match (§7).
    Unfinished {
        /// What the state awaits at the log's end.
        awaited: &'static str,
    },
    /// The rules layer asked the simulation for something the simulation refused: a bug in one of the
    /// two, surfaced as an error rather than swallowed.
    Invariant(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAwaited { awaited, request } => {
                write!(f, "the state awaits {awaited}, not a {request}")
            }
            Self::Machine(pool_rules::InputError::NotAwaited { awaited, input }) => {
                write!(f, "the state awaits {awaited}, not a {input}")
            }
            Self::Machine(pool_rules::InputError::DomainMismatch { awaited, given }) => write!(
                f,
                "the state awaits a {awaited:?} placement; the request claims {given:?}"
            ),
            Self::Machine(pool_rules::InputError::Placement(fault)) => {
                write!(f, "the placement was rejected: {fault:?}")
            }
            Self::Machine(pool_rules::InputError::UnknownOption(option)) => {
                write!(f, "option {option:?} is not one of the pending tree's")
            }
            Self::Machine(pool_rules::InputError::SpotRequestUnavailable) => write!(
                f,
                "the 1.6 ¶2 request is not available: a legal object ball is below the head string"
            ),
            Self::Machine(pool_rules::InputError::NotTheBreak) => {
                write!(f, "the call does not match the rack's break")
            }
            Self::UnknownOption(option) => {
                write!(f, "option id {option:?} is not in the rules vocabulary")
            }
            Self::Strike(StrikeError::Miscue { norm, limit }) => write!(
                f,
                "the strike is past the miscue envelope: |(a, b)| = {norm} > {limit}"
            ),
            Self::Strike(other) => write!(f, "the strike is not legal: {other:?}"),
            Self::ProfileMismatch { log, profile } => write!(
                f,
                "the log names profile {log:?}, but profile {profile:?} was loaded"
            ),
            Self::Unfinished { awaited } => write!(
                f,
                "the log ends with the match unfinished: the state awaits {awaited}"
            ),
            Self::Invariant(message) => write!(f, "internal invariant: {message}"),
        }
    }
}

impl std::error::Error for InputError {}

/// The session's observable state (`architecture.md` §8): the machine's state plus the match-level
/// values a shell renders and the rules state cannot answer.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionView {
    /// What the machine awaits.
    pub state: RulesState,
    /// The rack's index in the match, from 0.
    pub rack_index: u32,
    /// The rack's breaker (`rules.md` §8's alternation).
    pub breaker: Player,
    /// The group assignment (4.4), once a called ball has been pocketed legally: `[P1, P2]`.
    pub assignment: Option<[Group; 2]>,
    /// Racks won, `[P1, P2]`.
    pub race: [u8; 2],
    /// Racks needed to win the match.
    pub race_target: u32,
    /// The match's winner, once the race is decided.
    pub match_winner: Option<Player>,
    /// The current rack's shot counter (reset by a re-rack, `rules-break.md` §3.7).
    pub shot_count: u32,
}

/// A replayed match's outcome (`architecture.md` §8/§11): everything the golden replays pin,
/// recomputed from the log and never read back from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    /// The adjudication record of every entry, in order.
    pub records: Vec<Adjudication>,
    /// The final position's `state_hash` (`architecture.md` §11).
    pub final_state_hash: String,
    /// Racks won, `[P1, P2]`.
    pub race: [u8; 2],
    /// The match's winner.
    pub winner: Player,
    /// The racks the match played, re-racks included in the rack they belong to.
    pub racks: u32,
    /// The shots simulated, one per declaration entry.
    pub shots: u32,
}

/// The session's own state: the machine, the simulation, both seeded streams, and the input log
/// (`architecture.md` §8).
#[derive(Debug, Clone)]
pub struct Session {
    config: MatchConfig,
    rack: Rack,
    sim: Sim,
    rack_index: u32,
    race: [u8; 2],
    match_winner: Option<Player>,
    noise: Noise,
    log: InputLog,
    /// The current position's ball states, in the canonical order: the cue ball's placement, a
    /// spotted ball's new position, and a shot's rest block are the only writes, so this is the
    /// simulation's state read back without an accessor.
    positions: [BallState; 16],
    /// The last compute shot, for the shell's playback and the harness's event dump.
    last_shot: Option<Shot>,
}

impl Session {
    /// A fresh match: rack 0 racked from `rack_seed(match_seed, 0)`, its breaker drawn from the match
    /// seed, an empty log whose header is the configuration's (`architecture.md` §6).
    #[must_use]
    pub fn new(config: MatchConfig) -> Self {
        let (rack, sim, positions) = Self::rack_at(&config, 0);
        let log = InputLog {
            format_version: FORMAT_VERSION,
            profile: config.profile.id.clone(),
            match_seed: config.match_seed,
            noise_seed: config.noise_seed,
            difficulty: config.difficulty.clone(),
            race_target: config.race_target,
            entries: Vec::new(),
        };
        Self {
            noise: Noise::new(config.noise_seed),
            config,
            rack,
            sim,
            rack_index: 0,
            race: [0; 2],
            match_winner: None,
            log,
            positions,
            last_shot: None,
        }
    }

    /// Rack `index`'s machine and simulation, both from the same generated snapshot (`rules-break.md`
    /// §2.6/§2.7) — the one place a rack is constructed.
    fn rack_at(config: &MatchConfig, index: u32) -> (Rack, Sim, [BallState; 16]) {
        let seed = rack_seed(config.match_seed, index);
        let snapshot = rack::generate(seed);
        let sim = Sim::new(Table::with_profile(config.profile.clone()), snapshot);
        let positions = snapshot.rest_states();
        (
            Rack::new(seed, snapshot, breaker_of(config.match_seed, index)),
            sim,
            positions,
        )
    }

    /// One request, driven through the loop of `architecture.md` §8.
    pub fn request(&mut self, request: Request) -> Result<Adjudication, InputError> {
        match request {
            Request::Placement { domain, pos } => self.place(domain, pos),
            Request::SpotRequest => self.spot_request(),
            Request::Declaration(declaration) => self.declare(&declaration),
            Request::Option { option_id } => self.choose(&option_id),
            Request::Stalemate => self.stalemate(),
        }
    }

    /// Replay a recorded match (`architecture.md` §6/§8): rebuild the session from the log's header and
    /// run its entries, recomputing every derived value.
    ///
    /// A log is a whole match (`architecture.md` §7): the run must end with the race decided, so a
    /// truncated log is refused instead of reported as a partial match.
    pub fn replay(log: &InputLog, profile: Profile) -> Result<Replay, InputError> {
        if log.profile != profile.id {
            return Err(InputError::ProfileMismatch {
                log: log.profile.clone(),
                profile: profile.id,
            });
        }
        let mut session = Self::new(MatchConfig::from_log(log, profile));
        let mut records = Vec::with_capacity(log.entries.len());
        for entry in &log.entries {
            records.push(session.request(Request::from_entry(entry))?);
        }
        let winner = session.winner().ok_or(InputError::Unfinished {
            awaited: session.rack.state.kind(),
        })?;
        // The log the run recorded must be the log it consumed: entry → request → entry is the one
        // round trip replay rests on, and it is checked rather than assumed.
        if session.log.entries != log.entries {
            return Err(InputError::Invariant(
                "the run recorded a different log than it consumed".to_string(),
            ));
        }
        Ok(Replay {
            records,
            final_state_hash: session.state_hash(),
            race: session.race,
            winner,
            racks: session.rack_index + 1,
            shots: log
                .entries
                .iter()
                .filter(|entry| matches!(entry, Entry::Declaration(_)))
                .count() as u32,
        })
    }

    /// The machine's state plus the match-level values a shell renders (`architecture.md` §8).
    #[must_use]
    pub fn state(&self) -> SessionView {
        SessionView {
            state: self.rack.state.clone(),
            rack_index: self.rack_index,
            breaker: self.rack.breaker,
            assignment: self.rack.assignment,
            race: self.race,
            race_target: self.config.race_target,
            match_winner: self.match_winner,
            shot_count: self.rack.shot_count,
        }
    }

    /// The input log so far: the free-choice sequence a record of this match commits (§6).
    #[must_use]
    pub fn log(&self) -> &InputLog {
        &self.log
    }

    /// The last compute shot, for playback and for the fact-stream dump (`architecture.md` §10).
    #[must_use]
    pub fn last_shot(&self) -> Option<&Shot> {
        self.last_shot.as_ref()
    }

    /// The match's winner, once the race is decided (`rules.md` §8).
    #[must_use]
    pub const fn winner(&self) -> Option<Player> {
        self.match_winner
    }

    /// The current position's hash (`architecture.md` §11): FNV-1a over the bits of the 16 ball
    /// states, the value the replay goldens pin.
    #[must_use]
    pub fn state_hash(&self) -> String {
        state_hash_hex(&self.positions)
    }

    /// The current rack's seed (`rules-break.md` §2.7) — a re-rack never changes it.
    #[must_use]
    pub const fn rack_seed(&self) -> u64 {
        self.rack.rack_seed
    }

    // ---------------------------------------------------------------- the four request kinds

    /// A placement: the machine validates domain and geometry (`rules.md` §6), the simulation places
    /// the cue ball, and the rack's table view follows.
    fn place(&mut self, domain: PlacementDomain, pos: Vec2) -> Result<Adjudication, InputError> {
        let (record, next) = self
            .rack
            .adjudicate(Input::Placement { domain, pos })
            .map_err(InputError::Machine)?;
        let mut sim = self.sim.clone();
        if let Err(error) = sim.place_cue([pos.x, pos.y]) {
            return Err(invariant(format!(
                "pool-sim refused a cue ball placement the rules layer accepted: {error:?}"
            )));
        }
        self.sim = sim;
        self.positions[0] = BallState::at_rest([pos.x, pos.y, BALL_RADIUS_MM]);
        self.commit(next, record, Entry::Placement { domain, pos })
    }

    /// The 1.6 ¶2 spot request: the machine spots the ball, the simulation follows.
    fn spot_request(&mut self) -> Result<Adjudication, InputError> {
        let (record, next) = self
            .rack
            .adjudicate(Input::SpotRequest)
            .map_err(InputError::Machine)?;
        self.commit(next, record, Entry::SpotRequest)
    }

    /// The stalemate agreement (`rules.md` §7): the machine's fifth input, accepted in
    /// `AwaitingShot`. The agreement is the input, not a shot; the re-rack it applies is the
    /// stalemate tree's single option, taken as an ordinary option request.
    fn stalemate(&mut self) -> Result<Adjudication, InputError> {
        let (record, next) = self
            .rack
            .adjudicate(Input::Stalemate)
            .map_err(InputError::Machine)?;
        self.commit(next, record, Entry::Stalemate)
    }

    /// One option of the pending tree (`rules-break.md` §3): the id is resolved against the rules
    /// vocabulary first, so an unknown token names itself in the error.
    fn choose(&mut self, option_id: &str) -> Result<Adjudication, InputError> {
        let option = OptionId::by_name(option_id)
            .ok_or_else(|| InputError::UnknownOption(option_id.to_string()))?;
        let (record, next) = self
            .rack
            .adjudicate(Input::Option(option))
            .map_err(InputError::Machine)?;
        self.commit(
            next,
            record,
            Entry::Option {
                option_id: option_id.to_string(),
            },
        )
    }

    /// One shot: noise for a policy seat, the simulation to rest, the machine's record.
    fn declare(&mut self, declaration: &Declaration) -> Result<Adjudication, InputError> {
        // The machine's own two preconditions, checked before the shot is simulated: an input that is
        // not awaited must not cost a shot's physics, and a refused request must leave the session
        // untouched.
        let RulesState::AwaitingShot { .. } = self.rack.state else {
            return Err(InputError::NotAwaited {
                awaited: self.rack.state.kind(),
                request: "declaration",
            });
        };
        if matches!(declaration.call, pool_rules::Call::Break) != (self.rack.shot_count == 0) {
            return Err(InputError::Machine(pool_rules::InputError::NotTheBreak));
        }

        let effective = if declaration.from_policy {
            self.noise.perturb(
                self.config.difficulty.level,
                &declaration.shot_declaration(),
            )
        } else {
            declaration.shot_declaration()
        };
        let strike = StrikeDecl {
            aim: [effective.aim.x, effective.aim.y],
            speed_mm_s: effective.speed,
            spin: [effective.spin.a, effective.spin.b],
            elevation_rad: effective.elevation,
        };
        strike.validate().map_err(InputError::Strike)?;

        // A trial copy, so a shot the machine then refuses leaves the session where it was.
        let mut sim = self.sim.clone();
        let shot = sim.strike(strike).map_err(InputError::Strike)?;
        let observation =
            Observation::from_sim(shot.events(), shot.rest(), self.rack.table.clone());
        let (record, next) = self
            .rack
            .adjudicate(Input::Shot {
                declaration: effective,
                observation: Box::new(observation),
            })
            .map_err(InputError::Machine)?;

        self.sim = sim;
        self.positions = shot.rest().states;
        self.last_shot = Some(shot);
        self.commit(next, record, Entry::Declaration(declaration.clone()))
    }

    // ---------------------------------------------------------------- bookkeeping

    /// Commit an accepted input: the spots it produced, the machine's next rack, the log entry, and —
    /// when the rack ended or re-racked — the match's own transitions.
    fn commit(
        &mut self,
        next: Rack,
        record: Adjudication,
        entry: Entry,
    ) -> Result<Adjudication, InputError> {
        if record.apply.action == Action::ReRack {
            // `rules-break.md` §3.7: the snapshot restored, the seed unchanged, the counter reset.
            // The simulation is rebuilt from the same snapshot, so the two agree again.
            self.sim = Sim::new(
                Table::with_profile(self.config.profile.clone()),
                next.snapshot,
            );
            self.positions = next.snapshot.rest_states();
        }
        self.apply_spots(&next)?;
        self.rack = next;
        self.log.entries.push(entry);
        self.advance_match();
        Ok(record)
    }

    /// Put the balls this adjudication spotted onto the simulation's table (`rules.md` §4/§6,
    /// `rules-break.md` §3.2). The rack's spot list only ever grows within a rack; a re-rack restores
    /// the snapshot, whose balls are all in their slots already.
    fn apply_spots(&mut self, next: &Rack) -> Result<(), InputError> {
        let placed = self.rack.spots.len();
        debug_assert!(
            next.spots.len() >= placed,
            "the rack's spot list must not shrink"
        );
        for spot in &next.spots[placed..] {
            if let Err(error) = self.sim.place_ball(spot.ball, spot.pos_mm) {
                return Err(invariant(format!(
                    "pool-sim refused a spot the rules layer produced: ball {} at {:?}: {error:?}",
                    spot.ball, spot.pos_mm
                )));
            }
            self.positions[usize::from(spot.ball)] =
                BallState::at_rest([spot.pos_mm[0], spot.pos_mm[1], BALL_RADIUS_MM]);
        }
        Ok(())
    }

    /// The match's own transition after a rack ended (`rules.md` §8): score it, then either decide the
    /// match or set up the next rack, whose seed and breaker derive from the match seed again.
    fn advance_match(&mut self) {
        let RulesState::RackOver { winner } = self.rack.state else {
            return;
        };
        let scorer = match winner {
            Winner::P1 => Some(Player::P1),
            Winner::P2 => Some(Player::P2),
            // No rule in the spec produces a drawn rack (`rules.md` §1: the stalemate is re-racked by
            // agreement). Should one ever arise, it scores nobody and the match moves on rather than
            // inventing a winner.
            Winner::Drawn => None,
        };
        if let Some(seat) = scorer {
            self.race[seat.index()] += 1;
        }
        let decided =
            scorer.filter(|seat| u32::from(self.race[seat.index()]) >= self.config.race_target);
        if let Some(seat) = decided {
            self.match_winner = Some(seat);
            self.rack.state = RulesState::MatchOver { winner: seat };
        } else {
            self.rack_index += 1;
            let (rack, sim, positions) = Self::rack_at(&self.config, self.rack_index);
            self.rack = rack;
            self.sim = sim;
            self.positions = positions;
        }
    }
}

/// A violated invariant: a bug in the rules layer or the simulation, never an input problem
/// (`architecture.md` §4). Debug builds trip on the spot; release builds report it.
fn invariant(message: String) -> InputError {
    debug_assert!(false, "{message}");
    InputError::Invariant(message)
}
