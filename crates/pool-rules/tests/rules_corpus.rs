//! The break corpus, run (`architecture.md` §11, `rules-break.md` §"Corpus contract").
//!
//! Every entry of `docs/spec/rules-break.json` is driven through [`pool_rules::machine::Rack`] and its
//! `adjudication` record compared field by field. An entry's inputs are what the entry declares: the
//! fixture arrangement, the cue ball, the facts with their simultaneity groups, the rest block, and the
//! synthetic pre-state the entries that need one carry (`rb-08`, `rb-32`); a branch continuation
//! (`follow_up`) is driven by its parent's declared tree and chosen option.
//!
//! **One field is compared by token.** `adjudication.legality` is a free string in the corpus schema
//! ("Break-legality classification text"), and every entry's tail after the leading token is
//! per-entry commentary — `rb-01`'s "no foul" and `rb-05`'s "three distinct object balls to rails" sit
//! on the same classification, and `rb-13` writes "(4.3(f) with 4.7)" where `rb-12` writes "(4.3(f))".
//! No machine can produce that prose from the facts, so the harness compares the classification token
//! (the text before the first `(` or `;`) and reports the two full strings on mismatch. Every other
//! field compares exactly.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pool_rules::facts::{Observation, PreBall, PreState, RestView, RulesFact};
use pool_rules::machine::{Input, Rack};
use pool_rules::offer::{OptionId, Tree};
use pool_rules::record::{Adjudication, token_of};
use pool_rules::spot::spot_position;
use pool_rules::state::{Chooser, Player, RulesState, Target};
use pool_rules::vocab::{Call, ShotDeclaration, Spin, Vec2};
use serde::Deserialize;
use serde_json::Value;

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec")
}

fn read_json(name: &str) -> Value {
    let path = spec_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()))
}

