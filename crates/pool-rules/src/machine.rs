//! The rack-scoped machine (`rules.md` §1): the pure function `(state, input) → (record, state')`.
//!
//! The machine owns one rack: what it awaits, who breaks it, what is assigned, and the table as the
//! rules layer reads it. Every transition is a pure function of the state and the input, so a replay is
//! the rack plus the input log (`architecture.md` §6), and the break, the option trees, placement and
//! spotting meet here rather than in the crates that call them.

use pool_sim::rack::Arrangement;

use crate::break_rules;
use crate::facts::{Observation, PreBall, PreState};
use crate::offer::{ChosenOption, Offer, OptionId, Tree};
use crate::placement::{self, PlacementFault};
use crate::record::{Action, Adjudication, Apply, Classification, Foul, Verdict};
use crate::shot_rules::{self, Outcome, ShotContext};
use crate::spot::{self, Spot};
use crate::state::{Chooser, Group, Player, RulesState, Target};
use crate::vocab::{Call, PlacementDomain, ShotDeclaration, Vec2};

/// One input to the machine (`rules.md` §1's input column, `architecture.md` §6's log entries).
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// A cue-ball placement within the domain the state awaits.
    Placement {
        /// The domain the placement claims.
        domain: PlacementDomain,
        /// The cue ball's centre (mm, table frame).
        pos: Vec2,
    },
    /// The 1.6 ¶2 request: spot the legal object ball nearest the head string (`rules.md` §6).
    SpotRequest,
    /// One atomic shot: its declaration and the simulation's report.
    Shot {
        /// The declaration.
        declaration: ShotDeclaration,
        /// What the shot did (`rules.md` §10).
        observation: Box<Observation>,
    },
    /// One option of the tree the state presents.
    Option(OptionId),
    /// The stalemate declaration (`rules.md` §7): the agreement is the input, not a shot.
    Stalemate,
}

impl Input {
    /// The input's kind, for the error report.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Placement { .. } => "placement",
            Self::SpotRequest => "spot_request",
            Self::Shot { .. } => "shot",
            Self::Option(_) => "option",
            Self::Stalemate => "stalemate",
        }
    }
}

/// Why an input could not be taken (`architecture.md` §4's input-boundary policy).
#[derive(Debug, Clone, PartialEq)]
pub enum InputError {
    /// The input is not the one the state awaits.
    NotAwaited {
        /// What the state awaits.
        awaited: &'static str,
        /// What arrived.
        input: &'static str,
    },
    /// A placement's domain is not the domain the state awaits.
    DomainMismatch {
        /// The awaited domain.
        awaited: PlacementDomain,
        /// The domain the input claims.
        given: PlacementDomain,
    },
    /// The placement failed validation (3.10, enforced at the boundary).
    Placement(PlacementFault),
    /// The option is not one of the pending tree's.
    UnknownOption(OptionId),
    /// The 1.6 ¶2 request's precondition does not hold: some legal object ball is playable.
    SpotRequestUnavailable,
    /// A `Break` call outside the rack's break, or another call on the rack's break.
    NotTheBreak,
}

/// The rack-scoped rules machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Rack {
    /// What the machine awaits.
    pub state: RulesState,
    /// The seat that breaks this rack; the other seat is the incoming player of every offer.
    pub breaker: Player,
    /// The group assignment (4.4), once a called ball has been pocketed legally.
    pub assignment: Option<[Group; 2]>,
    /// The rack's seed (`rules-break.md` §2.7). No transition changes it.
    pub rack_seed: u64,
    /// The rack's frozen 15-slot snapshot (§2.7): what "the same positions" always means.
    pub snapshot: Arrangement,
    /// The rack's shot counter (§3.7 step 5).
    pub shot_count: u32,
    /// The table as the rules layer reads it (`rules.md` §10.8).
    pub table: PreState,
    /// The spot positions this machine produced (`rules.md` §4). The caller applies them to the ball
    /// array; the sim places object balls through its own API.
    pub spots: Vec<Spot>,
}

impl Rack {
    /// A rack awaiting its break: the cue ball in hand above the head string (4.3(a)).
    #[must_use]
    pub fn new(rack_seed: u64, snapshot: Arrangement, breaker: Player) -> Self {
        Self {
            state: RulesState::AwaitingPlacement {
                shooter: breaker,
                domain: PlacementDomain::AboveHeadString,
            },
            breaker,
            assignment: None,
            rack_seed,
            snapshot,
            shot_count: 0,
            table: PreState::from_states(&snapshot.rest_states(), Vec::new()),
            spots: Vec::new(),
        }
    }

