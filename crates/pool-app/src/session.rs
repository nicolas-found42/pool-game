//! The shell's session resource (`architecture.md` §10's `session.rs`): the `Session` drive loop,
//! turn resolution, the choice-state presentation hooks, and the player-facing message a record
//! produces.
//!
//! The shell owns no game state. Everything here reads `Session`; the one write is a logged input
//! through `Request` (§10's "the only channel from shell to simulation state"), and a refused request
//! leaves the session exactly as it was (`pool-match`'s contract). No ECS type crosses the crate
//! boundary — this module is the shell's face on the loop, not a second implementation of it.

use bevy::prelude::Resource;
use pool_match::{DifficultyLevel, InputError, MatchConfig, Request, Session, SessionView};
use pool_rules::record::{Action, Adjudication, Verdict};
use pool_rules::state::{Chooser, Group, Player, RulesState, Target, Winner};
use pool_rules::{Offer, PlacementDomain};
use pool_sim::BallState;

/// A seat's name in the HUD.
#[must_use]
pub fn seat_name(player: Player) -> &'static str {
    match player {
        Player::P1 => "P1",
        Player::P2 => "P2",
    }
}

/// A group's name in the HUD.
#[must_use]
pub fn group_name(group: Group) -> &'static str {
    match group {
        Group::Solids => "solids",
        Group::Stripes => "stripes",
    }
}

/// A placement domain, as the prompt words it.
#[must_use]
pub const fn domain_name(domain: PlacementDomain) -> &'static str {
    match domain {
        PlacementDomain::AboveHeadString => "above the head string",
        PlacementDomain::Anywhere => "anywhere on the table",
    }
}

/// What the machine awaits, resolved to the seat the shell serves and the surface it presents
/// (§10's turn resolution). Both seats are human in this slice: the policy seat's branch lands with
/// M4's `ai_host`, and nothing here can request one.
#[derive(Debug, Clone, PartialEq)]
pub enum Awaiting {
    /// The shooter authors a shot at the table (`input.rs`).
    Shot {
        /// Who shoots.
        shooter: Player,
        /// The shooter's target class.
        target: Target,
        /// Whether this shot is the rack's break (`rules.md` §1: its call is `Break`).
        is_break: bool,
    },
    /// The shooter places the cue ball (ball in hand).
    Placement {
        /// Who places, then shoots.
        shooter: Player,
        /// The domain the placement must satisfy.
        domain: PlacementDomain,
    },
    /// A player picks one option of the presented tree (`ui.rs`'s menu).
    Choice {
        /// Who chooses: the state's chooser role, resolved to a seat.
        chooser: Player,
        /// The presented option sets, in the state's order.
        offers: Vec<Offer>,
    },
    /// The rack ended (`rules.md` §5).
    RackOver {
        /// Who won it.
        winner: Winner,
    },
    /// The match ended (`rules.md` §8).
    MatchOver {
        /// Who won it.
        winner: Player,
    },
}

/// A message's tone: the HUD colours the line with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Neither good nor bad: who is at the table, what the state awaits.
    Neutral,
    /// A legal outcome.
    Good,
    /// A foul, a loss, a refused input.
    Bad,
}

/// One player-facing line: the last thing that happened, in the HUD's message slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The text.
    pub text: String,
    /// The tone.
    pub tone: Tone,
}

impl Line {
    /// A neutral line.
    #[must_use]
    pub fn neutral(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Neutral,
        }
    }

    /// A bad line.
    #[must_use]
    pub fn bad(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Bad,
        }
    }
}

/// The session resource: the drive loop plus the derived view the HUD and the table read.
#[derive(Resource)]
pub struct Game {
    config: MatchConfig,
    session: Session,
    view: SessionView,
    line: Line,
}

impl Game {
    /// A fresh match: the session from `config`, and the opening message.
    #[must_use]
    pub fn new(config: MatchConfig) -> Self {
        let session = Session::new(config.clone());
        let view = session.state();
        let mut game = Self {
            config,
            session,
            view,
            line: Line::neutral(String::new()),
        };
        game.line = game.opening();
        game
    }

