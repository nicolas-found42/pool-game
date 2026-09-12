//! The determinism-golden corpus (`architecture.md` §11): `docs/spec/goldens.json`.
//!
//! Each entry names its input and its expected value; the harness recomputes and compares, and a
//! mismatch fails with the actual value beside the golden.
//!
//! - `rack` entries defer to `rack-fixtures.json` (#14's fixtures) and assert exact agreement.
//! - `shot` entries carry a pre-state (`rack_seed` or an inline `arrangement`, `cue_mm`, optional
//!   `placements`), a strike declaration, the fact pattern's essentials, and the rest `state_hash`
//!   (`physics.md` §9.1's golden-shot corpus). A strike's `aim_deg` is the absolute aim angle in the
//!   table frame; `elevation_deg` is likewise in degrees, and `spin` offsets are fractions of the
//!   miscue envelope.
//! - `replay` entries arrive with M2; a kind with no verifier here fails loudly rather than passing
//!   silently.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pool_sim::facts::{FactKind, rail_summary};
use pool_sim::frame::Slot;
use pool_sim::rack::{self, Arrangement};
use pool_sim::sim::{Shot, Sim};
use pool_sim::state_hash::state_hash_hex;
use pool_sim::strike::StrikeDecl;
use pool_sim::table::{PocketId, Rail, Table};
use serde_json::{Value, json};

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec")
}

fn read_json(name: &str) -> Value {
    let path = spec_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()))
}

fn arrangement_from_json(value: &Value) -> Arrangement {
    let map: BTreeMap<String, String> =
        serde_json::from_value(value.clone()).expect("arrangement is a ball → slot map");
    let mut slots = [0_u8; 15];
    for (ball, label) in map {
        let ball: u8 = ball.parse().expect("ball number");
        let slot = Slot::parse(&label).expect("slot label");
        slots[slot.canonical_index()] = ball;
    }
    Arrangement { slots }
}

#[test]
fn every_golden_entry_verifies() {
    let doc = read_json("goldens.json");
    let entries = doc["entries"].as_array().expect("entries is an array");
    assert!(!entries.is_empty(), "the golden corpus must not be empty");

    let mut failures: Vec<String> = Vec::new();
    for entry in entries {
        let name = entry["name"].as_str().expect("entry name");
        let kind = entry["kind"].as_str().expect("entry kind");
        match kind {
            "rack" => verify_rack(name, entry),
            "shot" => failures.extend(verify_shot(name, entry)),
            other => panic!(
                "goldens.json entry `{name}` has kind `{other}`, which this harness does not verify"
            ),
        }
    }
    assert!(
        failures.is_empty(),
        "the golden corpus does not verify:\n\n{}",
        failures.join("\n\n")
    );
}

/// Rack entries defer to `rack-fixtures.json` (#14's fixtures) and assert exact agreement.
fn verify_rack(name: &str, entry: &Value) {
    let seeds: Vec<u64> = serde_json::from_value(entry["input"]["seeds"].clone())
        .unwrap_or_else(|e| panic!("{name}: seeds are not a list of integers: {e}"));
    let source = entry["expected"]["source"]
        .as_str()
        .expect("expected.source");
    let agreement = entry["expected"]["agreement"]
        .as_str()
        .expect("expected.agreement");
    assert_eq!(
        agreement, "exact",
        "{name}: only exact agreement is verifiable"
    );

    let fixtures = read_json(source);
    let by_seed: BTreeMap<u64, Arrangement> = fixtures["fixtures"]
        .as_array()
        .expect("fixtures is an array")
        .iter()
        .map(|f| {
            (
                f["seed"].as_u64().expect("seed"),
                arrangement_from_json(&f["arrangement"]),
            )
        })
        .collect();

    for seed in seeds {
        let expected = by_seed
            .get(&seed)
            .unwrap_or_else(|| panic!("{name}: seed {seed} is not in {source}"));
        let produced = rack::generate(seed);
        assert_eq!(
            produced,
            *expected,
            "{name}: seed {seed}\n  golden:   {:?}\n  produced: {:?}",
            expected.labelled(),
            produced.labelled()
        );
    }
}

/// One object-ball placement off the rack lattice (`physics.md` §8's pb-13 pin): the ball, and where
/// it goes.
type Placement = (u8, [f64; 2]);

/// A shot entry's pre-state: the arrangement, the cue ball, and any placements off the lattice.
fn prestate(entry: &Value) -> (Arrangement, [f64; 2], Vec<Placement>) {
    let input = &entry["input"];
    let arrangement = if let Some(seed) = input["rack_seed"].as_u64() {
        rack::generate(seed)
    } else if input["arrangement"].is_object() {
        arrangement_from_json(&input["arrangement"])
    } else {
        panic!("a shot input carries either `rack_seed` or `arrangement`");
    };
    let cue_mm: Vec<f64> = serde_json::from_value(input["cue_mm"].clone())
        .unwrap_or_else(|e| panic!("cue_mm is not a pair: {e}"));
    assert_eq!(cue_mm.len(), 2, "cue_mm is a pair");
    let placements: Vec<Placement> = input["placements"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|p| {
                    let ball =
                        u8::try_from(p["ball"].as_u64().expect("placement ball")).expect("1..=15");
                    let to: Vec<f64> = serde_json::from_value(p["to_mm"].clone())
                        .expect("placement to_mm is a pair");
                    (ball, [to[0], to[1]])
                })
                .collect()
        })
        .unwrap_or_default();
    (arrangement, [cue_mm[0], cue_mm[1]], placements)
}

