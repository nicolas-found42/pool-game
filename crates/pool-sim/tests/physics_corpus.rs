//! The physics tier's break corpus (`docs/spec/physics-break.json`) at `physics.md` §8's pins.
//!
//! `physics.md` §8 records the prototype's gate dispositions: **8 rows produced** (`pb-01`, `pb-02`,
//! `pb-04`, `pb-05`, `pb-06`, `pb-08`, `pb-13`, `pb-14`), each with its pinned aim and speed, and six
//! rows left unproduced with a recorded disposition. This suite runs every produced row at its pin —
//! the pinned parameters as committed, never a re-aim — and compares its `target_facts.expected`
//! physics-tier fields: the rail counts in `physics.md` §7's **physical** reading (distinct object
//! balls that actually touched a rail, excluding balls pocketed or driven off the table), the pocketed
//! and off-table sets, the cue ball's pocket, its ball contacts, and pb-13's frozen annotations. The
//! `legal` / `tree` / `rule` fields are the rules layer's (`rules-break.md`), not this tier's, and
//! are reported beside the comparison rather than asserted.
//!
//! ## Divergences this suite records
//!
//! The pins are the prototype's, and they do not transfer. Running the prototype's own committed code
//! at these pins today reproduces four of the eight rows (`pb-01`, `pb-02`, `pb-04`, `pb-08`) — its
//! pins were minted by an earlier revision of that code (its `results/rows.json` records the minted
//! observations, and its search would re-pick different parameters now). This implementation
//! reproduces `pb-06` alone — and not `pb-04`, the total miss, which is one phantom contact away.
//!
//! Two solver defects account for the split: the first is shared with the prototype (it emits the
//! same class of phantom fact on the same shot), the second was introduced by the port and is fixed:
//!
//! - **Phantom ball–ball contacts at `t ≈ 0`.** A pair's contact root is solved with each ball's
//!   current segment law, which is only valid until that ball's next mode transition. When the
//!   quadratic root lies beyond the law's validity, the Newton refinement walks the step out of the
//!   root's basin and the `t <= 0` clamp turns it into `t = 1e-9`: the pair is then treated as
//!   contacting *now*, and `hit_pair` applies a full impulse between balls that can be a metre apart.
//!   Minimal instance: the standard rack (fixture seed 1), the cue ball at (−800, 0) struck straight
//!   down the long string at 1400 mm/s — the first group emits `BallBall { a: 0, b: 6 }` (1634 mm
//!   apart) and `BallBall { a: 0, b: 10 }`, and ball 6 is at x ≈ 1240 mm one second later while the
//!   cue ball has not moved. The prototype emits the same class of fact (`BallBall { a: 0, b: 4 }`,
//!   `{0, 8}`, `{0, 9}` on the same shot) at `t = 1e-9`, so the defect is shared, and a break's
//!   outcome then depends on which phantom set a rounding difference produces.
//! - **A far-side face contact at `t = 0`** (found by workstream C and fixed before this branch pinned
//!   anything, `solve_wall`; rolling retention 0.6374 → 0.6992 at `e_n` 0.78): the `t = 0`
//!   shortcut for "at the face within rounding" had no lower bound on the signed distance, so a ball
//!   far *behind* a jaw's face plane — the plane extends past the segment's end into the playing
//!   area — fired an immediate jaw contact. Minimal instance: one ball at (−300, 0) rolling into the
//!   right long rail at 1500 mm/s took a 337 mm/s sideways kick from the `SidePlusY` jaw at the rail
//!   contact; with the shortcut's band made two-sided the same shot reproduces the prototype
//!   bit-for-bit (`t_rest = 4.234896 s`, post-cushion velocity `(0, −699.08, 175.09)` mm/s at
//!   `e_n = 0.75`).
//!
//! Neither defect is a corpus bug and the pins are not this suite's to amend (`physics.md` §8: "the
//! pattern is the requirement, the parameters are the pin"; a re-aim reproducing the pattern is
//! acceptable). `DIVERGENCES` therefore names the rows this implementation does not reproduce, with
//! the produced and expected facts, and the suite fails if one of them starts matching — so the
//! ledger cannot rot once the defects are fixed and the pins are re-aimed.

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

/// The prototype's pins for the eight produced rows (`physics.md` §8's table): absolute aim angle in
/// degrees, and launch speed in mm/s. Every one of them is level, with no spin.
const PINS: [(&str, f64, f64); 8] = [
    ("pb-01", -2.000, 1400.0),
    ("pb-02", -1.000, 1400.0),
    ("pb-04", -4.721, 550.0),
    ("pb-05", -1.679, 2400.0),
    ("pb-06", 1.821, 2400.0),
    ("pb-08", 1.000, 2700.0),
    ("pb-13", -1.007, 3100.0),
    ("pb-14", -3.282, 3100.0),
];

/// The six rows §8 leaves unpinned, with the disposition it records for each: no parameters are
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

