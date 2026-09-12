//! The `Session` drive loop, the match layer, and the execution-noise rule (`architecture.md` §5/§8).
//!
//! The tests drive the real examples: the committed fixture for the loop, and short inline logs for
//! the branches the fixture does not reach (a re-rack, a refused request).

use std::path::PathBuf;

use pool_match::log::Declaration;
use pool_match::{
    Difficulty, DifficultyLevel, Entry, InputError, InputLog, MatchConfig, Request, Session,
};
use pool_match::{breaker_of, first_breaker, rack_seed};
use pool_rng::SplitMix64;
use pool_rules::facts::PreBall;
use pool_rules::record::Action;
use pool_rules::state::{Player, RulesState};
use pool_rules::{Call, PlacementDomain, Spin, Vec2};
use pool_sim::rack;
use pool_sim::{Profile, Sim, Table};

/// The committed fixture's path, from the repository root.
const FIXTURE: &str = "data/matches/break-foul-ball-in-hand-eight-loss.json";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_log() -> InputLog {
    let path = root().join(FIXTURE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    InputLog::parse(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn config(match_seed: u64, noise_seed: u64) -> MatchConfig {
    MatchConfig {
        profile: Profile::default_profile(),
        match_seed,
        noise_seed,
        difficulty: Difficulty {
            level: DifficultyLevel::Pro,
            checkpoint: "test".to_string(),
        },
        race_target: 1,
    }
}

fn request(session: &mut Session, request: Request) -> pool_rules::record::Adjudication {
    session
        .request(request)
        .unwrap_or_else(|error| panic!("the request is legal: {error}"))
}

/// A session holding the fixture's first two entries: the break's placement and the break itself.
fn after_the_break(noise_seed: u64) -> Session {
    let log = fixture_log();
    let mut session = Session::new(config(log.match_seed, noise_seed));
    for entry in &log.entries[..2] {
        request(&mut session, Request::from_entry(entry));
    }
    session
}

/// The fixture's third entry (the scratch) with `from_policy` as asked.
fn declaration(from_policy: bool) -> Declaration {
    let Entry::Declaration(mut declaration) = fixture_log().entries[2].clone() else {
        panic!("the fixture's third entry is a declaration");
    };
    declaration.from_policy = from_policy;
    declaration
}

fn played(session: &mut Session, declaration: &Declaration) -> String {
    request(session, Request::Declaration(declaration.clone()));
    session.state_hash()
}

/// The seed derivation of `rules-break.md` §2.7: rack `i`'s seed is the `SplitMix64` stream seeded
/// with `match_seed` after `i + 1` steps, and the breaker alternates from the drawn first breaker.
#[test]
fn the_per_rack_seed_derivation_is_the_pinned_stream() {
    for match_seed in [0_u64, 1, 7, 0x00C0_FFEE_0000, u64::MAX] {
        for index in 0..8_u32 {
            let mut stream = SplitMix64::new(match_seed);
            let mut expected = stream.next_u64();
            for _ in 0..index {
                expected = stream.next_u64();
            }
            assert_eq!(
                rack_seed(match_seed, index),
                expected,
                "match_seed {match_seed}, rack {index}"
            );
        }
        // The draw the first breaker is taken from, recomputed the same way.
        let first = match SplitMix64::new(match_seed).next_below(2) {
            0 => Player::P1,
            _ => Player::P2,
        };
        assert_eq!(first_breaker(match_seed), first);
        for index in 0..6_u32 {
            let expected = if index % 2 == 0 { first } else { first.other() };
            assert_eq!(breaker_of(match_seed, index), expected, "rack {index}");
        }
    }
    // The alternation is real: consecutive racks break from opposite seats.
    let first = first_breaker(1);
    assert_ne!(breaker_of(1, 0), breaker_of(1, 1));
    assert_eq!(breaker_of(1, 2), first);
}

/// Execution noise is applied to a policy declaration and never to a human's, in log order.
#[test]
fn a_policy_declaration_is_perturbed_and_a_human_declaration_is_not() {
    let human = played(&mut after_the_break(1), &declaration(false));
    let human_again = played(&mut after_the_break(1), &declaration(false));
    assert_eq!(
        human, human_again,
        "a human declaration is simulated exactly as declared"
    );

    let policy = played(&mut after_the_break(1), &declaration(true));
    let policy_again = played(&mut after_the_break(1), &declaration(true));
    assert_eq!(
        policy, policy_again,
        "the same noise seed re-derives the same perturbation"
    );
    assert_ne!(
        human, policy,
        "a policy declaration executes the perturbed strike, not the declared one"
    );

    let other_seed = played(&mut after_the_break(2), &declaration(true));
    assert_ne!(
        policy, other_seed,
        "a different noise seed draws a different perturbation"
    );
}

/// A re-rack restores the snapshot, keeps the seed, and resets the rack's shot counter
/// (`rules-break.md` §3.7) — and the simulation is rebuilt with the rack.
#[test]
fn a_re_rack_restores_the_snapshot_and_keeps_the_seed() {
    let match_seed = 11;
    let mut session = Session::new(config(match_seed, 1));
    let fresh = session.clone();
    let seed = session.rack_seed();
    request(
        &mut session,
        Request::Placement {
            domain: PlacementDomain::AboveHeadString,
            pos: Vec2 { x: -800.0, y: 0.0 },
        },
    );
    // A break that never reaches the rack: 4.3(d)'s illegal break, no foul.
    let miss = Declaration {
        from_policy: false,
        call: Call::Break,
        aim: Vec2 { x: 0.0, y: 1.0 },
        speed: 1200.0,
        spin: Spin { a: 0.0, b: 0.0 },
        elevation: 0.0,
    };
    let break_record = request(&mut session, Request::Declaration(miss.clone()));
    assert!(
        break_record.legality.starts_with("illegal_break"),
        "a break that misses the rack is an illegal break: {}",
        break_record.legality
    );
    assert_eq!(break_record.apply.action, Action::OfferChoice);
    let after_break = session.state_hash();
    assert_eq!(session.state().shot_count, 1);
    let breaker = session.state().breaker;

    // The incoming player re-racks and breaks.
    let record = request(
        &mut session,
        Request::Option {
            option_id: "re_rack_and_break".to_string(),
        },
    );
    assert_eq!(record.apply.action, Action::ReRack);
    assert!(record.apply.shot_count_reset);
    let view = session.state();
    assert_eq!(view.breaker, breaker.other(), "the incoming player breaks");
    assert_eq!(view.shot_count, 0, "the counter is reset");
    assert_eq!(session.rack_seed(), seed, "the seed is unchanged");
    assert!(matches!(
        view.state,
        RulesState::AwaitingPlacement {
            domain: PlacementDomain::AboveHeadString,
            ..
        }
    ));
    assert_eq!(
        session.state_hash(),
        fresh.state_hash(),
        "the snapshot is restored, ball for ball"
    );

    // The same break, played again from the restored rack: the same positions, because the seed is
    // the same and nothing was re-derived.
    request(
        &mut session,
        Request::Placement {
            domain: PlacementDomain::AboveHeadString,
            pos: Vec2 { x: -800.0, y: 0.0 },
        },
    );
    request(&mut session, Request::Declaration(miss));
    assert_eq!(
        session.state_hash(),
        after_break,
        "the re-racked rack breaks exactly as it did"
    );
}

/// A request the state does not await is refused, and the session is left exactly as it was.
#[test]
fn a_refused_request_leaves_the_session_untouched() {
    let mut session = Session::new(config(3, 1));
    let before = session.state_hash();
    let error = session
        .request(Request::Declaration(declaration(false)))
        .expect_err("the rack awaits a placement, not a shot");
    assert!(matches!(error, InputError::NotAwaited { .. }), "{error:?}");
    assert_eq!(session.state_hash(), before);
    assert!(session.log().entries.is_empty(), "nothing was logged");

    // The break's call has to match the rack's shot count: a shot after the break whose call is
    // still `Break` is refused the same way, before any physics runs.
    request(
        &mut session,
        Request::Placement {
            domain: PlacementDomain::AboveHeadString,
            pos: Vec2 { x: -800.0, y: 0.0 },
        },
    );
    request(
        &mut session,
        Request::Declaration(Declaration {
            from_policy: false,
            call: Call::Break,
            aim: Vec2 { x: 1.0, y: 0.0 },
            speed: 5200.0,
            spin: Spin { a: 0.0, b: 0.0 },
            elevation: 0.0,
        }),
    );
    let before = session.state_hash();
    let error = session
        .request(Request::Declaration(Declaration {
            from_policy: false,
            call: Call::Break,
            aim: Vec2 { x: 1.0, y: 0.0 },
            speed: 5200.0,
            spin: Spin { a: 0.0, b: 0.0 },
            elevation: 0.0,
        }))
        .expect_err("the rack's break is behind it");
    assert!(matches!(error, InputError::Machine(_)), "{error:?}");
    assert_eq!(session.state_hash(), before);

    // An unknown option id never reaches the machine.
    let error = session
        .request(Request::Option {
            option_id: "nudge".to_string(),
        })
        .expect_err("no such option is in the vocabulary");
    assert!(matches!(error, InputError::UnknownOption(_)), "{error:?}");
}

/// The spot search's positions are positions the simulation accepts: the contact edges the two
/// crates compute are the same edges (`architecture.md` §9).
#[test]
fn the_simulation_accepts_the_positions_the_spot_search_produces() {
    for seed in 0..8_u64 {
        let arrangement = rack::generate(seed);
        let states = arrangement.rest_states();
        let balls: Vec<PreBall> = states
            .iter()
            .enumerate()
            .filter(|(_, state)| state.in_play())
            .map(|(id, state)| PreBall {
                id: id as u8,
                x_mm: state.pos_mm[0],
                y_mm: state.pos_mm[1],
            })
            .collect();
        for ball in 1..=15_u8 {
            let pos_mm = pool_rules::spot_position(ball, &balls);
            let mut sim = Sim::new(Table::new(), arrangement);
            sim.place_ball(ball, pos_mm).unwrap_or_else(|error| {
                panic!("seed {seed}: the sim refused ball {ball} at {pos_mm:?}: {error:?}")
            });
        }
    }
}

/// The recorded fixture replays byte-identically: the same final state hash and the same adjudication
/// records, recomputed from the log (`architecture.md` §6/§11).
#[test]
fn the_recorded_fixture_replays_byte_identically() {
    let log = fixture_log();
    let first = Session::replay(&log, Profile::default_profile()).expect("the fixture replays");
    let again = Session::replay(&log, Profile::default_profile()).expect("the fixture replays");
    assert_eq!(first.final_state_hash, again.final_state_hash);
    assert_eq!(records_json(&first), records_json(&again));

    // The same match, driven entry by entry rather than through the replay helper, ends in the same
    // position: the loop is the only implementation, and a recording of it is the log.
    let mut session = Session::new(MatchConfig::from_log(&log, Profile::default_profile()));
    for entry in &log.entries {
        request(&mut session, Request::from_entry(entry));
    }
    assert_eq!(session.state_hash(), first.final_state_hash);
    assert_eq!(*session.log(), log, "the run records the log it consumed");

    assert_eq!(first.race, [0, 1]);
    assert_eq!(first.winner, Player::P2);
    assert_eq!(first.racks, 1);
    assert_eq!(first.shots, 3);
    assert_eq!(
        session.state().state,
        RulesState::MatchOver { winner: Player::P2 }
    );
}

/// One JSON document per adjudication record, in order — the bytes a replay must reproduce.
fn records_json(replay: &pool_match::Replay) -> Vec<String> {
    replay
        .records
        .iter()
        .map(|record| serde_json::to_string(record).expect("an adjudication record serializes"))
        .collect()
}

/// A rack that ends advances the match to the next rack: the next seed, the alternated breaker, and a
/// fresh snapshot (`rules.md` §8, `rules-break.md` §2.7).
#[test]
fn a_rack_that_ends_advances_to_the_next_rack() {
    let log = fixture_log();
    let mut config = MatchConfig::from_log(&log, Profile::default_profile());
    config.race_target = 2;
    let mut session = Session::new(config);
    for entry in &log.entries {
        request(&mut session, Request::from_entry(entry));
    }

    let view = session.state();
    assert!(
        session.winner().is_none(),
        "one rack of a race of two decides nothing"
    );
    assert_eq!(view.rack_index, 1);
    assert_eq!(view.race, [0, 1], "P2 took the first rack");
    assert_eq!(view.breaker, breaker_of(log.match_seed, 1));
    assert_ne!(
        view.breaker,
        breaker_of(log.match_seed, 0),
        "breaks alternate"
    );
    assert_eq!(view.shot_count, 0, "the new rack's counter starts at zero");
    assert_eq!(session.rack_seed(), rack_seed(log.match_seed, 1));
    assert!(matches!(
        view.state,
        RulesState::AwaitingPlacement {
            domain: PlacementDomain::AboveHeadString,
            ..
        }
    ));
    // The scene is rack 1's arrangement, not the one the first rack was played on.
    let first_rack = Session::new(MatchConfig::from_log(&log, Profile::default_profile()));
    assert_ne!(session.state_hash(), first_rack.state_hash());

    // And it plays: the second rack takes its own break.
    request(
        &mut session,
        Request::Placement {
            domain: PlacementDomain::AboveHeadString,
            pos: Vec2 { x: -800.0, y: 0.0 },
        },
    );
    let record = request(
        &mut session,
        Request::Declaration(Declaration {
            from_policy: false,
            call: Call::Break,
            aim: Vec2 { x: 1.0, y: 0.0 },
            speed: 5200.0,
            spin: Spin { a: 0.0, b: 0.0 },
            elevation: 0.0,
        }),
    );
    assert!(
        record.legality.starts_with("legal_break"),
        "{}",
        record.legality
    );
}

/// A log recorded against another profile is refused rather than run under this one.
#[test]
fn a_log_for_another_profile_is_refused() {
    let mut log = fixture_log();
    log.profile = "tournament".to_string();
    let error =
        Session::replay(&log, Profile::default_profile()).expect_err("the profile ids differ");
    assert!(
        matches!(error, InputError::ProfileMismatch { .. }),
        "{error:?}"
    );
}
