//! The determinism-golden corpus (`architecture.md` §11): `docs/spec/goldens.json`.
//!
//! Each entry names its input and its expected value; the harness recomputes and compares, and a
//! mismatch fails with the actual value beside the golden. Shot and replay entries grow with M1 and
//! M2; a kind with no verifier here fails loudly rather than passing silently.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pool_sim::frame::Slot;
use pool_sim::rack::{self, Arrangement};

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec")
}

fn read_json(name: &str) -> serde_json::Value {
    let path = spec_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()))
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
fn every_golden_entry_verifies() {
    let doc = read_json("goldens.json");
    let entries = doc["entries"].as_array().expect("entries is an array");
    assert!(!entries.is_empty(), "the golden corpus must not be empty");

    for entry in entries {
        let name = entry["name"].as_str().expect("entry name");
        let kind = entry["kind"].as_str().expect("entry kind");
        match kind {
            "rack" => verify_rack(name, entry),
            // `replay` entries belong to `pool-match`'s harness — the crate that can see the `Session`
            // and the rules machine at once (`architecture.md` §11). Skipping them here is not a
            // silent pass: the entry has a verifier, one crate away.
            "replay" => {}
            other => panic!(
                "goldens.json entry `{name}` has kind `{other}`, which this harness does not verify"
            ),
        }
    }
}

/// Rack entries defer to `rack-fixtures.json` (#14's fixtures) and assert exact agreement.
fn verify_rack(name: &str, entry: &serde_json::Value) {
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
