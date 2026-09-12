//! The rack fixtures and invariants (`rules-break.md` §2.9/§2.10), checked against
//! `docs/spec/rack-fixtures.json`: the frozen fixture list, the hand-derived golden, the per-seed
//! invariant sweep, and the generator property sweep.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pool_sim::frame::Slot;
use pool_sim::rack::{self, Arrangement};

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec")
}

fn fixtures() -> serde_json::Value {
    let path = spec_dir().join("rack-fixtures.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("rack-fixtures.json parses")
}

fn arrangement_from_json(value: &serde_json::Value) -> Arrangement {
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
fn the_generator_reproduces_every_fixture() {
    let doc = fixtures();
    let fixtures = doc["fixtures"].as_array().expect("fixtures is an array");
    assert_eq!(fixtures.len(), 16, "the frozen fixture list is seeds 0–15");

    for fixture in fixtures {
        let seed = fixture["seed"].as_u64().expect("seed");
        let expected = arrangement_from_json(&fixture["arrangement"]);
        let produced = rack::generate(seed);
        assert_eq!(
            produced,
            expected,
            "seed {seed}: produced {:?}, fixture says {:?}",
            produced.labelled(),
            expected.labelled()
        );
    }
}

#[test]
fn the_hand_derived_golden_matches() {
    let doc = fixtures();
    let golden = &doc["golden"];
    let seed = golden["seed"].as_u64().expect("golden seed");
    let expected = arrangement_from_json(&golden["arrangement"]);
    assert_eq!(
        rack::generate(seed),
        expected,
        "the hand-derived golden (seed {seed})"
    );
}

#[test]
fn the_invariant_sweep_holds_over_the_fixture_list_and_the_property_range() {
    let doc = fixtures();
    let range = &doc["property_sweep_range"];
    let start = range["start"].as_u64().expect("sweep start");
    let end = range["end"].as_u64().expect("sweep end");
    assert_eq!((start, end), (0, 255), "the pinned property sweep is 0–255");

    for seed in start..=end {
        let arrangement = rack::generate(seed);
        rack::check_invariants(&arrangement)
            .unwrap_or_else(|e| panic!("seed {seed} violates an invariant: {e}"));
        assert_eq!(
            arrangement,
            rack::generate(seed),
            "seed {seed} is not generate-twice equal"
        );
    }
}

#[test]
fn the_invariant_ids_in_the_corpus_all_map_to_checked_rules() {
    // The corpus names its invariants; every one of them is discharged by `check_invariants`, and the
    // two that need the arrangement to be *legal* rather than merely well-formed are covered by
    // construction (§2.8's proof) plus the fixture sweep above.
    let doc = fixtures();
    let invariants = doc["invariants"]
        .as_array()
        .expect("invariants is an array");
    let known = [
        "eight_in_third_row_middle",
        "corners_one_ball_per_group",
        "fifteen_distinct_balls",
        "adjacent_slots_exactly_2r",
        "generate_twice_byte_equality",
        "every_arrangement_legal",
    ];
    for invariant in invariants {
        let id = invariant["id"].as_str().expect("invariant id");
        assert!(
            known.contains(&id),
            "invariant {id} has no check in check_invariants"
        );
    }
}