    /// The shooter's target class from this rack's state (`rules.md` §2).
    #[must_use]
    pub fn target_of(&self, shooter: Player) -> Target {
        shot_rules::target_of(shooter, self.assignment, &self.table.on_table())
    }

    /// The pure transition: `(state, input) → (adjudication record, state')`.
    pub fn adjudicate(&self, input: Input) -> Result<(Adjudication, Rack), InputError> {
        match (self.state.clone(), input) {
            (
                RulesState::AwaitingPlacement { shooter, domain },
                Input::Placement { domain: given, pos },
            ) => {
                if given != domain {
                    return Err(InputError::DomainMismatch {
                        awaited: domain,
                        given,
                    });
                }
                placement::validate(given, pos, &self.table.balls)
                    .map_err(InputError::Placement)?;
                let mut next = self.clone();
                place_cue(&mut next.table, pos, given);
                next.state = RulesState::AwaitingShot {
                    shooter,
                    target: self.target_of(shooter),
                };
                let apply = next.apply(Action::PlaceCue, Vec::new());
                Ok((
                    record(
                        Verdict::NoShot,
                        &Classification::Placement,
                        Vec::new(),
                        Vec::new(),
                        None,
                        apply,
                    ),
                    next,
                ))
            }
            (
                RulesState::AwaitingPlacement {
                    shooter,
                    domain: PlacementDomain::AboveHeadString,
                },
                Input::SpotRequest,
            ) => {
                let legal = shot_rules::legal_balls(
                    self.target_of(shooter),
                    self.assignment.map(|groups| groups[shooter.index()]),
                );
                let ball = self
                    .requestable_spot(&legal)
                    .ok_or(InputError::SpotRequestUnavailable)?;
                let mut next = self.clone();
                next.spot(ball);
                let apply = next.apply(Action::SpotBall, vec![ball]);
                Ok((
                    record(
                        Verdict::NoShot,
                        &Classification::SpotRequest { ball },
                        Vec::new(),
                        Vec::new(),
                        None,
                        apply,
                    ),
                    next,
                ))
            }
            (
                RulesState::AwaitingShot { shooter, .. },
                Input::Shot {
                    declaration,
                    observation,
                },
            ) => {
                let is_break = matches!(declaration.call, Call::Break);
                if is_break != (self.shot_count == 0) {
                    return Err(InputError::NotTheBreak);
                }
                Ok(if is_break {
                    self.adjudicate_break(shooter, &observation)
                } else {
                    self.adjudicate_shot(shooter, &declaration.call, &observation)
                })
            }
            (RulesState::AwaitingShot { .. }, Input::Stalemate) => {
                let offers = vec![Tree::Stalemate.offer()];
                let chosen =
                    ChosenOption::of(Tree::Stalemate, OptionId::ReRackAndOriginalBreakerBreaks);
                let next = Rack {
                    state: RulesState::AwaitingChoice {
                        chooser: Chooser::Other,
                        offers: offers.clone(),
                    },
                    ..self.clone()
                };
                let apply = next.apply(Action::OfferChoice, Vec::new());
                Ok((
                    record(
                        Verdict::NoShot,
                        &Classification::StalemateDeclaration,
                        Vec::new(),
                        offers,
                        Some(chosen),
                        apply,
                    ),
                    next,
                ))
            }
            (RulesState::AwaitingChoice { offers, .. }, Input::Option(option)) => {
                let Some(tree) = offers
                    .iter()
                    .find(|offer| offer.options.iter().any(|item| item.option == option))
                    .map(|offer| offer.tree)
                else {
                    return Err(InputError::UnknownOption(option));
                };
                Ok(self.apply_option(tree, option, offers))
            }
            (state, input) => Err(InputError::NotAwaited {
                awaited: state.kind(),
                input: input.kind(),
            }),
        }
    }

    /// The legal ball the 1.6 ¶2 request would spot, if every legal object ball is above the head
    /// string.
    #[must_use]
    fn requestable_spot(&self, legal: &[u8]) -> Option<u8> {
        if !placement::all_above_head_string(&self.table.balls, legal) {
            return None;
        }
        placement::nearest_to_head_string(&self.table.balls, legal)
    }