/// The strike declaration a shot entry carries (`physics.md` §4).
fn strike_of(entry: &Value) -> StrikeDecl {
    let strike = &entry["input"]["strike"];
    let aim_deg = strike["aim_deg"].as_f64().expect("strike.aim_deg");
    let rad = aim_deg.to_radians();
    let spin: Vec<f64> =
        serde_json::from_value(strike["spin"].clone()).expect("strike.spin is a pair");
    StrikeDecl {
        aim: [rad.cos(), rad.sin()],
        speed_mm_s: strike["speed_mm_s"].as_f64().expect("strike.speed_mm_s"),
        spin: [spin[0], spin[1]],
        elevation_rad: strike["elevation_deg"]
            .as_f64()
            .expect("strike.elevation_deg")
            .to_radians(),
    }
}

/// Run a shot entry and produce the fact pattern's essentials in the corpus' own spelling.
fn run_shot(entry: &Value) -> (Shot, Arrangement) {
    let (arrangement, cue_mm, placements) = prestate(entry);
    let mut sim = Sim::new(Table::new(), arrangement);
    for (ball, to_mm) in placements {
        sim.place_ball(ball, to_mm)
            .unwrap_or_else(|e| panic!("placement of ball {ball} at {to_mm:?} rejected: {e:?}"));
    }
    sim.place_cue(cue_mm)
        .unwrap_or_else(|e| panic!("cue placement at {cue_mm:?} rejected: {e:?}"));
    let shot = sim
        .strike(strike_of(entry))
        .unwrap_or_else(|e| panic!("strike rejected: {e:?}"));
    (shot, arrangement)
}

/// The produced facts of a shot entry, as the JSON object `expected.facts` is written in.
fn produced_facts(shot: &Shot, arrangement: &Arrangement) -> Value {
    let start = arrangement.rest_states();
    let rest = shot.rest();
    let summary = rail_summary(shot.events(), &rest.states);

    let mut pocketed: Vec<(u8, PocketId)> = Vec::new();
    let mut object_ball_contacts: Vec<u8> = Vec::new();
    for fact in shot.events() {
        let counterpart = match fact.kind {
            FactKind::Pocketed { ball, pocket } => {
                pocketed.push((ball, pocket));
                continue;
            }
            FactKind::BallBall { a: 0, b } => b,
            FactKind::BallBall { a, b: 0 } => a,
            _ => continue,
        };
        if !object_ball_contacts.contains(&counterpart) {
            object_ball_contacts.push(counterpart);
        }
    }
    object_ball_contacts.sort_unstable();

    let moved: Vec<u8> = (1..16_u8)
        .filter(|&ball| rest.states[usize::from(ball)].pos_mm != start[usize::from(ball)].pos_mm)
        .collect();
    let frozen: Vec<Value> = rest
        .frozen_rails
        .iter()
        .map(|(ball, rail): &(u8, Rail)| json!({ "ball": ball, "rail": rail }))
        .collect();

    json!({
        "pocketed": pocketed
            .iter()
            .map(|(ball, pocket)| json!({ "ball": ball, "pocket": pocket }))
            .collect::<Vec<Value>>(),
        "off_table": rest.off_table,
        "distinct_object_balls_to_rails": summary.distinct_object_balls,
        "rail_contacts_total": summary.contacts,
        "suppressed_frozen_contacts": summary.suppressed_frozen,
        "object_ball_contacts": object_ball_contacts.len(),
        "cue_ball_pocketed": rest.pocketed.contains(&0),
        "moved_object_balls": moved,
        "rest_frozen_rails": frozen,
        "t_rest_s": shot.t_rest_s(),
    })
}

/// Shot entries pin the rest `state_hash`, the fact pattern's essentials, and nothing else. Returns
/// the mismatch dumps rather than panicking, so one run reports every stale entry at once.
fn verify_shot(name: &str, entry: &Value) -> Vec<String> {
    let (shot, arrangement) = run_shot(entry);
    let golden = &entry["expected"];
    let actual_facts = produced_facts(&shot, &arrangement);
    let actual_hash = state_hash_hex(&shot.rest().states);
    let actual = json!({ "facts": actual_facts.clone(), "state_hash": actual_hash });

    let mut failures: Vec<String> = Vec::new();
    let golden_facts = golden["facts"]
        .as_object()
        .expect("expected.facts is an object");
    let actual_facts = actual_facts.as_object().expect("produced facts");
    for (key, want) in golden_facts {
        match actual_facts.get(key) {
            None => failures.push(format!(
                "  `{key}`: the golden has it, the run produced none"
            )),
            Some(got) if got != want => {
                failures.push(format!("  `{key}`: golden {want}, produced {got}"));
            }
            Some(_) => {}
        }
    }
    for key in actual_facts.keys() {
        if !golden_facts.contains_key(key) {
            failures.push(format!(
                "  `{key}`: the run produced it, the golden does not pin it"
            ));
        }
    }
    let want_hash = golden["state_hash"].as_str().expect("expected.state_hash");
    if want_hash != actual_hash {
        failures.push(format!(
            "  `state_hash`: golden {want_hash}, produced {actual_hash}"
        ));
    }
    if failures.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "{name}: the run does not match its golden\n{}\n  golden:   {}\n  produced: {}",
        failures.join("\n"),
        serde_json::to_string_pretty(golden).expect("render the golden"),
        serde_json::to_string_pretty(&actual).expect("render the run"),
    )]
}
