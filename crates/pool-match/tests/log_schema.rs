//! The input-log types against `docs/spec/input-log.schema.json`: a full round trip, the schema's
//! `additionalProperties: false` and tagged unions, and the numeric bounds.

use std::path::PathBuf;

use pool_match::log::{InputLog, LogError};

/// The published schema, compiled once per test.
fn schema_validator() -> jsonschema::Validator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/input-log.schema.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let schema: serde_json::Value = serde_json::from_str(&text).expect("the schema parses");
    jsonschema::validator_for(&schema).expect("the schema compiles")
}

fn assert_schema_rejects(text: &str, why: &str) {
    let validator = schema_validator();
    let instance: serde_json::Value = serde_json::from_str(text).expect("the case is JSON");
    assert!(
        !validator.is_valid(&instance),
        "the schema accepted a document that should be rejected: {why}"
    );
}

const FULL_LOG: &str = r#"{
  "format_version": 1,
  "profile": "default",
  "match_seed": 12345678901234567890,
  "noise_seed": 7,
  "difficulty": { "level": "intermediate", "checkpoint": "sha256:abc" },
  "race_target": 5,
  "entries": [
    { "kind": "placement", "domain": "above_head_string", "pos": { "x": -800.0, "y": 0.0 } },
    { "kind": "declaration", "from_policy": false, "call": { "type": "break" },
      "aim": { "x": 1.0, "y": 0.0 }, "speed": 7000.0, "spin": { "a": 0.0, "b": 0.0 }, "elevation": 0.0 },
    { "kind": "declaration", "from_policy": true, "call": { "type": "ball", "ball": 3, "pocket": "foot_left" },
      "aim": { "x": 0.5, "y": 0.8660254037844386 }, "speed": 2100.0,
      "spin": { "a": 0.25, "b": -0.5 }, "elevation": 0.17453292519943295 },
    { "kind": "option", "option_id": "illegal_break.re_rack" },
    { "kind": "declaration", "from_policy": false, "call": { "type": "safety" },
      "aim": { "x": 0.0, "y": 1.0 }, "speed": 900.0, "spin": { "a": 0.0, "b": 0.0 }, "elevation": 0.0 },
    { "kind": "spot_request" }
  ]
}"#;

#[test]
fn a_full_log_round_trips_through_the_schema_types() {
    let log = InputLog::parse(FULL_LOG).expect("the reference log parses");
    assert_eq!(log.entries.len(), 6);
    let json = log.to_json().expect("serializes");
    let again = InputLog::parse(&json).expect("the serialized form parses");
    assert_eq!(log, again, "round trip must be exact");
}

#[test]
fn the_reference_log_validates_against_the_published_schema() {
    let validator = schema_validator();
    let instance: serde_json::Value = serde_json::from_str(FULL_LOG).expect("the log is JSON");
    if let Err(error) = validator.validate(&instance) {
        panic!("the reference log does not satisfy input-log.schema.json: {error}");
    }
}

#[test]
fn the_schema_rejects_what_it_says_it_rejects() {
    // `additionalProperties: false` at the top level.
    assert_schema_rejects(
        &FULL_LOG.replace("\"race_target\": 5,", "\"race_target\": 5, \"extra\": 1,"),
        "an unknown top-level key",
    );
    // An unknown entry kind.
    assert_schema_rejects(
        &FULL_LOG.replace("\"kind\": \"spot_request\"", "\"kind\": \"nudge\""),
        "an unknown entry kind",
    );
    // A ball outside 1..=15.
    assert_schema_rejects(&FULL_LOG.replace("\"ball\": 3", "\"ball\": 16"), "ball 16");
    // The tagged union admits no extra keys on a variant.
    assert_schema_rejects(
        &FULL_LOG.replace(
            "{ \"type\": \"safety\" }",
            "{ \"type\": \"safety\", \"ball\": 3 }",
        ),
        "a safety carrying a ball",
    );
    // An unknown difficulty level.
    assert_schema_rejects(
        &FULL_LOG.replace("\"level\": \"intermediate\"", "\"level\": \"expert\""),
        "an unknown difficulty level",
    );
}

#[test]
fn the_types_reject_what_the_schema_rejects_where_serde_can() {
    // The types are the in-code representation; they enforce the shape serde can express (unknown
    // fields on structs, the tagged unions, the ball range) and `validate` adds the numeric bounds.
    // Where serde cannot (extra keys on an internally tagged variant — a documented serde limitation),
    // the schema check above is the enforcement.
    let extra = FULL_LOG.replace("\"race_target\": 5,", "\"race_target\": 5, \"extra\": 1,");
    assert!(matches!(InputLog::parse(&extra), Err(LogError::Parse(_))));

    let bad_ball = FULL_LOG.replace("\"ball\": 3", "\"ball\": 16");
    assert!(matches!(
        InputLog::parse(&bad_ball),
        Err(LogError::Parse(_))
    ));

    let bad_level = FULL_LOG.replace("\"level\": \"intermediate\"", "\"level\": \"expert\"");
    assert!(matches!(
        InputLog::parse(&bad_level),
        Err(LogError::Parse(_))
    ));
}

#[test]
fn the_numeric_bounds_the_schema_states_are_enforced() {
    let zero_target = FULL_LOG.replace("\"race_target\": 5", "\"race_target\": 0");
    assert!(matches!(
        InputLog::parse(&zero_target),
        Err(LogError::Invalid(_))
    ));

    let zero_version = FULL_LOG.replace("\"format_version\": 1", "\"format_version\": 0");
    assert!(matches!(
        InputLog::parse(&zero_version),
        Err(LogError::Invalid(_))
    ));

    let negative_speed = FULL_LOG.replace("\"speed\": 900.0", "\"speed\": -900.0");
    assert!(matches!(
        InputLog::parse(&negative_speed),
        Err(LogError::Invalid(_))
    ));
}

#[test]
fn the_spin_contract_carries_the_envelope_fraction() {
    let log = InputLog::parse(FULL_LOG).expect("parses");
    let pool_match::Entry::Declaration(declaration) = &log.entries[2] else {
        panic!("entry 2 is a declaration");
    };
    assert_eq!(declaration.spin.a, 0.25);
    assert_eq!(declaration.spin.b, -0.5);
    assert!((declaration.spin.magnitude() - 0.559_016_994_374_947_5).abs() < 1e-12);
}