    /// The break's transition (`rules-break.md` §3.2–§3.6).
    fn adjudicate_break(&self, shooter: Player, observation: &Observation) -> (Adjudication, Rack) {
        let ruling = break_rules::classify(observation);
        let mut next = self.after_shot(observation);
        for ball in &ruling.spotted {
            next.spot(*ball);
        }
        let (state, action) = match ruling.tree {
            Some(tree) => (
                RulesState::AwaitingChoice {
                    chooser: tree.chooser(),
                    offers: vec![tree.offer()],
                },
                Action::OfferChoice,
            ),
            None => match ruling.classification {
                Classification::LegalBreakByPocket => (
                    RulesState::AwaitingShot {
                        shooter,
                        target: Target::Open,
                    },
                    Action::Continue,
                ),
                _ => (
                    RulesState::AwaitingShot {
                        shooter: shooter.other(),
                        target: Target::Open,
                    },
                    Action::PassTurn,
                ),
            },
        };
        next.state = state;
        let offers = match &next.state {
            RulesState::AwaitingChoice { offers, .. } => offers.clone(),
            _ => Vec::new(),
        };
        let apply = next.apply(action, ruling.spotted);
        (
            record(
                ruling.verdict,
                &ruling.classification,
                ruling.fouls,
                offers,
                None,
                apply,
            ),
            next,
        )
    }

    /// An ordinary shot's transition (`rules.md` §2–§5).
    fn adjudicate_shot(
        &self,
        shooter: Player,
        call: &Call,
        observation: &Observation,
    ) -> (Adjudication, Rack) {
        let context = ShotContext {
            shooter,
            assignment: self.assignment,
            pre_state: &self.table,
        };
        let ruling = shot_rules::classify(&context, call, observation);
        let mut next = self.after_shot(observation);
        if let Some(assignment) = ruling.assignment {
            next.assignment = Some(assignment);
        }
        let (state, action) = match ruling.outcome {
            Outcome::Continue => (
                RulesState::AwaitingShot {
                    shooter,
                    target: next.target_of(shooter),
                },
                Action::Continue,
            ),
            Outcome::PassTurn => (
                RulesState::AwaitingShot {
                    shooter: shooter.other(),
                    target: next.target_of(shooter.other()),
                },
                Action::PassTurn,
            ),
            Outcome::BallInHand => (
                RulesState::AwaitingPlacement {
                    shooter: shooter.other(),
                    domain: PlacementDomain::Anywhere,
                },
                Action::CueBallInHandAnywhere,
            ),
            Outcome::RackOver(winner) => (RulesState::RackOver { winner }, Action::RackOver),
        };
        next.state = state;
        let apply = next.apply(action, Vec::new());
        (
            record(
                ruling.verdict,
                &ruling.classification,
                ruling.fouls,
                Vec::new(),
                None,
                apply,
            ),
            next,
        )
    }

    /// An option's transition (`rules-break.md` §3.3–§3.7, `rules.md` §7).
    fn apply_option(
        &self,
        tree: Tree,
        option: OptionId,
        offers: Vec<Offer>,
    ) -> (Adjudication, Rack) {
        let chooser = self.chooser(tree.chooser());
        let spotted = match option {
            OptionId::SpotEightAndAccept | OptionId::SpotEightAndCueBallInHandAboveHeadString => {
                vec![8]
            }
            _ => Vec::new(),
        };
        let mut next = self.clone();
        for ball in &spotted {
            next.spot(*ball);
        }
        let cue_on_table = self.table.position(0).is_some();
        let accept = |rack: &Rack| {
            if cue_on_table {
                RulesState::AwaitingShot {
                    shooter: chooser,
                    target: rack.target_of(chooser),
                }
            } else {
                placement_state(chooser)
            }
        };
        let (next, action) = match option {
            OptionId::AcceptInPosition => {
                let state = accept(&next);
                (Rack { state, ..next }, Action::AcceptInPosition)
            }
            OptionId::CueBallInHandAboveHeadString => (
                Rack {
                    state: placement_state(chooser),
                    ..next
                },
                Action::CueBallInHandAboveHeadString,
            ),
            OptionId::SpotEightAndAccept => {
                let state = accept(&next);
                (Rack { state, ..next }, Action::SpotEightAndAccept)
            }
            OptionId::SpotEightAndCueBallInHandAboveHeadString => (
                Rack {
                    state: placement_state(chooser),
                    ..next
                },
                Action::SpotEightAndCueBallInHand,
            ),
            OptionId::ReRackAndBreak => (next.re_rack(self.breaker.other()), Action::ReRack),
            OptionId::ReRackAndOffenderBreaks => (next.re_rack(self.breaker), Action::ReRack),
            OptionId::ReBreak => (next.re_rack(self.chooser(tree.chooser())), Action::ReRack),
            OptionId::ReRackAndOriginalBreakerBreaks => {
                (next.re_rack(self.breaker), Action::ReRack)
            }
        };
        let chosen = ChosenOption::of(tree, option);
        // The stalemate tree's record keeps the declaration's classification: the tree's single option
        // *is* the agreement, and the corpus writes `stalemate_declaration` for both the entrance
        // (sb-05) and the applications (sb-01..sb-04).
        let classification = if tree == Tree::Stalemate {
            Classification::StalemateDeclaration
        } else {
            Classification::OptionApplied {
                tree,
                index: chosen.index,
            }
        };
        let apply = next.apply(action, spotted);
        (
            record(
                Verdict::NoShot,
                &classification,
                Vec::new(),
                offers,
                Some(chosen),
                apply,
            ),
            next,
        )
    }