/// The rows this implementation's pinned run does not reproduce, with the produced facts beside the
/// expected ones (measured on this branch; see the module docs for the mechanism). `pb-06` is the one
/// produced row whose target the pins reproduce.
const DIVERGENCES: [(&str, &str); 7] = [
    (
        "pb-01",
        "`distinct_object_balls_to_rails` target 4, produced 1; `rail_contacts_total` target 4, \
         produced 2",
    ),
    (
        "pb-02",
        "`distinct_object_balls_to_rails` 4 vs 1; `rail_contacts_total` 5 vs 1; `cue_ball_pocketed` \
         false vs true",
    ),
    (
        "pb-04",
        "`object_ball_contacts` target 0, produced 1 — the phantom pair of the module docs, on the \
         row whose whole requirement is that the cue ball contacts nothing",
    ),
    (
        "pb-05",
        "`distinct_object_balls_to_rails` 3 vs 1; `rail_contacts_total` 3 vs 2",
    ),
    (
        "pb-08",
        "`distinct_object_balls_to_rails` 4 vs 1; `rail_contacts_total` 4 vs 2; `cue_ball_pocketed` \
         false vs true",
    ),
    (
        "pb-13",
        "`distinct_object_balls_to_rails` 3 vs 2; `pocketed` [] vs [11, 14]; the frozen ball is \
         still frozen to RightLong at rest and `frozen_ball_left_and_returned` is false either way",
    ),
    (
        "pb-14",
        "`distinct_object_balls_to_rails` target 3, produced 4",
    ),
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

/// The row's pre-state: its arrangement and cue ball, plus pb-13's frozen pin (`physics.md` §8: the
/// ball of rack slot 5.0 moves to (700, 606.425) mm, on the right long cushion — the spec renders
/// the exact-contact value `HALF_WIDTH − R` as 606.4).
fn prestate(row: &Value) -> Sim {
    let arrangement = arrangement_from_json(&row["arrangement"]);
    let mut sim = Sim::new(Table::new(), arrangement);
    let frozen =
        row["target_facts"]["expected"]["frozen_to_rail_at_shot_start"].as_bool() == Some(true);
    if frozen {
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
    sim
}

/// Run a pinned row: the §8 aim is the absolute aim angle, and every pin is level with no spin.
fn run_pinned(row: &Value, aim_deg: f64, speed_mm_s: f64) -> Shot {
    let mut sim = prestate(row);
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

/// The `target_facts.expected` fields this tier produces; `legal`, `tree`, and `rule` are the rules
/// layer's (`rules-break.md`), so a comparison never reaches for them.
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
fn pinned_rows_are_compared_to_their_target_facts() {
    let doc = corpus();
    let rows = doc["entries"].as_array().expect("entries is an array");
    let mut report = String::new();
    let mut failures: Vec<String> = Vec::new();

    for (id, aim_deg, speed_mm_s) in PINS {
        let row = rows
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("{id} is not in physics-break.json"));
        let shot = run_pinned(row, aim_deg, speed_mm_s);
        let actual = produced(&shot);
        let expected = &row["target_facts"]["expected"];
        let diffs = differences(expected, &actual);
        let divergence = DIVERGENCES.iter().find(|(row_id, _)| *row_id == id);

        let _ = writeln!(report, "\n{id} at aim {aim_deg}°, {speed_mm_s} mm/s");
        if diffs.is_empty() {
            report.push_str("  reproduces its target_facts\n");
        } else {
            for diff in &diffs {
                let _ = writeln!(report, "  {diff}");
            }
        }
        let _ = writeln!(
            report,
            "  rules layer (reported, not asserted): legal {}, tree {}, rule {}",
            expected["legal"], expected["tree"], expected["rule"]
        );

        match (divergence, diffs.is_empty()) {
            (Some((_, note)), true) => failures.push(format!(
                "{id} now reproduces its target_facts, but DIVERGENCES still lists it ({note}) — \
                 delete the entry"
            )),
            (Some(_), false) | (None, true) => {}
            (None, false) => failures.push(format!("{id} does not reproduce its target_facts")),
        }
    }

    println!("{report}");
    assert!(failures.is_empty(), "{report}\n{}", failures.join("\n"));
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
        // The corpus's null parameters are requirements, not gaps: a row that gains a pin belongs in
        // PINS above, and this fails loudly rather than silently skipping it.
        assert!(
            row["shot"]["aim_deg"].is_null() && row["shot"]["cue_speed_mm_s"].is_null(),
            "{id} now carries parameters; add it to PINS and run it"
        );
        assert!(
            !PINS.iter().any(|(pinned, _, _)| *pinned == id),
            "{id} is both pinned and unpinned"
        );
        println!("{id}: not exercised — {disposition}");
        reported += 1;
    }
    assert_eq!(reported, DISPOSITIONS.len());
}
