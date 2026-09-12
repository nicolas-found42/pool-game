//! The physics tier's break corpus (`docs/spec/physics-break.json`) at its committed pins.
//!
//! `physics.md` §8 records the prototype's gate dispositions: **8 rows produced** (`pb-01`, `pb-02`,
//! `pb-04`, `pb-05`, `pb-06`, `pb-08`, `pb-13`, `pb-14`), each with the prototype's pinned aim and
//! speed, and six rows left unproduced with a recorded disposition. This suite runs every row that
//! carries a pin in the corpus and compares its `target_facts.expected` physics-tier fields: the rail
//! counts in `physics.md` §7's **physical** reading (distinct object balls that actually touched a
//! rail, excluding balls pocketed or driven off the table), the pocketed and off-table sets, the cue
//! ball's pocket, its ball contacts, and pb-13's frozen annotations. The `legal` / `tree` / `rule`
//! fields belong to the rules layer (`rules-break.md`) and are printed beside the comparison as a
//! cross-check of the pattern the row encodes, not asserted.
//!
//! ## The pins here are not all §8's
//!
//! §8's prototype pins were minted on a solver that fabricated ball–ball contacts (a pair's root was
//! solved against a segment law extrapolated past its validity; the Newton refinement then escaped
//! the root, the `t <= 0` clamp turned it into `t = 1e-9`, and `hit_pair` applied a full impulse
//! between balls a metre apart — the standard rack, cue at (−800, 0) straight down the string at
//! 1400 mm/s, emitted `BallBall { a: 0, b: 6 }` at 1634 mm of separation, and the prototype emits the
//! same class of phantom on the same shot). PR #38 repaired that, and §8 explicitly allows the
//! consequence — *the pattern is the requirement, the parameters are the pin* — so the six rows whose
//! pins no longer reproduced their patterns were **re-aimed at M1**: `pb-01`, `pb-02`, `pb-05`,
//! `pb-08`, `pb-13`, `pb-14` now carry the re-aimed `shot.aim_deg` / `shot.cue_speed_mm_s` in the
//! corpus (see its `generator.derivation` note), each searched level and spin-free over (aim, speed)
//! on the row's own arrangement and verified against the whole expected block. `PROTOTYPE_PINS` below
//! keeps §8's numbers so the report shows what changed; a row that stops reproducing now fails rather
//! than being tolerated, because the corpus carries a pin that does.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use pool_sim::constants::{BALL_RADIUS_MM, HALF_WIDTH_MM};
use pool_sim::facts::{FactKind, rail_summary};
use pool_sim::frame::Slot;
use pool_sim::rack::Arrangement;
use pool_sim::sim::{Shot, Sim};
use pool_sim::strike::StrikeDecl;
use pool_sim::table::Table;
use serde_json::{Value, json};

/// `physics.md` §8's prototype pins for the eight produced rows — the record of what the re-aim
/// moved away from (aim in degrees, speed in mm/s). Every one of them was level, with no spin.
const PROTOTYPE_PINS: [(&str, f64, f64); 8] = [
    ("pb-01", -2.000, 1400.0),
    ("pb-02", -1.000, 1400.0),
    ("pb-04", -4.721, 550.0),
    ("pb-05", -1.679, 2400.0),
    ("pb-06", 1.821, 2400.0),
    ("pb-08", 1.000, 2700.0),
    ("pb-13", -1.007, 3100.0),
    ("pb-14", -3.282, 3100.0),
];

/// The six rows §8 leaves unproduced, with the disposition it records for each: no parameters are
/// invented for them, and the suite reports them instead of failing.
const DISPOSITIONS: [(&str, &str); 6] = [
    (
        "pb-03",
        "the pattern is reached with ball 3 pocketed (3/3, 4.3(c)); the row pins ball 12 — amend the \
         row's ball number or re-pin it",
    ),
    (
        "pb-07",
        "no off-table event exists at the default e_slate; conditional on physics.md §3.4",
    ),
    (
        "pb-09",
        "no off-table event exists at the default e_slate; conditional on physics.md §3.4",
    ),
    (
        "pb-10",
        "the 8 is pocketed with the right tree, but 11 distinct balls reach rails, not 0 — the row \
         is over-constrained",
    ),
    (
        "pb-11",
        "no run with the 8 pocketed *and* the cue ball pocketed *and* zero rails — over-constrained \
         as written",
    ),
    (
        "pb-12",
        "needs an off-table event; conditional on physics.md §3.4",
    ),
];

