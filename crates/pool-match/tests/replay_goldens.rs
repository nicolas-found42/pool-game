//! The golden replays (`architecture.md` §11): `docs/spec/goldens.json`'s `kind: "replay"` entries.
//!
//! Each entry names a committed input log and pins what replaying it must produce: the final
//! position's `state_hash`, the race, and the adjudication record of every entry. The harness
//! recomputes all of it from the log — nothing under `expected` is ever read back into the run
//! (`architecture.md` §6's principle) — and compares the records as JSON values, which for these
//! records is their serialized bytes: no adjudication field is a float.
//!
//! The `rack` and `shot` kinds are verified by `pool-sim`'s harness, which cannot see this crate; a
//! kind with no verifier here or there fails loudly rather than passing silently.

use std::path::{Path, PathBuf};

use pool_match::{InputLog, Session};
use pool_sim::Profile;
use serde_json::Value;

/// The repository root, from this crate's manifest: golden paths resolve from it.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} does not parse: {error}", path.display()))
}

#[test]
fn every_golden_replay_verifies() {
    let doc = read_json(&root().join("docs/spec/goldens.json"));
    let entries = doc["entries"].as_array().expect("entries is an array");
    assert!(!entries.is_empty(), "the golden corpus must not be empty");

    let mut failures: Vec<String> = Vec::new();
    let mut replays = 0_u32;
    for entry in entries {
        let name = entry["name"].as_str().expect("entry name");
        match entry["kind"].as_str().expect("entry kind") {
            "rack" | "shot" => {}
            "replay" => {
                replays += 1;
                if let Err(reasons) = verify_replay(entry) {
                    for reason in reasons {
                        failures.push(format!("{name}: {reason}"));
                    }
                }
            }
            other => failures.push(format!(
                "{name}: kind `{other}` has no verifier in this harness"
            )),
        }
    }
    assert!(replays > 0, "the replay corpus must not be empty");
    assert!(
        failures.is_empty(),
        "{} golden replay failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Recompute a log's replay and compare it against the entry's pinned values.
fn verify_replay(entry: &Value) -> Result<(), Vec<String>> {
    let path = entry["input"]["log"]
        .as_str()
        .expect("input.log is a path from the repository root");
    let text = std::fs::read_to_string(root().join(path))
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
    let log =
        InputLog::parse(&text).unwrap_or_else(|error| panic!("{path} does not parse: {error}"));

    // The profile record is the one the log's header names, read from `config/profiles/<id>.json`
    // (`architecture.md` §12) rather than assumed.
    let profile_path = root().join(format!("config/profiles/{}.json", log.profile));
    let profile: Profile = serde_json::from_str(
        &std::fs::read_to_string(&profile_path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", profile_path.display())),
    )
    .unwrap_or_else(|error| panic!("{} does not parse: {error}", profile_path.display()));

    let replay = Session::replay(&log, profile).expect("the recorded log replays");
    let expected = &entry["expected"];
    let mut failures = Vec::new();

    let hash = expected["final_state_hash"]
        .as_str()
        .expect("expected.final_state_hash");
    if hash != replay.final_state_hash {
        failures.push(format!(
            "final_state_hash:\n  golden:   {hash}\n  produced: {}",
            replay.final_state_hash
        ));
    }

    let race: [u8; 2] = serde_json::from_value(expected["race"].clone())
        .expect("expected.race is a pair of rack counts");
    if race != replay.race {
        failures.push(format!(
            "race:\n  golden:   {race:?}\n  produced: {:?}",
            replay.race
        ));
    }

    let records = expected["records"]
        .as_array()
        .expect("expected.records is an array");
    if records.len() != replay.records.len() {
        failures.push(format!(
            "records: golden has {}, produced {}",
            records.len(),
            replay.records.len()
        ));
    }
    for (index, (golden, produced)) in records.iter().zip(&replay.records).enumerate() {
        let produced = serde_json::to_value(produced).expect("an adjudication record serializes");
        if *golden != produced {
            failures.push(format!(
                "records[{index}]:\n  golden:   {golden}\n  produced: {produced}"
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
