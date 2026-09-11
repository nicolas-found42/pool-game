//! Fact derivation: what the rules tier consumes (#6 §5 / rules-break.md 2.7).
//!
//! The prototype reports facts only -- per-ball rail contacts, pocketed,
//! off-table, cue-ball pocketed, ball-ball contacts, simultaneity group ids
//! and rest state. The break-legality classifier at the bottom reproduces the
//! pinned *precedence table* of rules-break.md so a row can be judged
//! produced/not-produced; it is not a rules engine.

use crate::sim::{Fact, FactKind, Sim};
use crate::table::Rail;

#[derive(Clone, Debug, Default)]
pub struct ShotFacts {
    /// Distinct object balls with at least one counted rail contact (2.7 with
    /// the frozen-ball clause applied).
    pub distinct_object_balls_to_rails: usize,
    /// Total counted rail-contact events by object balls.
    pub rail_contacts_total: usize,
    /// Distinct (ball, rail) pairs touched by object balls.
    pub rail_pairs_total: usize,
    /// Contacts suppressed by the frozen-at-start clause.
    pub frozen_contacts_suppressed: usize,
    pub pocketed: Vec<u8>,
    pub off_table: Vec<u8>,
    pub cue_ball_pocketed: bool,
    /// Distinct object balls the cue ball contacted.
    pub cue_ball_contacts: usize,
    /// Object balls frozen to a rail at shot start, with that rail.
    pub frozen_at_start: Vec<(u8, Rail)>,
    /// Balls that both left a frozen rail and returned to it.
    pub frozen_left_and_returned: Vec<u8>,
    pub per_ball_rails: Vec<(u8, Vec<Rail>)>,
    pub ball_ball_contacts: usize,
}

impl ShotFacts {
    /// Physical-count reading of the corpus field (see RESULTS.md): the corpus
    /// rows count rail *contacts*, so a pocketed ball contributes no rail.
    pub fn distinct_physical(&self) -> usize {
        self.distinct_object_balls_to_rails
    }
    /// Rules 2.7 reading: pocketed and off-table balls also count.
    pub fn distinct_rule_2_7(&self) -> usize {
        let mut set: Vec<u8> = self
            .per_ball_rails
            .iter()
            .filter(|(_, r)| !r.is_empty())
            .map(|(b, _)| *b)
            .collect();
        for b in self.pocketed.iter().chain(self.off_table.iter()) {
            if !set.contains(b) {
                set.push(*b);
            }
        }
        set.len()
    }
}

pub fn derive(sim: &Sim) -> ShotFacts {
    let mut f = ShotFacts::default();
    let mut rail_map: Vec<(u8, Vec<Rail>)> = Vec::new();
    let mut cue_contacts: Vec<u8> = Vec::new();
    for fact in &sim.facts {
        match fact.kind {
            FactKind::RailContact {
                ball,
                rail,
                frozen_no_count,
            } => {
                if ball == 0 {
                    continue;
                }
                if frozen_no_count {
                    f.frozen_contacts_suppressed += 1;
                    continue;
                }
                f.rail_contacts_total += 1;
                let e = rail_map
                    .iter_mut()
                    .find(|(b, _)| *b == ball)
                    .map(|(_, v)| v);
                match e {
                    Some(v) => {
                        if !v.contains(&rail) {
                            v.push(rail);
                        }
                    }
                    None => rail_map.push((ball, vec![rail])),
                }
            }
            FactKind::Pocketed { ball, .. } => {
                if ball == 0 {
                    f.cue_ball_pocketed = true;
                } else if !f.pocketed.contains(&ball) {
                    f.pocketed.push(ball);
                }
            }
            FactKind::OffTable { ball } => {
                if ball == 0 {
                    f.cue_ball_pocketed = true;
                } else if !f.off_table.contains(&ball) {
                    f.off_table.push(ball);
                }
            }
            FactKind::BallBall { a, b } => {
                f.ball_ball_contacts += 1;
                let other = if a == 0 {
                    Some(b)
                } else if b == 0 {
                    Some(a)
                } else {
                    None
                };
                if let Some(o) = other {
                    if !cue_contacts.contains(&o) {
                        cue_contacts.push(o);
                    }
                }
            }
            _ => {}
        }
    }
    f.distinct_object_balls_to_rails = rail_map.iter().filter(|(_, v)| !v.is_empty()).count();
    f.rail_pairs_total = rail_map.iter().map(|(_, v)| v.len()).sum();
    f.per_ball_rails = rail_map;
    f.cue_ball_contacts = cue_contacts.len();
    f.pocketed.sort_unstable();
    f.off_table.sort_unstable();
    for b in &sim.balls {
        for r in &b.frozen_rails {
            f.frozen_at_start.push((b.id, *r));
        }
    }
    f
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    LegalClean,
    LegalPocketed,
    IllegalBreak,
    BreakFoul,
    EightOnBreak,
}