    /// The opening line: the first rack's breaker, and what the state asks of them.
    fn opening(&self) -> Line {
        match (&self.view.state, self.view.rack_index) {
            (RulesState::AwaitingPlacement { shooter, domain }, _) => Line::neutral(format!(
                "rack {}: {} breaks — cue ball in hand {}",
                self.view.rack_index + 1,
                seat_name(*shooter),
                domain_name(*domain)
            )),
            (state, rack) => Line::neutral(format!(
                "rack {}: {} to shoot ({})",
                rack + 1,
                seat_name(shooter_of(state).unwrap_or(Player::P1)),
                state.kind()
            )),
        }
    }

    /// One input, driven through the loop (`architecture.md` §8). A refused request changes nothing
    /// but the message line.
    pub fn request(&mut self, request: Request) -> Result<(), InputError> {
        let before = self.view.state.clone();
        match self.session.request(request) {
            Ok(record) => {
                self.view = self.session.state();
                self.line = describe(&record, &before, &self.view);
                Ok(())
            }
            Err(error) => {
                self.line = Line::bad(error.to_string());
                Err(error)
            }
        }
    }

    /// What the machine awaits, resolved (§10's turn resolution).
    #[must_use]
    pub fn awaiting(&self) -> Awaiting {
        match &self.view.state {
            RulesState::AwaitingShot { shooter, target } => Awaiting::Shot {
                shooter: *shooter,
                target: *target,
                is_break: self.view.shot_count == 0,
            },
            RulesState::AwaitingPlacement { shooter, domain } => Awaiting::Placement {
                shooter: *shooter,
                domain: *domain,
            },
            RulesState::AwaitingChoice { chooser, offers } => Awaiting::Choice {
                chooser: seat_of(*chooser, self.view.breaker),
                offers: offers.clone(),
            },
            RulesState::RackOver { winner } => Awaiting::RackOver { winner: *winner },
            RulesState::MatchOver { winner } => Awaiting::MatchOver { winner: *winner },
        }
    }

    /// The configuration the match runs under — the log's header (`architecture.md` §6).
    #[must_use]
    pub const fn config(&self) -> &MatchConfig {
        &self.config
    }

    /// The message slot's current line.
    #[must_use]
    pub fn line(&self) -> &Line {
        &self.line
    }

    /// The derived view the HUD reads (`architecture.md` §8).
    #[must_use]
    pub fn view(&self) -> &SessionView {
        &self.view
    }

    /// The current position's ball states, in the canonical order — the pre-shot state the table
    /// draws and the placement preview validates against.
    #[must_use]
    pub fn positions(&self) -> &[BallState; 16] {
        self.session.positions()
    }

    /// The last declaration the log holds: what the shell's tests read back to check the committed
    /// input is the authored one.
    #[cfg(test)]
    #[must_use]
    pub fn last_declaration(&self) -> Option<&pool_match::Declaration> {
        self.session
            .log()
            .entries
            .iter()
            .rev()
            .find_map(|entry| match entry {
                pool_match::Entry::Declaration(declaration) => Some(declaration),
                _ => None,
            })
    }

    /// The last computed shot, for `playback.rs`.
    #[must_use]
    pub fn last_shot(&self) -> Option<&pool_sim::Shot> {
        self.session.last_shot()
    }
}

/// A difficulty level's name (`ai.md` §7's four levels; inert while both seats are human).
#[must_use]
pub const fn difficulty_name(level: DifficultyLevel) -> &'static str {
    match level {
        DifficultyLevel::Beginner => "beginner",
        DifficultyLevel::Intermediate => "intermediate",
        DifficultyLevel::Advanced => "advanced",
        DifficultyLevel::Pro => "pro",
    }
}

/// The seat at the table, from the state.
fn shooter_of(state: &RulesState) -> Option<Player> {
    match state {
        RulesState::AwaitingShot { shooter, .. }
        | RulesState::AwaitingPlacement { shooter, .. } => Some(*shooter),
        _ => None,
    }
}

/// The seat a chooser role names (`rules-break.md` §3's `roleChooser`).
///
/// `Chooser::Incoming` is the incoming player — the seat that is not the rack's breaker — and
/// `Chooser::Other` (§7's stalemate offer, which either player may raise) reads as that same
/// non-breaker seat here.
fn seat_of(role: Chooser, breaker: Player) -> Player {
    match role {
        Chooser::Incoming | Chooser::Other => breaker.other(),
        Chooser::Breaker => breaker,
    }
}