    /// The seat a chooser role names (`rules-break.md` §3.3–§3.6): [`Chooser::Incoming`] is the seat
    /// that is not the breaker. [`Chooser::Other`] — the stalemate's role, whose only option the
    /// breaker's identity decides — reads the same way: the rack machine carries no proposer identity.
    #[must_use]
    fn chooser(&self, chooser: Chooser) -> Player {
        match chooser {
            Chooser::Breaker => self.breaker,
            Chooser::Incoming | Chooser::Other => self.breaker.other(),
        }
    }

    /// The state after a shot: the table from the rest block and the counter advanced.
    fn after_shot(&self, observation: &Observation) -> Rack {
        let mut next = self.clone();
        next.table = PreState::after(&observation.rest);
        next.shot_count = self.shot_count + 1;
        next
    }

    /// Reconstruct the rack (`rules-break.md` §3.7): the snapshot restored, the seed unchanged, the
    /// shot counter reset, and the breaker set by the option's text.
    fn re_rack(&self, breaker: Player) -> Rack {
        debug_assert_eq!(
            pool_sim::rack::generate(self.rack_seed),
            self.snapshot,
            "the rack's seed must reproduce its snapshot (rules-break.md §2.7)"
        );
        Rack {
            state: placement_state(breaker),
            breaker,
            assignment: None,
            rack_seed: self.rack_seed,
            snapshot: self.snapshot,
            shot_count: 0,
            table: PreState::from_states(&self.snapshot.rest_states(), Vec::new()),
            spots: self.spots.clone(),
        }
    }

    /// Spot `ball` at the position 1.5's algorithm gives it, and put it back on the rules layer's
    /// table view.
    fn spot(&mut self, ball: u8) {
        let pos_mm = spot::spot_position(ball, &self.table.balls);
        self.spots.push(Spot { ball, pos_mm });
        let placed = PreBall {
            id: ball,
            x_mm: pos_mm[0],
            y_mm: pos_mm[1],
        };
        match self.table.balls.iter_mut().find(|state| state.id == ball) {
            Some(state) => *state = placed,
            None => self.table.balls.push(placed),
        }
        self.table.balls.sort_unstable_by_key(|state| state.id);
    }

    /// The `apply` block for an action, from the state this rack is in after the action.
    fn apply(&self, action: Action, spotted: Vec<u8>) -> Apply {
        Apply {
            action,
            breaker: self.breaker,
            rack_seed_unchanged: true,
            shot_count_reset: matches!(action, Action::ReRack),
            spotted,
            state: self.state.clone(),
        }
    }
}

/// The state a placement-awaiting rack is in: the shooter places the cue ball above the head string.
fn placement_state(shooter: Player) -> RulesState {
    RulesState::AwaitingPlacement {
        shooter,
        domain: PlacementDomain::AboveHeadString,
    }
}

/// Put the cue ball at `pos` in the rules layer's table view and remember the domain it was placed
/// within (3.11's precondition).
fn place_cue(table: &mut PreState, pos: Vec2, domain: PlacementDomain) {
    let placed = PreBall {
        id: 0,
        x_mm: pos.x,
        y_mm: pos.y,
    };
    match table.balls.iter_mut().find(|state| state.id == 0) {
        Some(state) => *state = placed,
        None => table.balls.push(placed),
    }
    table.balls.sort_unstable_by_key(|state| state.id);
    table.placement_domain = Some(domain);
}

/// Assemble a record.
fn record(
    verdict: Verdict,
    classification: &Classification,
    fouls: Vec<Foul>,
    offers: Vec<Offer>,
    chosen_option: Option<ChosenOption>,
    apply: Apply,
) -> Adjudication {
    Adjudication {
        verdict,
        legality: classification.label(),
        fouls,
        offers,
        chosen_option,
        apply,
    }
}
