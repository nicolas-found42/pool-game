//! The CLI as it is run (`architecture.md` §7): the fixture replays and exits 0, a tampered log exits
//! non-zero naming the mismatch, and `strike` computes one shot from a seed and a declaration.
//!
//! The binary is the workspace's own: the tests spawn `pool-headless` from the repository root, where
//! `config/profiles/<id>.json` resolves.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

/// The committed fixture, from the repository root.
const FIXTURE: &str = "data/matches/break-foul-ball-in-hand-eight-loss.json";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pool-headless"))
        .args(args)
        .current_dir(root())
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pool-headless-{}-{name}", std::process::id()))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// The final state hash the golden replay pins for the fixture: the CLI's report must agree with it.
fn golden_hash() -> String {
    let text = read(&root().join("docs/spec/goldens.json"));
    let doc: Value = serde_json::from_str(&text).expect("goldens.json parses");
    let entries = doc["entries"].as_array().expect("entries is an array");
    let entry = entries
        .iter()
        .find(|entry| entry["kind"] == json!("replay") && entry["input"]["log"] == json!(FIXTURE))
        .expect("the fixture has a golden replay entry");
    entry["expected"]["final_state_hash"]
        .as_str()
        .expect("the entry pins a final state hash")
        .to_string()
}

/// A copy of the fixture with `edit` applied, written where the binary can read it.
fn tampered(name: &str, edit: impl FnOnce(&mut Value)) -> PathBuf {
    let mut log: Value =
        serde_json::from_str(&read(&root().join(FIXTURE))).expect("the fixture parses");
    edit(&mut log);
    let path = temp(name);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&log).expect("the tampered log serializes"),
    )
    .expect("the tampered log writes");
    path
}

fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn replay_runs_the_fixture_and_reports_its_final_state() {
    let records = temp("records.jsonl");
    let events = temp("events.jsonl");
    let output = run(&[
        "replay",
        FIXTURE,
        "--emit",
        records.to_str().expect("a path"),
        "--emit-events",
        events.to_str().expect("a path"),
    ]);
    let out = stdout(&output);
    assert!(output.status.success(), "exit {:?}\n{out}", output.status);
    assert!(out.contains("p2 wins the race 0-1"), "{out}");
    assert!(
        out.contains("entries[2] declaration: Foul standard_foul (3.1)"),
        "{out}"
    );
    assert!(
        out.contains(&format!("final state hash: {}", golden_hash())),
        "the report must agree with the golden entry:\n{out}"
    );

    let records_text = read(&records);
    assert_eq!(records_text.lines().count(), 5, "{records_text}");
    let events_text = read(&events);
    assert!(
        events_text.lines().count() > 100,
        "{} facts",
        events_text.lines().count()
    );
    let first: Value =
        serde_json::from_str(events_text.lines().next().expect("a fact")).expect("a fact is JSON");
    assert!(first["kind"].is_object(), "{first}");
    cleanup(&[records, events]);
}

#[test]
fn a_tampered_log_exits_non_zero_naming_the_mismatch() {
    let cases: Vec<(&str, PathBuf, &str)> = vec![
        (
            "dropped-placement",
            tampered("dropped-placement", |log| {
                log["entries"].as_array_mut().expect("entries").remove(0);
            }),
            "entries[0]: the state awaits awaiting_placement, not a declaration",
        ),
        (
            "placement-out-of-domain",
            tampered("placement-out-of-domain", |log| {
                log["entries"][0]["pos"] = json!({ "x": 0.0, "y": 0.0 });
            }),
            "entries[0]: the placement was rejected: OutsideDomain",
        ),
        (
            "zero-speed",
            tampered("zero-speed", |log| {
                log["entries"][1]["speed"] = json!(0.0);
            }),
            "entries[1]: the strike is not legal: BadSpeed",
        ),
        (
            "truncated",
            tampered("truncated", |log| {
                log["entries"].as_array_mut().expect("entries").pop();
            }),
            "the log ends with the match unfinished (awaiting_shot)",
        ),
    ];
    for (name, path, expected) in &cases {
        let output = run(&["replay", path.to_str().expect("a path")]);
        let err = stderr(&output);
        assert!(
            !output.status.success(),
            "{name}: the tampered log must be refused\n{err}"
        );
        assert!(
            err.contains(expected),
            "{name}: the error must name the mismatch\n  expected: {expected}\n  got:      {err}"
        );
    }
    cleanup(
        &cases
            .into_iter()
            .map(|(_, path, _)| path)
            .collect::<Vec<_>>(),
    );
}

#[test]
fn an_unknown_option_is_refused_by_the_option_vocabulary() {
    // A break that misses the rack: an illegal break, so the state presents 4.3(d)'s tree.
    let log = json!({
        "format_version": 1,
        "profile": "default",
        "match_seed": 11,
        "noise_seed": 1,
        "difficulty": { "level": "pro", "checkpoint": "test" },
        "race_target": 1,
        "entries": [
            { "kind": "placement", "domain": "above_head_string", "pos": { "x": -800.0, "y": 0.0 } },
            { "kind": "declaration", "from_policy": false, "call": { "type": "break" },
              "aim": { "x": 0.0, "y": 1.0 }, "speed": 1200.0,
              "spin": { "a": 0.0, "b": 0.0 }, "elevation": 0.0 },
            { "kind": "option", "option_id": "nudge" }
        ]
    });
    let path = temp("unknown-option.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&log).expect("serializes"),
    )
    .expect("writes");
    let output = run(&["replay", path.to_str().expect("a path")]);
    let err = stderr(&output);
    assert!(!output.status.success(), "{err}");
    assert!(
        err.contains("entries[2]: option id \"nudge\" is not in the rules vocabulary"),
        "{err}"
    );
    cleanup(&[path]);
}

#[test]
fn strike_computes_one_shot_from_a_seed_and_a_declaration() {
    let strike_file = temp("strike.json");
    let events = temp("strike-events.jsonl");
    std::fs::write(
        &strike_file,
        r#"{"aim":{"x":1.0,"y":0.0},"speed":6200.0,"spin":{"a":0.0,"b":0.0},"elevation":0.0}"#,
    )
    .expect("the strike file writes");

    let output = run(&[
        "strike",
        "--rack-seed",
        "1",
        "--placement",
        "-800,0",
        "--strike-file",
        strike_file.to_str().expect("a path"),
        "--emit-events",
        events.to_str().expect("a path"),
    ]);
    let out = stdout(&output);
    assert!(
        output.status.success(),
        "exit {:?}\n{out}{}",
        output.status,
        stderr(&output)
    );
    assert!(out.contains("rest state hash: "), "{out}");
    assert!(out.contains("shot: rest at "), "{out}");
    let events_text = read(&events);
    assert!(
        events_text.lines().count() > 10,
        "{} facts",
        events_text.lines().count()
    );

    // A declaration past the miscue envelope is rejected at the boundary, naming the reason.
    std::fs::write(
        &strike_file,
        r#"{"aim":{"x":1.0,"y":0.0},"speed":6200.0,"spin":{"a":2.0,"b":0.0},"elevation":0.0}"#,
    )
    .expect("the strike file writes");
    let output = run(&[
        "strike",
        "--rack-seed",
        "1",
        "--strike-file",
        strike_file.to_str().expect("a path"),
    ]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("Miscue"), "{}", stderr(&output));
    cleanup(&[strike_file, events]);
}