/// The player-facing line a record produces: fouls, turn changes, rack and match results.
fn describe(record: &Adjudication, before: &RulesState, view: &SessionView) -> Line {
    let shooter = shooter_of(before);
    let seat = shooter.map_or("the shooter", seat_name);
    let fouls: Vec<&str> = record.fouls.iter().map(|foul| foul.rule.id()).collect();
    let penalty = match record.apply.action {
        Action::CueBallInHandAnywhere => format!(
            "{} takes ball in hand anywhere",
            seat_name(view.breaker.other())
        ),
        Action::CueBallInHandAboveHeadString | Action::SpotEightAndCueBallInHand => format!(
            "{} takes cue ball in hand above the head string",
            seat_name(view.breaker.other())
        ),
        Action::PassTurn => format!("the turn passes to {}", seat_name(view.breaker.other())),
        Action::Continue => format!("{seat} continues"),
        Action::ReRack => "the rack is reconstructed".to_string(),
        Action::OfferChoice => "a choice is awaited".to_string(),
        Action::AcceptInPosition => "the balls are played where they lie".to_string(),
        Action::SpotEightAndAccept => "the 8 is spotted".to_string(),
        Action::PlaceCue => "cue ball placed".to_string(),
        Action::SpotBall => "ball spotted".to_string(),
        Action::RackOver => "the rack is over".to_string(),
    };
    let chosen = record
        .chosen_option
        .as_ref()
        .map_or_else(String::new, |chosen| {
            format!(" — {}", chosen.option.description())
        });

    let (mut text, mut tone) = match record.verdict {
        Verdict::Foul => (
            format!(
                "foul ({}){}: {penalty}",
                if fouls.is_empty() {
                    "4.9".to_string()
                } else {
                    fouls.join(", ")
                },
                chosen
            ),
            Tone::Bad,
        ),
        Verdict::IllegalBreak => (
            format!("illegal break (4.3(d)){chosen}: {penalty}"),
            Tone::Bad,
        ),
        Verdict::EightOnBreak => (
            format!("the 8 left the table on the break{chosen}: {penalty}"),
            Tone::Bad,
        ),
        Verdict::Legal => (format!("legal shot — {penalty}"), Tone::Good),
        Verdict::NoShot => (format!("{}{chosen}", no_shot_text(record)), Tone::Neutral),
        Verdict::Win => (
            format!("{seat} wins the rack ({}–{})", view.race[0], view.race[1]),
            Tone::Good,
        ),
        Verdict::Loss => (
            format!("{seat} loses the rack ({})", record.legality),
            Tone::Bad,
        ),
    };

    // The match layer re-racks inside the same request (`pool-match`'s `advance_match`), so a rack
    // change rides on the record that ended the old one.
    if view.match_winner.is_some() {
        let winner = view.match_winner.unwrap_or(Player::P1);
        text = format!(
            "{} wins the match {}-{}",
            seat_name(winner),
            view.race[0],
            view.race[1]
        );
        tone = Tone::Good;
    } else if let Some(next) = next_rack(before, view) {
        text = format!(
            "{text} — rack {}: {} breaks",
            view.rack_index + 1,
            seat_name(next)
        );
    }
    Line { text, tone }
}

/// A non-shot record's line: the machine's classification, in the shell's words.
fn no_shot_text(record: &Adjudication) -> String {
    match pool_rules::record::token_of(&record.legality) {
        "stalemate_declaration" => "stalemate proposed and agreed (1.13/4.11)".to_string(),
        "cue_ball_placed" => "cue ball placed".to_string(),
        token => token.to_string(),
    }
}

/// The next rack's breaker, when the record ended a rack and the match continues.
fn next_rack(before: &RulesState, view: &SessionView) -> Option<Player> {
    let ended = matches!(before, RulesState::AwaitingShot { .. })
        && matches!(view.state, RulesState::AwaitingPlacement { .. });
    ended.then_some(view.breaker)
}