/// The `target_facts.expected` fields this tier produces. `legal`, `tree`, and `rule` are the rules
/// layer's, so the comparison never reaches for them: they are reported beside it.
const COMPARABLE: [&str; 8] = [
    "distinct_object_balls_to_rails",
    "rail_contacts_total",
    "pocketed",
    "off_table",
    "cue_ball_pocketed",
    "object_ball_contacts",
    "frozen_to_rail_at_shot_start",
    "frozen_ball_left_and_returned",
];

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec")
}

fn corpus() -> Value {
    let path = spec_dir().join("physics-break.json");
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

/// A row's committed pin: `(aim in degrees, launch speed in mm/s)`, or `None` when it is one of the
/// rows §8 leaves unproduced. `spin` and `elevation_deg` stay null — a pin is level, with no spin.
fn pin(row: &Value) -> Option<(f64, f64)> {
    let aim = row["shot"]["aim_deg"].as_f64();
    let speed = row["shot"]["cue_speed_mm_s"].as_f64();
    match (aim, speed) {
        (None, None) => None,
        (Some(aim), Some(speed)) => {
            assert!(
                row["shot"]["spin"].is_null() && row["shot"]["elevation_deg"].is_null(),
                "{}: a pinned row is level with no spin",
                row["id"]
            );
            Some((aim, speed))
        }
        _ => panic!(
            "{}: a pin carries both aim_deg and cue_speed_mm_s",
            row["id"]
        ),
    }
}

/// Run a pinned row: the corpus' aim is the absolute aim angle (`physics.md` §8's reading), and the
/// pin's frozen pre-state moves the ball of rack slot 5.0 to (700, 606.425) mm — the exact-contact
/// value `HALF_WIDTH − R` that §8 renders as 606.4 — onto the right long cushion.
fn run_pinned(row: &Value, aim_deg: f64, speed_mm_s: f64) -> Shot {
    let arrangement = arrangement_from_json(&row["arrangement"]);
    let mut sim = Sim::new(Table::new(), arrangement);
    if row["target_facts"]["expected"]["frozen_to_rail_at_shot_start"].as_bool() == Some(true) {
        let ball = arrangement.ball_at(Slot::parse("5.0").expect("slot 5.0"));
        sim.place_ball(ball, [700.0, HALF_WIDTH_MM - BALL_RADIUS_MM])
            .expect("the frozen pin is a legal placement");
    }
    let cue = [
        row["shot"]["cue_ball"]["x_mm"].as_f64().expect("cue x_mm"),
        row["shot"]["cue_ball"]["y_mm"].as_f64().expect("cue y_mm"),
    ];
    sim.place_cue(cue)
        .expect("the cue pin is a legal placement");
    let rad = aim_deg.to_radians();
    let decl = StrikeDecl {
        aim: [rad.cos(), rad.sin()],
        speed_mm_s,
        spin: [0.0, 0.0],
        elevation_rad: 0.0,
    };
    sim.strike(decl).expect("a pinned strike is valid")
}

/// The physics tier's produced facts, in the corpus' own field names and §7 count semantics.
fn produced(shot: &Shot) -> Value {
    let rest = shot.rest();
    let summary = rail_summary(shot.events(), &rest.states);

    let mut pocketed: Vec<u8> = Vec::new();
    let mut object_ball_contacts: Vec<u8> = Vec::new();
    let mut frozen_contact = false;
    let mut left_and_returned = false;
    for fact in shot.events() {
        match fact.kind {
            // `pocketed` carries object balls only: the cue ball's pocket is its own field in this
            // corpus' vocabulary (and in `rules-break.md`'s trees).
            FactKind::Pocketed { ball, .. } if ball != 0 => pocketed.push(ball),
            FactKind::BallBall { a, b } if a == 0 || b == 0 => {
                let other = if a == 0 { b } else { a };
                if !object_ball_contacts.contains(&other) {
                    object_ball_contacts.push(other);
                }
            }
            FactKind::RailContact {
                frozen_at_shot_start,
                left_since_shot_start,
                ..
            } => {
                frozen_contact |= frozen_at_shot_start;
                left_and_returned |= frozen_at_shot_start && left_since_shot_start;
            }
            _ => {}
        }
    }
    pocketed.sort_unstable();
    object_ball_contacts.sort_unstable();

    // The shot-start frozen annotation is observable only through a rail contact's field pair and
    // the rest block (`physics.md` §7, `rules.md` §10.2): a ball frozen at the shot's start that
    // never leaves its rail emits no contact at all, so its annotation is read from the rest block.
    let frozen_at_start = frozen_contact || !rest.frozen_rails.is_empty();

    json!({
        "distinct_object_balls_to_rails": summary.distinct_object_balls,
        "rail_contacts_total": summary.contacts,
        "pocketed": pocketed,
        "off_table": rest.off_table,
        "cue_ball_pocketed": rest.pocketed.contains(&0),
        "object_ball_contacts": object_ball_contacts.len(),
        "frozen_to_rail_at_shot_start": frozen_at_start,
        "frozen_ball_left_and_returned": left_and_returned,
    })
}

/// The per-field comparison of a row's produced facts against its target.
fn differences(expected: &Value, actual: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for key in COMPARABLE {
        if expected.get(key).is_none() {
            continue; // the row does not pin this field
        }
        let (want, got) = (&expected[key], &actual[key]);
        if want != got {
            out.push(format!("`{key}`: target {want}, produced {got}"));
        }
    }
    out
}

#[test]
fn pinned_rows_reproduce_their_target_facts() {
    let doc = corpus();
    let rows = doc["entries"].as_array().expect("entries is an array");
    let mut report = String::new();
    let mut failures: Vec<String> = Vec::new();
    let mut pinned = 0;

    for (id, prototype_aim, prototype_speed) in PROTOTYPE_PINS {
        let row = rows
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("{id} is not in physics-break.json"));
        let Some((aim_deg, speed_mm_s)) = pin(row) else {
            failures.push(format!(
                "{id} carries no pin: physics-break.json has aim_deg/cue_speed_mm_s null, so the row \
                 that §8 records as produced is not exercised"
            ));
            continue;
        };
        pinned += 1;
        let shot = run_pinned(row, aim_deg, speed_mm_s);
        let actual = produced(&shot);
        let expected = &row["target_facts"]["expected"];
        let diffs = differences(expected, &actual);

        let re_aimed = if (aim_deg, speed_mm_s) == (prototype_aim, prototype_speed) {
            String::new()
        } else {
            format!(" (re-aimed from §8's {prototype_aim:.3}°, {prototype_speed:.0} mm/s)")
        };
        let _ = writeln!(
            report,
            "\n{id} at aim {aim_deg:.3}°, {speed_mm_s:.0} mm/s{re_aimed}"
        );
        if diffs.is_empty() {
            let _ = writeln!(
                report,
                "  reproduces its target_facts: distinct {}, contacts {}, pocketed {}, off table \
                 {}, cue ball pocketed {}",
                actual["distinct_object_balls_to_rails"],
                actual["rail_contacts_total"],
                actual["pocketed"],
                actual["off_table"],
                actual["cue_ball_pocketed"]
            );
        } else {
            for diff in &diffs {
                let _ = writeln!(report, "  {diff}");
            }
            failures.push(format!("{id}: {}", diffs.join("; ")));
        }
        let _ = writeln!(
            report,
            "  rules layer (reported, not asserted): legal {}, tree {}, rule {}",
            expected["legal"], expected["tree"], expected["rule"]
        );
    }

    assert_eq!(
        pinned,
        PROTOTYPE_PINS.len(),
        "every row §8 records as produced must carry a pin in the corpus"
    );
    println!("{report}");
    assert!(
        failures.is_empty(),
        "the pinned rows do not reproduce their target_facts:{report}"
    );
}

#[test]
fn unpinned_rows_report_their_recorded_disposition() {
    let doc = corpus();
    let rows = doc["entries"].as_array().expect("entries is an array");
    let mut reported = 0;

    for (id, disposition) in DISPOSITIONS {
        let row = rows
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("{id} is not in physics-break.json"));
        // The corpus' null parameters are requirements, not gaps: a row that gains a pin belongs in
        // the pinned set above, and this fails loudly rather than silently skipping it.
        assert!(
            pin(row).is_none(),
            "{id} now carries a pin; it is exercised by the pinned set, not reported as unproduced"
        );
        println!("{id}: not exercised — {disposition}");
        reported += 1;
    }
    assert_eq!(reported, DISPOSITIONS.len());
}