// ---------------------------------------------------------------- the corpus's shape

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    #[serde(rename = "corpus_version")]
    version: u32,
    rack_fixture_list: String,
    trees: BTreeMap<String, TreeSpec>,
    entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeSpec {
    trigger: String,
    chooser: Chooser,
    options: Vec<TreeOption>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeOption {
    index: u8,
    option: String,
}

#[derive(Debug, Deserialize)]
struct Entry {
    id: String,
    name: String,
    spec_ref: String,
    arrangement: ArrangementRef,
    cue_ball: Point,
    break_kind: String,
    declaration: DeclarationKind,
    #[serde(default)]
    facts: Vec<RulesFact>,
    #[serde(default)]
    simultaneity_groups: Vec<Vec<usize>>,
    #[serde(default)]
    rest: Option<RestView>,
    target_facts: TargetFacts,
    adjudication: Value,
    #[serde(default)]
    follow_up: Vec<FollowUp>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArrangementRef {
    #[serde(rename = "ref")]
    fixture: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    x_mm: f64,
    y_mm: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationKind {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct TargetFacts {
    summary: String,
    expected: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FollowUp {
    entry: String,
    chosen_option: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    seed: u64,
    arrangement: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct FixtureList {
    fixtures: Vec<Fixture>,
}

// ---------------------------------------------------------------- input construction

/// The fixture an entry's `arrangement.ref` names, as the arrangement and its seed.
fn fixture(
    corpus: &Corpus,
    fixtures: &FixtureList,
    entry: &Entry,
) -> (u64, pool_sim::rack::Arrangement) {
    let index: usize = entry
        .arrangement
        .fixture
        .rsplit('/')
        .next()
        .and_then(|tail| tail.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "{}: cannot read the fixture ref {}",
                entry.id, entry.arrangement.fixture
            )
        });
    let fixture = fixtures
        .fixtures
        .get(index)
        .unwrap_or_else(|| panic!("{}: fixture {index} is not in the fixture list", entry.id));
    let mut slots = [0_u8; 15];
    for (ball, label) in &fixture.arrangement {
        let ball: u8 = ball
            .parse()
            .unwrap_or_else(|_| panic!("{}: bad ball number {ball}", entry.id));
        let slot = pool_sim::frame::Slot::parse(label)
            .unwrap_or_else(|| panic!("{}: bad slot label {label}", entry.id));
        slots[slot.canonical_index()] = ball;
    }
    let _ = corpus;
    (fixture.seed, pool_sim::rack::Arrangement { slots })
}

/// The entry's table as the rules layer reads it (`rules.md` §10.8): the declared rest block when the
/// entry carries one, the arrangement's own positions otherwise, with the declared cue ball moved
/// where the entry names one.
fn table_of(
    entry: &Entry,
    arrangement: &pool_sim::rack::Arrangement,
    frozen_to_rail: Vec<u8>,
) -> PreState {
    let mut balls: Vec<PreBall> = match &entry.rest {
        Some(rest) => rest
            .balls
            .iter()
            .map(|ball| PreBall {
                id: ball.id,
                x_mm: ball.x_mm,
                y_mm: ball.y_mm,
            })
            .collect(),
        None => arrangement
            .rest_states()
            .iter()
            .enumerate()
            .filter(|(_, state)| state.in_play())
            .map(|(id, state)| PreBall {
                id: id as u8,
                x_mm: state.pos_mm[0],
                y_mm: state.pos_mm[1],
            })
            .collect(),
    };
    // The entries that declare a cue ball place it; a rest block that leaves the cue ball out leaves it
    // off the table (that is how the corpus says "in hand").
    if entry.rest.as_ref().is_none_or(|rest| rest.on_table(0)) {
        let cue = PreBall {
            id: 0,
            x_mm: entry.cue_ball.x_mm,
            y_mm: entry.cue_ball.y_mm,
        };
        match balls.iter_mut().find(|ball| ball.id == 0) {
            Some(placed) => *placed = cue,
            None => balls.push(cue),
        }
    }
    balls.sort_unstable_by_key(|ball| ball.id);
    PreState {
        balls,
        frozen_to_rail,
        placement_domain: None,
    }
}

/// The synthetic pre-state an entry declares: the balls it says were frozen to a rail at shot start,
/// and the ones whose frozen contact stayed frozen (the 2.7 exception's other half).
fn declared_pre_state(entry: &Entry) -> (Vec<u8>, Vec<u8>) {
    let expected = &entry.target_facts.expected;
    let frozen = expected
        .get("pre_state")
        .and_then(|pre| pre.get("frozen_to_rail"))
        .and_then(Value::as_array)
        .map(|balls| {
            balls
                .iter()
                .filter_map(Value::as_u64)
                .map(|ball| ball as u8)
                .collect()
        })
        .unwrap_or_default();
    let stayed = expected
        .get("not_counted")
        .and_then(Value::as_array)
        .map(|balls| {
            balls
                .iter()
                .filter(|ball| {
                    ball.get("reason").and_then(Value::as_str) == Some("frozen_to_rail_stayed")
                })
                .filter_map(|ball| ball.get("ball").and_then(Value::as_u64))
                .map(|ball| ball as u8)
                .collect()
        })
        .unwrap_or_default();
    (frozen, stayed)
}

/// The observation an entry declares, with the rail contacts' two first-class frozen fields filled
/// from the entry's synthetic pre-state.
///
/// The corpus's rules tier writes a rail fact as the ball and the time; `rules.md` §10.2's pair is
/// declared only by `rb-08` and `rb-32`, and only for the frozen side (`pre_state.frozen_to_rail`,
/// plus `not_counted` for the ball whose nudge stayed frozen). A rail contact for a ball the entry
/// does not declare as frozen counts.
fn observation_of(entry: &Entry, pre_state: PreState) -> Observation {
    let (frozen, stayed) = declared_pre_state(entry);
    let mut facts = entry.facts.clone();
    for fact in &mut facts {
        if let RulesFact::Rail {
            ball,
            frozen_at_shot_start,
            left_since_shot_start,
            ..
        } = fact
        {
            *frozen_at_shot_start = frozen.contains(ball);
            *left_since_shot_start = !stayed.contains(ball);
        }
    }
    Observation {
        facts: pool_rules::facts::FactView {
            facts,
            simultaneity_groups: entry.simultaneity_groups.clone(),
        },
        rest: entry.rest.clone().unwrap_or(RestView {
            balls: Vec::new(),
            supported_over_mouth: Vec::new(),
            frozen_rails: Vec::new(),
        }),
        pre_state,
    }
}

/// The tree a branch-continuation entry applies, and why: the parent entry's `follow_up` names it.
fn pending_tree(corpus: &Corpus, entry: &Entry) -> Tree {
    for parent in &corpus.entries {
        for follow in &parent.follow_up {
            if follow.entry == entry.id {
                let tree = follow
                    .chosen_option
                    .get("tree")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{}: parent {} names no tree", entry.id, parent.id));
                return Tree::by_name(tree)
                    .unwrap_or_else(|| panic!("{}: unknown tree {tree}", entry.id));
            }
        }
    }
    panic!("{}: no entry lists it as a follow-up", entry.id);
}

/// The option a branch-continuation entry takes: what its parent's `follow_up` names.
fn chosen_option(corpus: &Corpus, entry: &Entry) -> OptionId {
    for parent in &corpus.entries {
        for follow in &parent.follow_up {
            if follow.entry == entry.id {
                let option = follow.chosen_option.get("option").and_then(Value::as_str);
                return OptionId::by_name(option.unwrap_or_else(|| {
                    panic!("{}: parent {} names no option", entry.id, parent.id)
                }))
                .unwrap_or_else(|| panic!("{}: unknown option {option:?}", entry.id));
            }
        }
    }
    panic!("{}: no entry lists it as a follow-up", entry.id);
}

/// The rack an entry starts from, and the input it receives.
fn drive(corpus: &Corpus, fixtures: &FixtureList, entry: &Entry) -> (Rack, Input) {
    let (seed, arrangement) = fixture(corpus, fixtures, entry);
    let (frozen, _) = declared_pre_state(entry);
    let table = table_of(entry, &arrangement, frozen.clone());
    let rack = Rack {
        table: table.clone(),
        ..Rack::new(seed, arrangement, Player::P1)
    };

    match entry.declaration.kind.as_str() {
        // A break shot: the declaration's impulse is the physics tier's (`docs/spec/physics-break.json`
        // pins it); the rules tier carries the call, and the machine reads nothing else.
        "break" => {
            let declaration = ShotDeclaration {
                call: Call::Break,
                aim: Vec2 { x: 1.0, y: 0.0 },
                speed: 0.0,
                spin: Spin { a: 0.0, b: 0.0 },
                elevation: 0.0,
            };
            let rack = Rack {
                state: RulesState::AwaitingShot {
                    shooter: Player::P1,
                    target: Target::Open,
                },
                ..rack
            };
            let observation = observation_of(entry, table);
            (
                rack,
                Input::Shot {
                    declaration,
                    observation: Box::new(observation),
                },
            )
        }
        // A declaration event that adjudicates no shot.
        "none" => {
            if entry.break_kind == "stalemate_declaration" && !is_a_follow_up(corpus, entry) {
                let rack = Rack {
                    state: RulesState::AwaitingShot {
                        shooter: Player::P1,
                        target: Target::Open,
                    },
                    ..rack
                };
                (rack, Input::Stalemate)
            } else {
                let tree = pending_tree(corpus, entry);
                let rack = Rack {
                    state: RulesState::AwaitingChoice {
                        chooser: tree.chooser(),
                        offers: vec![tree.offer()],
                    },
                    ..rack
                };
                (rack, Input::Option(chosen_option(corpus, entry)))
            }
        }
        other => panic!("{}: unknown declaration kind {other}", entry.id),
    }
}

fn is_a_follow_up(corpus: &Corpus, entry: &Entry) -> bool {
    corpus.entries.iter().any(|parent| {
        parent
            .follow_up
            .iter()
            .any(|follow| follow.entry == entry.id)
    })
}

// ---------------------------------------------------------------- comparison

fn differs(path: &str, expected: &Value, produced: &Value, out: &mut Vec<String>) {
    match (expected, produced) {
        (Value::Object(expected), Value::Object(produced)) => {
            for (key, value) in expected {
                let child = format!("{path}.{key}");
                match produced.get(key) {
                    Some(produced) if key == "legality" => {
                        let (expected, produced) = (value.as_str(), produced.as_str());
                        match (expected, produced) {
                            (Some(expected), Some(produced))
                                if token_of(expected) == token_of(produced) => {}
                            _ => {
                                out.push(format!(
                                    "{child}: expected {expected:?}, produced {produced:?}"
                                ));
                            }
                        }
                    }
                    Some(produced) => differs(&child, value, produced, out),
                    None => out.push(format!("{child}: missing, expected {value}")),
                }
            }
            for (key, value) in produced {
                if !expected.contains_key(key) {
                    out.push(format!("{path}.{key}: unexpected, produced {value}"));
                }
            }
        }
        (Value::Array(expected), Value::Array(produced)) => {
            if expected.len() != produced.len() {
                out.push(format!(
                    "{path}: expected {} entries, produced {}",
                    expected.len(),
                    produced.len()
                ));
            }
            for (index, (expected, produced)) in expected.iter().zip(produced).enumerate() {
                differs(&format!("{path}[{index}]"), expected, produced, out);
            }
        }
        _ => {
            if expected != produced {
                out.push(format!("{path}: expected {expected}, produced {produced}"));
            }
        }
    }
}

// ---------------------------------------------------------------- the entries

fn corpus() -> Corpus {
    let text = std::fs::read_to_string(spec_dir().join("rules-break.json"))
        .expect("rules-break.json is readable");
    serde_json::from_str(&text).expect("rules-break.json matches the harness's shape")
}

fn fixtures() -> FixtureList {
    serde_json::from_value(read_json("rack-fixtures.json")).expect("rack-fixtures.json parses")
}

/// The declared trees are the machine's trees: the ids, their order, and their choosers
/// (`rules-break.md` §3, `rules-break.json`'s `trees`).
#[test]
fn the_machine_trees_match_the_corpus_declaration() {
    let corpus = corpus();
    assert!(
        !corpus.trees.is_empty(),
        "the corpus declares at least one tree"
    );
    for (name, spec) in &corpus.trees {
        let tree = Tree::by_name(name).unwrap_or_else(|| panic!("the machine has no tree {name}"));
        assert_eq!(
            spec.chooser,
            tree.chooser(),
            "{name}: the chooser differs from the corpus's declaration"
        );
        let declared: Vec<&str> = spec
            .options
            .iter()
            .map(|option| option.option.as_str())
            .collect();
        let machine: Vec<&str> = tree.options().iter().map(|option| option.name()).collect();
        assert_eq!(
            declared, machine,
            "{name}: the option ids or their order differ"
        );
        for (position, option) in spec.options.iter().enumerate() {
            assert_eq!(
                usize::from(option.index),
                position + 1,
                "{name}: option indices start at 1 and increase"
            );
        }
        assert!(
            !spec.trigger.is_empty(),
            "{name}: the tree declares its trigger"
        );
    }
}

/// Every option of every declared tree has at least one follow-up entry (`rules-break.md`'s coverage
/// target), and every follow-up entry is an entry that exists.
#[test]
fn every_declared_option_has_a_follow_up() {
    let corpus = corpus();
    let ids: Vec<&str> = corpus
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let mut covered: BTreeMap<(String, u8), Vec<String>> = BTreeMap::new();
    for entry in &corpus.entries {
        for follow in &entry.follow_up {
            assert!(
                ids.contains(&follow.entry.as_str()),
                "{}: follow-up {} is not an entry",
                entry.id,
                follow.entry
            );
            let tree = follow
                .chosen_option
                .get("tree")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{}: a follow-up names no tree", entry.id));
            let index = follow
                .chosen_option
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| panic!("{}: a follow-up names no index", entry.id));
            assert!(
                corpus.trees.contains_key(tree),
                "{}: follow-up tree {tree} is not declared",
                entry.id
            );
            covered
                .entry((
                    tree.to_string(),
                    u8::try_from(index).expect("a small index"),
                ))
                .or_default()
                .push(follow.entry.clone());
        }
    }
    for (name, spec) in &corpus.trees {
        for option in &spec.options {
            assert!(
                covered.contains_key(&(name.clone(), option.index)),
                "{name} option {} ({}) has no follow-up entry",
                option.index,
                option.option
            );
        }
    }
}

/// The 41 entries, each adjudicated and compared field by field.
#[test]
fn the_corpus_is_adjudicated_exactly() {
    let corpus = corpus();
    let fixtures = fixtures();
    assert_eq!(corpus.schema_version, 1);
    assert!(corpus.version >= 1, "the corpus version is monotone");
    assert_eq!(
        corpus.entries.len(),
        41,
        "the corpus is the gate: all 41 entries run"
    );
    assert_eq!(corpus.rack_fixture_list, "rack-fixtures.json");

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0_usize;
    for entry in &corpus.entries {
        let (rack, input) = drive(&corpus, &fixtures, entry);
        let table = rack.table.clone();
        let produced = match rack.adjudicate(input) {
            Ok((produced, _next)) => produced,
            Err(error) => {
                failures.push(format!(
                    "{}\n  name: {}\n  spec: {}\n  the machine refused the entry's input: {error:?}\n  target: {}\n  note: {}",
                    entry.id,
                    entry.name,
                    entry.spec_ref,
                    entry.target_facts.summary,
                    entry.note.as_deref().unwrap_or("-"),
                ));
                continue;
            }
        };
        ran += 1;

        let produced_value =
            serde_json::to_value(&produced).expect("an adjudication record serializes");
        let mut problems = Vec::new();
        differs(
            "adjudication",
            &entry.adjudication,
            &produced_value,
            &mut problems,
        );
        check_declared_spot(entry, &table, &produced, &mut problems);
        if !problems.is_empty() {
            failures.push(format!(
                "{}\n  name: {}\n  spec: {}\n  target: {}\n  {}\n  expected: {}\n  produced: {}\n  note: {}",
                entry.id,
                entry.name,
                entry.spec_ref,
                entry.target_facts.summary,
                problems.join("\n  "),
                serde_json::to_string_pretty(&entry.adjudication).expect("a value prints"),
                serde_json::to_string_pretty(&produced_value).expect("a value prints"),
                entry.note.as_deref().unwrap_or("-"),
            ));
        }
    }

    assert_eq!(ran, 41, "every entry ran");
    assert!(
        failures.is_empty(),
        "{} of {} corpus entries did not adjudicate as asserted:\n\n{}",
        failures.len(),
        corpus.entries.len(),
        failures.join("\n\n")
    );
}

/// The corpus's `spot_position` where it declares one: 1.5's answer for the 8 against the entry's own
/// table.
fn check_declared_spot(
    entry: &Entry,
    table: &PreState,
    produced: &Adjudication,
    out: &mut Vec<String>,
) {
    let declared = entry
        .target_facts
        .expected
        .get("spot_position")
        .and_then(Value::as_str);
    let Some(declared) = declared else {
        return;
    };
    if !produced.apply.spotted.contains(&8) {
        out.push(format!(
            "spot_position: the corpus declares {declared:?} but the record spots {:?}",
            produced.apply.spotted
        ));
        return;
    }
    if declared.contains("foot_spot") {
        let position = spot_position(8, &table.balls);
        if position != [pool_sim::constants::FOOT_SPOT_X_MM, 0.0] {
            out.push(format!(
                "spot_position: the corpus declares {declared:?}, the search produced {position:?}"
            ));
        }
    }
}
