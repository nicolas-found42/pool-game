//! The profile record (`architecture.md` §12): `config/profiles/<id>.json` must parse into the
//! `Profile` the code defaults to, and carry its provenance and conditions.

use std::path::PathBuf;

use pool_sim::profile::Profile;

fn profile_path(id: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../config/profiles/{id}.json"))
}

#[test]
fn the_default_profile_record_matches_the_code_default() {
    let path = profile_path("default");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let record: Profile = serde_json::from_str(&text).expect("the record parses");
    assert_eq!(
        record,
        Profile::default_profile(),
        "the record and the code default must agree"
    );
    assert_eq!(record.id, "default");
    assert!(
        !record.provenance.sources.is_empty(),
        "the record carries its provenance"
    );
    assert!(
        !record.provenance.caveats.is_empty(),
        "the record carries what it does not claim"
    );
}

#[test]
fn a_record_with_an_unknown_key_is_rejected() {
    let text = std::fs::read_to_string(profile_path("default")).expect("read");
    let tampered = text.replace("\"mu_s\": 0.2,", "\"mu_s\": 0.2, \"mu_unknown\": 1.0,");
    assert!(
        serde_json::from_str::<Profile>(&tampered).is_err(),
        "unknown keys must not be ignored"
    );
}

#[test]
fn the_mu_b_record_is_the_coefficients_not_the_table() {
    let text = std::fs::read_to_string(profile_path("default")).expect("read");
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mu_b = value["mu_b"].as_object().expect("mu_b is an object");
    assert_eq!(
        mu_b.len(),
        3,
        "mu_b records a, b, c — the table is derived, never stored"
    );
    let record: Profile = serde_json::from_str(&text).expect("parses");
    assert!((record.mu_b.eval(0.0) - 0.117_951).abs() < 1e-12);
}