#[derive(Clone, Debug)]
pub struct Classification {
    pub legal_break: bool,
    pub tree: Tree,
    pub rule: &'static str,
}

/// The pinned precedence table of rules-break.md, evaluated from the facts.
pub fn classify(f: &ShotFacts, cue_ball_pocketed_or_off: bool) -> Classification {
    let eight_off = f.off_table.contains(&8);
    let rattled = f.pocketed.contains(&8);
    if rattled || eight_off {
        return Classification {
            legal_break: !cue_ball_pocketed_or_off && !eight_off,
            tree: Tree::EightOnBreak,
            rule: if eight_off { "4.3(g)" } else if cue_ball_pocketed_or_off { "4.3(f)" } else { "4.3(e)" },
        };
    }
    if !f.pocketed.is_empty() {
        // 4.3(c): pocketing makes the break legal; a foul still routes to 4.3(h)
        return Classification {
            legal_break: !cue_ball_pocketed_or_off && f.off_table.is_empty(),
            tree: if cue_ball_pocketed_or_off || !f.off_table.is_empty() {
                Tree::BreakFoul
            } else {
                Tree::LegalPocketed
            },
            rule: if cue_ball_pocketed_or_off { "4.3(h)" } else if !f.off_table.is_empty() { "4.3(g)" } else { "4.3(c)" },
        };
    }
    if f.distinct_object_balls_to_rails < 4 {
        return Classification {
            legal_break: false,
            tree: Tree::IllegalBreak,
            rule: "4.3(d)",
        };
    }
    Classification {
        legal_break: !cue_ball_pocketed_or_off && f.off_table.is_empty(),
        tree: if cue_ball_pocketed_or_off || !f.off_table.is_empty() {
            Tree::BreakFoul
        } else {
            Tree::LegalClean
        },
        rule: if cue_ball_pocketed_or_off { "4.3(h)" } else if !f.off_table.is_empty() { "4.3(g)" } else { "4.3(d)" },
    }
}

pub fn rail_name(r: Rail) -> &'static str {
    match r {
        Rail::HeadShort => "head_short",
        Rail::FootShort => "foot_short",
        Rail::LeftLong => "left_long",
        Rail::RightLong => "right_long",
    }
}

pub fn fact_name(k: &FactKind) -> String {
    match k {
        FactKind::BallBall { a, b } => format!("ball_ball {} {}", a, b),
        FactKind::RailContact { ball, rail, .. } => format!("rail {} {}", ball, rail_name(*rail)),
        FactKind::JawContact { ball, pocket } => format!("jaw {} {}", ball, pocket),
        FactKind::Pocketed { ball, pocket } => format!("pocketed {} {}", ball, pocket),
        FactKind::OffTable { ball } => format!("off_table {}", ball),
        FactKind::Kick { ball, .. } => format!("kick {}", ball),
        FactKind::Rest { ball } => format!("rest {}", ball),
        FactKind::SlideToRoll { ball } => format!("slide_to_roll {}", ball),
        FactKind::RollToStop { ball } => format!("roll_to_stop {}", ball),
        FactKind::SpinDown { ball } => format!("spin_down {}", ball),
        FactKind::Freeze { ball } => format!("freeze {}", ball),
        FactKind::Depenetration { a, b } => format!("depenetration {} {}", a, b),
    }
}

pub fn fact_json(f: &Fact, ball_ids: &[u8]) -> String {
    let id = ball_ids.get(0).copied().unwrap_or(0);
    let _ = id;
    format!(
        "{{\"seq\":{},\"t\":{:.6},\"group\":{},\"kind\":\"{}\"}}",
        f.seq,
        f.t,
        f.group,
        fact_name(&f.kind)
    )
}
