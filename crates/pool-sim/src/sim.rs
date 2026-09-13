//! The analytic event-driven core (`physics.md` §1/§3) and the shot facade of `architecture.md` §4.
//!
//! State advances event to event: every candidate event time (ball–ball, ball–cushion, ball–jaw,
//! pocket drop, landing, motion-mode transition) is solved in closed form, the minimum is taken, the
//! impulse or transition is applied, and the loop repeats. Contacts are *solved*, never sampled, so a
//! break-speed ball cannot tunnel.
//!
//! Every ball carries a local analytic law, and the shot keeps its segment timeline, so
//! [`Shot::state_at`] is an exact evaluation — never an interpolation.

use crate::ball::{BallState, MotionMode};
use crate::constants::{
    BALL_DIAMETER_MM, BALL_INERTIA_G_MM2, BALL_MASS_G, BALL_RADIUS_MM, CONTACT_SLOP_MM,
    CUSHION_CLEARANCE_MM, CUSHION_NORMAL_Z, DROP_BOUNDARY_TOLERANCE_MM, FROZEN_GAP_MM,
    GRAVITY_MM_S2, HALF_LEN_MM, HALF_WIDTH_MM, SIMULTANEITY_EPS_S, SLEEP_ANGULAR_RAD_S,
    SLEEP_LINEAR_MM_S, cushion_normal_horizontal,
};
use crate::facts::{Fact, FactKind, KickCause};
use crate::math::{V3, v3};
use crate::rack::Arrangement;
use crate::strike::{StrikeDecl, StrikeError};
use crate::table::{DropShape, Pocket, PocketId, Rail, Table, Wall};

/// The group-clearance loop's no-progress counter: consecutive groups that advance no time are the
/// stall the guard of `physics.md` §1 exists for. A legitimate chain — a group's own impulses
/// re-checking at the same instant — is bounded by the contact graph (a rack's is tens of groups), so
/// the limit sits an order of magnitude above any real chain and far below a runaway.
const STALL_LIMIT: u32 = 1_000;
/// The default event cap per shot (`Sim::max_events`).
pub const DEFAULT_MAX_EVENTS: u64 = 400_000;
/// The default time cap per shot, in seconds (`Sim::max_time_s`).
pub const DEFAULT_MAX_TIME_S: f64 = 240.0;
/// The Newton refinement's iteration count in the root finders. The refinement starts from the
/// constant-velocity quadratic root and converges quadratically, so the count is the accuracy bound:
/// four iterations leave a fast sliding pair's residual at a few nanometres — larger than the
/// contact slop, so a *converged* root would read as a miss and the pair would tunnel — while
/// sixteen takes the committed cases' residual five orders below it. The loop is bounded, not
/// unbounded, because a pair whose true trajectory never reaches contact has no root to converge to.
const ROOT_ITERATIONS: usize = 16;
/// The approach floor (mm/s): a pair whose relative normal speed is below it is touching, not
/// approaching. Two balls sliding tangentially in contact differ by rounding noise at this scale
/// (1e-18 mm/s at pool speeds), and an event fired on that noise neither moves the state nor advances
/// the clock — a group that makes no progress; worse, the rest snap that zeroes one ball's velocity
/// can flip that noise's sign for ever.
const APPROACH_FLOOR_MM_S: f64 = 1e-9;
/// How many times the position-only separation step sweeps the table before it gives up.
const DEPENETRATION_PASSES: u32 = 8;
/// One ball's local analytic law, valid from its segment's start:
///
/// ```text
/// p(τ) = p0 + v0·τ + a·τ²/2       v(τ) = v0 + a·τ
/// w(τ) = (w0.x + wa.x·τ, w0.y + wa.y·τ, clamp_z(w0.z ∓ wz_decay·τ))
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Law {
    /// Position at the segment's start (mm).
    pub p0: V3,
    /// Velocity at the segment's start (mm/s).
    pub v0: V3,
    /// Constant acceleration over the segment (mm/s²).
    pub a: V3,
    /// Angular velocity at the segment's start (rad/s).
    pub w0: V3,
    /// Angular acceleration's horizontal part over the segment (rad/s²).
    pub wa: V3,
    /// Vertical spin decay over the segment (rad/s²), toward zero.
    pub wz_decay: f64,
    /// The mode in force over the segment.
    pub mode: MotionMode,
}

impl Law {
    /// The stationary law at `p0`: no motion, no spin.
    #[must_use]
    fn at_rest(p0: V3) -> Self {
        Self {
            p0,
            v0: V3::ZERO,
            a: V3::ZERO,
            w0: V3::ZERO,
            wa: V3::ZERO,
            wz_decay: 0.0,
            mode: MotionMode::Stationary,
        }
    }

    /// Position `tau` seconds into the segment.
    #[must_use]
    pub fn pos(&self, tau: f64) -> V3 {
        self.p0 + self.v0 * tau + self.a * (0.5 * tau * tau)
    }

    /// Velocity `tau` seconds into the segment.
    #[must_use]
    pub fn vel(&self, tau: f64) -> V3 {
        self.v0 + self.a * tau
    }

    /// Angular velocity `tau` seconds into the segment.
    #[must_use]
    pub fn spin(&self, tau: f64) -> V3 {
        let horizontal = self.w0 + self.wa * tau;
        let decay = self.wz_decay * tau;
        let z = if self.w0.z >= 0.0 {
            (self.w0.z - decay).max(0.0)
        } else {
            (self.w0.z + decay).min(0.0)
        };
        v3(horizontal.x, horizontal.y, z)
    }
}

/// One timeline entry: the time it starts and the per-ball laws in force from it (`physics.md` §1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    /// The segment's start time (s), measured from the strike.
    pub t: f64,
    /// The law of each ball, in the canonical order (cue, 1–15).
    pub laws: [Law; 16],
}

/// A ball that is at rest over a pocket mouth, held up by another ball: the rules layer counts it as
/// pocketed (`physics.md` §3.5.5, `rules.md` §10.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportedOverMouth {
    /// The ball whose centre lies inside the mouth.
    pub ball: u8,
    /// The balls it is in contact with — its support.
    pub supporting_balls: Vec<u8>,
}

/// The shot's final state (`architecture.md` §4.5).
#[derive(Clone, Debug, PartialEq)]
pub struct RestBlock {
    /// Every ball's final state, in the canonical order (cue, 1–15).
    pub states: [BallState; 16],
    /// The pocketed balls, ascending.
    pub pocketed: Vec<u8>,
    /// The off-table balls, ascending.
    pub off_table: Vec<u8>,
    /// Ball pairs in contact at rest (`FROZEN_GAP_MM`), ascending by ids.
    pub frozen_pairs: Vec<[u8; 2]>,
    /// Balls in contact with a rail at rest, ascending by ball id.
    pub frozen_rails: Vec<(u8, Rail)>,
    /// Balls at rest inside a mouth on another ball's support.
    pub supported_over_mouth: Vec<SupportedOverMouth>,
}

/// Why a shot stopped short of its own end. Both caps and the stall guard are of `physics.md` §1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Runaway {
    /// The event cap was hit.
    EventCap {
        /// The cap.
        cap: u64,
    },
    /// The time cap was hit.
    TimeCap {
        /// The cap, in seconds.
        cap_s: f64,
    },
    /// The no-progress counter tripped: consecutive groups advanced no time.
    Stall,
}

/// A shot's measured behaviour: the counters the acceptance hooks and the ladder read.
#[derive(Clone, Debug, PartialEq)]
pub struct ShotOutcome {
    /// The shot's duration (s), from the strike to rest.
    pub t_rest_s: f64,
    /// The number of applied events.
    pub events: u64,
    /// The number of simultaneity groups.
    pub groups: u64,
    /// The largest cushion penetration (mm) over the shot: 0 is no penetration at all.
    pub max_penetration_mm: f64,
    /// The smallest surface gap (mm) between two balls over the shot; `+∞` if no pair ever shared the
    /// table.
    pub min_pair_gap_mm: f64,
    /// The largest rise of a ball's centre above its resting height (mm).
    pub max_hop_mm: f64,
    /// The vertical-recovery coefficient this run used — every rail result reports it
    /// (`physics.md` §3.4/§6).
    pub e_slate: f64,
    /// The guard that ended the run, if one did.
    pub runaway: Option<Runaway>,
}

/// A computed shot: the fact stream, the segment timeline, and the rest block (`architecture.md` §4).
#[derive(Clone, Debug)]
pub struct Shot {
    timeline: Vec<Segment>,
    facts: Vec<Fact>,
    rest: RestBlock,
    outcome: ShotOutcome,
    pocketed_at_s: [f64; 16],
    off_table_at_s: [f64; 16],
}

impl Shot {
    /// Every ball's exact state at `t` (s), clamped to `[0, t_rest_s]`.
    ///
    /// Several groups can share one instant when a group's clearance re-checks at the same `t`; the
    /// state reported is the one after all of them, i.e. the last segment starting at or before `t`.
    #[must_use]
    pub fn state_at(&self, t: f64) -> [BallState; 16] {
        let t = t.clamp(0.0, self.outcome.t_rest_s);
        let index = self
            .timeline
            .partition_point(|segment| segment.t <= t)
            .saturating_sub(1);
        let segment = self.timeline[index];
        let tau = t - segment.t;
        std::array::from_fn(|i| {
            let law = segment.laws[i];
            BallState {
                pos_mm: law.pos(tau).into(),
                vel_mm_s: law.vel(tau).into(),
                spin_rad_s: law.spin(tau).into(),
                mode: law.mode,
                pocketed: t >= self.pocketed_at_s[i],
                off_table: t >= self.off_table_at_s[i],
            }
        })
    }

    /// The fact stream, in `seq` order (`physics.md` §7).
    #[must_use]
    pub fn events(&self) -> &[Fact] {
        &self.facts
    }

    /// The shot's final state and annotations.
    #[must_use]
    pub fn rest(&self) -> &RestBlock {
        &self.rest
    }

    /// The segment timeline: one entry per simultaneity group (`physics.md` §1).
    #[must_use]
    pub fn timeline(&self) -> &[Segment] {
        &self.timeline
    }

    /// The shot's measured behaviour.
    #[must_use]
    pub fn outcome(&self) -> &ShotOutcome {
        &self.outcome
    }

    /// The shot's duration (s).
    #[must_use]
    pub fn t_rest_s(&self) -> f64 {
        self.outcome.t_rest_s
    }
}

/// A rejected cue-ball placement (`architecture.md` §9's geometry predicates).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlacementError {
    /// The position is not finite.
    NotFinite,
    /// The cue ball's centre would not sit on the playing surface.
    OutsideSurface {
        /// The rejected position (mm).
        pos_mm: [f64; 2],
    },
    /// The cue ball would overlap another ball.
    Overlaps {
        /// The ball it would overlap.
        ball: u8,
        /// Their centre distance minus one diameter (mm): negative is an overlap.
        gap_mm: f64,
    },
}

/// One ball's evolving state inside the core. Ball ids are indices: 0 is the cue ball.
#[derive(Clone, Debug)]
struct Ball {
    p: V3,
    v: V3,
    w: V3,
    mode: MotionMode,
    law: Law,
    law_t: f64,
    pocketed: bool,
    off_table: bool,
    /// Is this ball part of the shot? [`Sim::new`] launches every ball of its arrangement; the
    /// analysis seam of [`Sim::cleared`] holds the rest out, so the fitting ladder's isolated shots
    /// (`physics.md` §6) are not polluted by fifteen racked balls.
    in_play: bool,
    /// The wall indices this ball was in contact with at shot start, with the `left_since_shot_start`
    /// flag of `rules.md` §10.2.
    frozen_walls: Vec<(usize, bool)>,
}

impl Ball {
    fn at_rest(p: V3) -> Self {
        Self {
            p,
            v: V3::ZERO,
            w: V3::ZERO,
            mode: MotionMode::Stationary,
            law: Law::at_rest(p),
            law_t: 0.0,
            pocketed: false,
            off_table: false,
            in_play: true,
            frozen_walls: Vec::new(),
        }
    }

    fn active(&self) -> bool {
        self.in_play && !self.pocketed && !self.off_table
    }
}

/// A candidate event (`physics.md` §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ev {
    Wall { ball: u8, wall: u8 },
    Tip { ball: u8, tip: u8 },
    Pair { i: u8, j: u8 },
    Land { ball: u8 },
    Drop { ball: u8, pocket: u8 },
    SlideToRoll { ball: u8 },
    RollToStop { ball: u8 },
    SpinDown { ball: u8 },
}

impl Ev {
    /// The canonical resolution order inside a simultaneity group: contact type first, then ball ids.
    fn rank(self) -> (u8, u8, u16) {
        match self {
            Self::Pair { i, j } => (0, i, u16::from(j)),
            Self::Wall { ball, wall } => (1, ball, u16::from(wall)),
            Self::Tip { ball, tip } => (2, ball, u16::from(tip)),
            Self::Drop { ball, pocket } => (3, ball, u16::from(pocket)),
            Self::Land { ball } => (4, ball, 0),
            Self::SlideToRoll { ball } => (5, ball, 0),
            Self::RollToStop { ball } => (6, ball, 0),
            Self::SpinDown { ball } => (7, ball, 0),
        }
    }
}

/// The face a surface impulse came from, for the fact it reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    Rail { rail: Rail, wall: usize },
    Jaw { pocket: PocketId },
}

/// The simulation facade (`architecture.md` §4): one `Sim` per shot, driven to rest by [`Sim::strike`].
#[derive(Clone, Debug)]
pub struct Sim {
    table: Table,
    balls: [Ball; 16],
    t: f64,
    group: u32,
    seq: u64,
    facts: Vec<Fact>,
    timeline: Vec<Segment>,
    pocketed_at_s: [f64; 16],
    off_table_at_s: [f64; 16],
    /// The event cap: a shot that exceeds it ends with [`Runaway::EventCap`].
    pub max_events: u64,
    /// The time cap (s): a shot that exceeds it ends with [`Runaway::TimeCap`].
    pub max_time_s: f64,
    /// The linear sleep threshold (mm/s, `physics.md` §5).
    pub sleep_linear_mm_s: f64,
    /// The angular sleep threshold (rad/s, `physics.md` §5).
    pub sleep_angular_rad_s: f64,
}

impl Sim {
    /// A table with its arrangement set: every ball at rest in its rack slot, the cue ball parked where
    /// [`Arrangement::rest_states`] puts it, until [`Sim::place_cue`] moves it.
    #[must_use]
    pub fn new(table: Table, arrangement: Arrangement) -> Self {
        let states = arrangement.rest_states();
        let balls: [Ball; 16] = std::array::from_fn(|i| {
            let [x, y, z] = states[i].pos_mm;
            debug_assert!(
                z == BALL_RADIUS_MM,
                "the rack's rest state sits on the cloth"
            );
            Ball::at_rest(v3(x, y, z))
        });
        Self {
            table,
            balls,
            t: 0.0,
            group: 0,
            seq: 0,
            facts: Vec::new(),
            timeline: Vec::new(),
            pocketed_at_s: [f64::INFINITY; 16],
            off_table_at_s: [f64::INFINITY; 16],
            max_events: DEFAULT_MAX_EVENTS,
            max_time_s: DEFAULT_MAX_TIME_S,
            sleep_linear_mm_s: SLEEP_LINEAR_MM_S,
            sleep_angular_rad_s: SLEEP_ANGULAR_RAD_S,
        }
    }

    /// A table cleared of every ball — the analysis seam of `physics.md` §6.
    ///
    /// The fitting ladder's stages isolate one parameter group each, which takes isolated shots: a
    /// single ball against one cushion, a pair in free space. The rack of [`Sim::new`] is the table's
    /// only legal start for a game (`rules-break.md` §2.5), so this path exists for the ladder and
    /// the differential stage, which place their own balls with [`Sim::place_ball`] and give one of
    /// them a pre-state with [`Sim::launch`]. The cleared balls keep their rack positions and are
    /// simply out of play: they raise no fact, and the rest block does not report them.
    #[must_use]
    pub fn cleared(table: Table) -> Self {
        let mut slots = [0_u8; 15];
        for (index, slot) in slots.iter_mut().enumerate() {
            *slot = index as u8 + 1;
        }
        let mut sim = Self::new(table, Arrangement { slots });
        for ball in &mut sim.balls {
            ball.in_play = false;
        }
        sim
    }

    /// Place the cue ball (ball-in-hand, `architecture.md` §9): its centre must sit on the playing
    /// surface, clear of every other ball by at least one diameter.
    ///
    /// # Errors
    ///
    /// Returns the [`PlacementError`] of [`Sim::place_ball`]: a non-finite position, a centre off the
    /// playing surface, or a placement overlapping a ball in play.
    pub fn place_cue(&mut self, pos_mm: [f64; 2]) -> Result<(), PlacementError> {
        self.place_ball(0, pos_mm)
    }

    /// Place a ball at rest by hand: `0` is the cue ball, `1..=15` an object ball. Same domain as
    /// [`Sim::place_cue`] — centre on the surface, one diameter clear of every other ball in play —
    /// and the ball joins the shot (the counterpart of [`Sim::cleared`] for the ladder's pre-states,
    /// `physics.md` §6/§8).
    ///
    /// # Errors
    ///
    /// Returns [`PlacementError::NotFinite`] for a non-finite position,
    /// [`PlacementError::OutsideSurface`] when the centre would leave the playing surface, and
    /// [`PlacementError::Overlaps`] when the ball would sit inside a ball in play.
    ///
    /// # Panics
    ///
    /// Panics if `ball` is not a ball id (`0..=15`).
    pub fn place_ball(&mut self, ball: u8, pos_mm: [f64; 2]) -> Result<(), PlacementError> {
        assert!(
            usize::from(ball) < self.balls.len(),
            "ball id out of range: {ball}"
        );
        let index = usize::from(ball);
        let [x, y] = pos_mm;
        if !x.is_finite() || !y.is_finite() {
            return Err(PlacementError::NotFinite);
        }
        if x.abs() > HALF_LEN_MM - BALL_RADIUS_MM || y.abs() > HALF_WIDTH_MM - BALL_RADIUS_MM {
            return Err(PlacementError::OutsideSurface { pos_mm });
        }
        let p = v3(x, y, BALL_RADIUS_MM);
        for (other_index, other) in self.balls.iter().enumerate() {
            if other_index == index || !other.active() {
                continue;
            }
            let gap = (p - other.p).len() - BALL_DIAMETER_MM;
            // Contact, not overlap: the same rounding guard the solver's own contact tests use. An
            // exact-contact position re-measures a few ulps short of a diameter on this route — the
            // spot search of `rules.md` §4 returns one, and 1.5's clause 2 *wants* contact — while a
            // real overlap is six orders of magnitude larger than this guard.
            if gap < -CONTACT_SLOP_MM {
                return Err(PlacementError::Overlaps {
                    ball: other_index as u8,
                    gap_mm: gap,
                });
            }
        }
        self.balls[index] = Ball::at_rest(p);
        Ok(())
    }

    /// Launch a ball with an explicit pre-state: the fitting ladder's free-space checks state the
    /// ball's `(v, ω)` rather than a stick's — `physics.md` §6's pure-spin branch is a ball with spin
    /// and no translation, which no declaration can express. [`Sim::strike`] is the declaration path
    /// and calls this for the cue ball.
    ///
    /// # Panics
    ///
    /// Panics if `ball` is not a ball id (`0..=15`), or the launch state is not finite.
    pub fn launch(&mut self, ball: u8, v_mm_s: V3, w_rad_s: V3) -> Shot {
        assert!(
            usize::from(ball) < self.balls.len(),
            "ball id out of range: {ball}"
        );
        assert!(
            v_mm_s.is_finite() && w_rad_s.is_finite(),
            "a launch state must be finite"
        );
        self.annotate_frozen();
        self.t = 0.0;
        self.group = 0;
        self.seq = 0;
        self.pocketed_at_s = [f64::INFINITY; 16];
        self.off_table_at_s = [f64::INFINITY; 16];
        self.facts.clear();
        self.timeline.clear();

        let launched = &mut self.balls[usize::from(ball)];
        launched.v = v_mm_s;
        launched.w = w_rad_s;
        launched.mode = motion_mode(launched.p, launched.v, launched.w);
        self.install_all();
        self.snapshot();

        let outcome = self.run_to_rest();
        self.assert_finite();
        let rest = self.measure_rest();
        Shot {
            timeline: std::mem::take(&mut self.timeline),
            facts: std::mem::take(&mut self.facts),
            rest,
            outcome,
            pocketed_at_s: self.pocketed_at_s,
            off_table_at_s: self.off_table_at_s,
        }
    }

    /// Compute the shot to rest and hand back its `Shot` (`architecture.md` §4).
    ///
    /// The sim keeps the final state, so a caller may place the cue again and strike the next shot;
    /// the facts and the timeline move into the returned `Shot`.
    ///
    /// # Errors
    ///
    /// Returns the [`StrikeError`] of [`StrikeDecl::resolve`] when the declaration is out of domain.
    pub fn strike(&mut self, decl: StrikeDecl) -> Result<Shot, StrikeError> {
        let resolved = decl.resolve(self.table.profile.pivot_mm)?;
        Ok(self.launch(0, resolved.v_mm_s, resolved.w_rad_s))
    }

    // ---------------------------------------------------------------- shot start

    /// Read the frozen annotations of the state at shot start (`rules.md` §10.2): the cushion faces a
    /// ball is in contact with, with the separation flag cleared.
    fn annotate_frozen(&mut self) {
        let walls = &self.table.walls;
        for ball in &mut self.balls {
            ball.frozen_walls.clear();
            if !ball.active() {
                continue;
            }
            for (index, wall) in walls.iter().enumerate() {
                if wall.rail.is_none() {
                    continue;
                }
                let [d, s] = face_coordinates(ball.p, wall);
                if s >= 0.0 && s <= wall.len && (d - BALL_RADIUS_MM).abs() <= FROZEN_GAP_MM {
                    ball.frozen_walls.push((index, false));
                }
            }
        }
    }

    /// Flag a frozen-rail ball as separated once it is clear of that cushion (`rules.md` §10.2).
    fn update_separation(&mut self) {
        let walls = &self.table.walls;
        for ball in &mut self.balls {
            if !ball.active() {
                continue;
            }
            for (index, separated) in &mut ball.frozen_walls {
                if *separated {
                    continue;
                }
                let wall = &walls[*index];
                let [d, _] = face_coordinates(ball.p, wall);
                if d - BALL_RADIUS_MM > FROZEN_GAP_MM {
                    *separated = true;
                }
            }
        }
    }

    // ---------------------------------------------------------------- the loop

    /// Advance the state from event to event until nothing is left to happen (`physics.md` §1).
    fn run_to_rest(&mut self) -> ShotOutcome {
        let mut events: u64 = 0;
        let mut groups: u64 = 0;
        let mut runaway = None;
        let mut stall: u32 = 0;

        loop {
            if events > self.max_events {
                runaway = Some(Runaway::EventCap {
                    cap: self.max_events,
                });
                break;
            }
            if self.t > self.max_time_s {
                runaway = Some(Runaway::TimeCap {
                    cap_s: self.max_time_s,
                });
                break;
            }
            let candidates = self.candidates();
            let mut best = f64::INFINITY;
            for (t, _) in &candidates {
                if *t < best {
                    best = *t;
                }
            }
            if !best.is_finite() {
                break; // nothing left to happen: the shot is at rest
            }
            let t_before = self.t;
            let t_next = if best < self.t { self.t } else { best };
            let mut group: Vec<Ev> = candidates
                .into_iter()
                .filter(|(t, _)| *t <= best + SIMULTANEITY_EPS_S)
                .map(|(_, event)| event)
                .collect();
            group.sort_by_key(|event| event.rank());

            self.t = t_next;
            self.group += 1;
            groups += 1;
            self.advance_to_now();
            self.update_separation();
            for event in group {
                events += 1;
                self.apply(event);
            }
            self.depenetrate_pairs();
            self.install_all();
            self.assert_finite();
            self.snapshot();
            self.snap_resting();

            if t_next <= t_before + 1e-15 {
                stall += 1;
                if stall > STALL_LIMIT {
                    runaway = Some(Runaway::Stall);
                    break;
                }
            } else {
                stall = 0;
            }
        }

        let measurements = self.measure();
        ShotOutcome {
            t_rest_s: self.t,
            events,
            groups,
            max_penetration_mm: measurements.max_penetration,
            min_pair_gap_mm: measurements.min_pair_gap,
            max_hop_mm: measurements.max_hop,
            e_slate: self.table.profile.e_slate,
            runaway,
        }
    }

    /// Move every active ball to the current time along its law.
    fn advance_to_now(&mut self) {
        let t = self.t;
        for ball in &mut self.balls {
            if !ball.active() {
                continue;
            }
            let tau = t - ball.law_t;
            let law = ball.law;
            ball.p = law.pos(tau);
            ball.v = law.vel(tau);
            ball.w = law.spin(tau);
        }
    }

    /// Reinstall every ball's law from its current state, valid from `self.t`.
    fn install_all(&mut self) {
        let t = self.t;
        let profile = &self.table.profile;
        for ball in &mut self.balls {
            ball.law = law_for(profile, ball.p, ball.v, ball.w, ball.mode);
            ball.law_t = t;
        }
    }

    /// Append the current laws to the timeline.
    fn snapshot(&mut self) {
        self.timeline.push(Segment {
            t: self.t,
            laws: std::array::from_fn(|i| self.balls[i].law),
        });
    }

    /// Snap balls that fell below both sleep thresholds to rest (`physics.md` §1/§5), so the solver
    /// does not chase micro-motion forever.
    ///
    /// A ball on the cloth counts whatever its mode says: a landing ball whose whole state is under
    /// the thresholds would otherwise re-land at the same instant for ever — the mode flag is not a
    /// licence to keep a stopped ball moving. A ball in flight is left to land.
    fn snap_resting(&mut self) {
        let (sleep_linear, sleep_angular) = (self.sleep_linear_mm_s, self.sleep_angular_rad_s);
        let mut snapped: Vec<u8> = Vec::new();
        for (i, ball) in self.balls.iter().enumerate() {
            if !ball.active()
                || ball.mode == MotionMode::Stationary
                || ball.p.z - BALL_RADIUS_MM > CONTACT_SLOP_MM
            {
                continue;
            }
            if ball.v.len() <= sleep_linear && ball.w.abs_max() <= sleep_angular {
                snapped.push(i as u8);
            }
        }
        for id in snapped {
            let ball = &mut self.balls[usize::from(id)];
            ball.v = V3::ZERO;
            ball.w = V3::ZERO;
            ball.mode = MotionMode::Stationary;
            ball.law = Law::at_rest(ball.p);
            ball.law_t = self.t;
            self.push_boundary_fact(FactKind::Rest { ball: id });
        }
    }

    /// Push one pair apart along `n`, to a gap of exactly `2R`, and report it (`physics.md` §1). A
    /// pair already within [`CONTACT_SLOP_MM`], or one still approaching, is left alone.
    fn separate_pair(&mut self, i: usize, j: usize, n: V3) {
        let (ball_i, ball_j) = (&self.balls[i], &self.balls[j]);
        if !ball_i.active() || !ball_j.active() {
            return;
        }
        let distance = (ball_j.p - ball_i.p).len();
        if BALL_DIAMETER_MM - distance <= CONTACT_SLOP_MM {
            return;
        }
        if (ball_i.v - ball_j.v).dot(n) > 0.0 {
            return; // still approaching: the impulse owns it
        }
        let push = (BALL_DIAMETER_MM - distance) * 0.5;
        self.balls[i].p = self.balls[i].p - n * push;
        self.balls[j].p = self.balls[j].p + n * push;
        self.push_event_fact(FactKind::Depenetration {
            a: i as u8,
            b: j as u8,
        });
    }

    /// The position-only separation step of `physics.md` §1: after a group clears, push
    /// already-separating overlapping pairs apart along the line of centres, to a gap of exactly `2R`.
    ///
    /// The rack's exact-contact lattice sits within rounding of `2R` and is left alone; the step owns
    /// overlaps past [`CONTACT_SLOP_MM`], never the lattice. Pairs are separated one at a time, so a
    /// cluster needs more than one sweep to clear: the step repeats until nothing moves (or the cap is
    /// reached, which a dense cluster's own symmetry can require).
    fn depenetrate_pairs(&mut self) {
        for _ in 0..DEPENETRATION_PASSES {
            let before = self.facts.len();
            for i in 0..self.balls.len() {
                for j in (i + 1)..self.balls.len() {
                    let n = (self.balls[j].p - self.balls[i].p).norm();
                    self.separate_pair(i, j, n);
                }
            }
            let moved = self.facts.len() > before;
            if !moved {
                break;
            }
        }
    }

    // ---------------------------------------------------------------- facts

    /// A fact emitted by applying an event of the current simultaneity group.
    fn push_event_fact(&mut self, kind: FactKind) {
        self.facts.push(Fact {
            seq: self.seq,
            t: self.t,
            group: Some(self.group),
            kind,
        });
        self.seq += 1;
    }

    /// A fact emitted at a group boundary, outside the group's event set.
    fn push_boundary_fact(&mut self, kind: FactKind) {
        self.facts.push(Fact {
            seq: self.seq,
            t: self.t,
            group: None,
            kind,
        });
        self.seq += 1;
    }

    fn assert_finite(&self) {
        for (i, ball) in self.balls.iter().enumerate() {
            assert!(
                ball.p.is_finite() && ball.v.is_finite() && ball.w.is_finite(),
                "ball {i} left the finite domain at t = {} (p {}, v {}, w {})",
                self.t,
                ball.p,
                ball.v,
                ball.w
            );
            assert!(
                ball.law.p0.is_finite()
                    && ball.law.v0.is_finite()
                    && ball.law.a.is_finite()
                    && ball.law.w0.is_finite()
                    && ball.law.wa.is_finite()
                    && ball.law.wz_decay.is_finite(),
                "ball {i}'s law is not finite at t = {}",
                self.t
            );
        }
    }

    // ---------------------------------------------------------------- measurements

    /// The run's extreme measurements (`physics.md` §6/§9.2).
    ///
    /// Penetration and overlap are read from the shot's **states** — the end of each instant, which is
    /// what `Shot::state_at` reports — never from the solver's steps *within* an instant. An instant
    /// that begins with a landing can step through an interpenetration while it separates the ball from
    /// what it landed on; that step is resolution, not a state the shot is ever in.
    ///
    /// The hop is a trajectory extreme rather than a state, so it is read from every segment's exact
    /// apex (`z` is a parabola in flight and affine otherwise).
    fn measure(&self) -> Measurements {
        let mut measurements = Measurements::default();
        for (index, segment) in self.timeline.iter().enumerate() {
            if self
                .timeline
                .get(index + 1)
                .is_some_and(|next| next.t == segment.t)
            {
                continue; // a solver step inside the instant, not the instant's state
            }
            let ball = |i: usize| (segment.laws[i].pos(0.0), self.out_of_play(i, segment.t));
            for i in 0..self.balls.len() {
                let (pi, gone_i) = ball(i);
                if !gone_i {
                    for wall in &self.table.walls {
                        let s = face_coordinates(pi, wall)[1];
                        let s = s.max(0.0).min(wall.len);
                        let along = wall.p + wall.t * s;
                        let penetration = BALL_RADIUS_MM - (pi - along).len();
                        if penetration > measurements.max_penetration {
                            measurements.max_penetration = penetration;
                        }
                    }
                }
                if let Some(apex) = segment_apex(segment, index, self, i)
                    && apex > measurements.max_hop
                {
                    measurements.max_hop = apex;
                }
                for j in (i + 1)..self.balls.len() {
                    let (pj, gone_j) = ball(j);
                    if gone_i || gone_j {
                        continue;
                    }
                    let gap = (pj - pi).len() - BALL_DIAMETER_MM;
                    if gap < measurements.min_pair_gap {
                        measurements.min_pair_gap = gap;
                    }
                }
            }
        }
        measurements
    }

    /// Whether a ball was out of play at `t`.
    fn out_of_play(&self, ball: usize, t: f64) -> bool {
        t >= self.pocketed_at_s[ball] || t >= self.off_table_at_s[ball]
    }

    /// The rest block of `architecture.md` §4.5: final states, the out-of-play sets, and the frozen
    /// and supported-over-mouth annotations.
    fn measure_rest(&self) -> RestBlock {
        let states: [BallState; 16] = std::array::from_fn(|i| self.ball_state(i));
        let mut pocketed = Vec::new();
        let mut off_table = Vec::new();
        let mut frozen_pairs = Vec::new();
        let mut frozen_rails: Vec<(u8, Rail)> = Vec::new();
        let mut supported_over_mouth = Vec::new();

        for (i, ball) in self.balls.iter().enumerate() {
            if ball.pocketed {
                pocketed.push(i as u8);
            }
            if ball.off_table {
                off_table.push(i as u8);
            }
            if !ball.active() {
                continue;
            }
            for wall in &self.table.walls {
                let Some(rail) = wall.rail else { continue };
                let [d, s] = face_coordinates(ball.p, wall);
                if s >= 0.0 && s <= wall.len && (d - BALL_RADIUS_MM).abs() <= FROZEN_GAP_MM {
                    frozen_rails.push((i as u8, rail));
                }
            }
            for pocket in &self.table.pockets {
                if pocket.shape.f(ball.p) <= 0.0 {
                    continue;
                }
                let supporting_balls: Vec<u8> = (0..self.balls.len())
                    .filter(|j| *j != i)
                    .filter(|j| {
                        let other = &self.balls[*j];
                        other.active()
                            && ((ball.p - other.p).len() - BALL_DIAMETER_MM).abs() <= FROZEN_GAP_MM
                    })
                    .map(|j| j as u8)
                    .collect();
                if !supporting_balls.is_empty() {
                    supported_over_mouth.push(SupportedOverMouth {
                        ball: i as u8,
                        supporting_balls,
                    });
                }
            }
        }

        for i in 0..self.balls.len() {
            for j in (i + 1)..self.balls.len() {
                if !self.balls[i].active() || !self.balls[j].active() {
                    continue;
                }
                let gap = (self.balls[j].p - self.balls[i].p).len() - BALL_DIAMETER_MM;
                if gap.abs() <= FROZEN_GAP_MM {
                    frozen_pairs.push([i as u8, j as u8]);
                }
            }
        }
        // A ball at a rail's segment split can register on both halves of the same rail.
        frozen_rails.sort_unstable();
        frozen_rails.dedup();

        RestBlock {
            states,
            pocketed,
            off_table,
            frozen_pairs,
            frozen_rails,
            supported_over_mouth,
        }
    }

    /// The current `BallState` of one ball, as the public state type.
    fn ball_state(&self, i: usize) -> BallState {
        let ball = &self.balls[i];
        BallState {
            pos_mm: ball.p.into(),
            vel_mm_s: ball.v.into(),
            spin_rad_s: ball.w.into(),
            mode: ball.mode,
            pocketed: ball.pocketed,
            off_table: ball.off_table,
        }
    }

    // ---------------------------------------------------------------- candidates

    /// A ball's next mode transition: the event that ends its current segment law, and the validity
    /// bound on every other candidate that law contributes.
    ///
    /// One law is one mode (`physics.md` §3.1): the sliding law's acceleration is in force until the
    /// contact point stops slipping, the rolling law's until the ball stops, a flight's until it
    /// lands. `None` means the law never ends on its own — a stationary ball's — so its validity is
    /// unbounded.
    fn law_transition(&self, ball: &Ball, id: u8, p: V3, v: V3, w: V3) -> Option<(f64, Ev)> {
        let profile = &self.table.profile;
        match ball.mode {
            MotionMode::Stationary => None,
            MotionMode::Spinning => {
                let dt = spin_down_time(w, profile.spin_decay_rad_s2, self.sleep_angular_rad_s);
                (dt.is_finite() && dt > 0.0).then_some((dt, Ev::SpinDown { ball: id }))
            }
            MotionMode::Rolling => {
                let speed = v.len();
                (speed > 0.0).then(|| {
                    (
                        speed / profile.roll_decel_mm_s2(),
                        Ev::RollToStop { ball: id },
                    )
                })
            }
            MotionMode::Sliding => {
                let slip = contact_point_velocity(v, w).len();
                (slip > 0.0).then(|| {
                    (
                        slip / (3.5 * profile.slide_decel_mm_s2()),
                        Ev::SlideToRoll { ball: id },
                    )
                })
            }
            MotionMode::Airborne => solve_z_landing(p.z, v.z).map(|dt| (dt, Ev::Land { ball: id })),
        }
    }

    /// Every candidate event time from the current state.
    fn candidates(&self) -> Vec<(f64, Ev)> {
        let mut out: Vec<(f64, Ev)> = Vec::with_capacity(64);
        // A segment law is a closed form for one mode only, and it ends at the ball's next mode
        // transition. That transition is a candidate in its own right; it is also the **validity
        // bound** on every other candidate the law contributes. A contact root beyond it is an
        // extrapolation of a law no longer in force — the phantom-contact class this solver must
        // never propose: the root is dropped here, and the transition (which fires first anyway)
        // reinstalls the law, so the candidate set is re-solved under the law actually in force.
        let mut validity = [f64::INFINITY; 16];
        for (i, ball) in self.balls.iter().enumerate() {
            if !ball.active() {
                continue;
            }
            let tau = self.t - ball.law_t;
            let p = ball.law.pos(tau);
            let v = ball.law.vel(tau);
            let w = ball.law.spin(tau);
            let id = i as u8;
            if let Some((dt, event)) = self.law_transition(ball, id, p, v, w) {
                out.push((self.t + dt, event));
                validity[i] = dt;
            }

            // Above the cushions a ball flies over the rails, the jaws and the pockets
            // (`physics.md` §3.4's ceiling rule).
            if ball.mode == MotionMode::Airborne && p.z > CUSHION_CLEARANCE_MM {
                continue;
            }
            for (index, wall) in self.table.walls.iter().enumerate() {
                if let Some(dt) = solve_wall(p, v, ball.law.a, wall).filter(|dt| *dt <= validity[i])
                {
                    out.push((
                        self.t + dt,
                        Ev::Wall {
                            ball: id,
                            wall: index as u8,
                        },
                    ));
                }
            }
            for (index, tip) in self.table.tips.iter().enumerate() {
                if let Some(dt) =
                    solve_point(p, v, ball.law.a, *tip).filter(|dt| *dt <= validity[i])
                {
                    out.push((
                        self.t + dt,
                        Ev::Tip {
                            ball: id,
                            tip: index as u8,
                        },
                    ));
                }
            }
            for (index, pocket) in self.table.pockets.iter().enumerate() {
                if let Some(dt) =
                    solve_drop(p, v, ball.law.a, pocket).filter(|dt| *dt <= validity[i])
                {
                    out.push((
                        self.t + dt,
                        Ev::Drop {
                            ball: id,
                            pocket: index as u8,
                        },
                    ));
                }
            }
        }

        self.pair_candidates(&validity, &mut out);
        out
    }

    /// The pair-contact candidates: every pair that is approaching, or overlapping past the contact
    /// slop, with a solved root inside both balls' segment validity — the pair's relative law is a
    /// difference of the two, so it holds no longer than the shorter one.
    fn pair_candidates(&self, validity: &[f64; 16], out: &mut Vec<(f64, Ev)>) {
        for i in 0..self.balls.len() {
            for j in (i + 1)..self.balls.len() {
                if !self.balls[i].active() || !self.balls[j].active() {
                    continue;
                }
                let (a, b) = (&self.balls[i], &self.balls[j]);
                let (ta, tb) = (self.t - a.law_t, self.t - b.law_t);
                let pi = a.law.pos(ta);
                let pj = b.law.pos(tb);
                let vi = a.law.vel(ta);
                let vj = b.law.vel(tb);
                // Only an approaching pair has an event: a pair touching through the lattice must not
                // re-trigger at 0, and an already-separating overlap is the separation step's business,
                // not the impulse's.
                let (_, v_n) = pair_contact(pi, pj, vi, vj);
                let approaching = is_approaching(v_n);
                let dp = pi - pj;
                let gap2 = dp.dot(dp) - BALL_DIAMETER_MM * BALL_DIAMETER_MM;
                let slop_area = 2.0 * BALL_DIAMETER_MM * CONTACT_SLOP_MM;
                // Only an approaching pair — or one overlapping past the slop, which must be resolved
                // one way or the other — has an event: a pair touching through the rack's
                // exact-contact lattice must not re-trigger at zero.
                //
                // This branch is the **contact-now** case, and it is the only one: a pair within the
                // slop of contact *is* touching, so an event at the current instant is a contact, not
                // a fabrication. The clamp the root finders no longer carry read a runaway step as
                // "contact now" without ever asking whether the pair were anywhere near each other.
                let dt = if gap2 <= slop_area {
                    if approaching || gap2 < -slop_area {
                        0.0
                    } else {
                        continue;
                    }
                } else if approaching {
                    // A solved root is a candidate only inside **both** balls' segment validity (the
                    // pair's relative law is a difference of the two, so it holds no longer than the
                    // shorter one), and only if the pair is really in contact and approaching there.
                    // The phantom this rejects: a root at ≈ 1.15 s on a pair whose sliding law ends
                    // at 0.71 s, refined out of its basin onto a metre-apart pair (`solve_pair`).
                    let valid = validity[i].min(validity[j]);
                    match solve_pair(dp, vi - vj, a.law.a - b.law.a) {
                        Some(dt)
                            if dt <= valid
                                && pair_contact_at(
                                    a.law.pos(ta + dt),
                                    b.law.pos(tb + dt),
                                    a.law.vel(ta + dt),
                                    b.law.vel(tb + dt),
                                ) =>
                        {
                            dt
                        }
                        _ => continue,
                    }
                } else {
                    continue;
                };
                out.push((
                    self.t + dt,
                    Ev::Pair {
                        i: i as u8,
                        j: j as u8,
                    },
                ));
            }
        }
    }

    // ---------------------------------------------------------------- event application

    fn apply(&mut self, event: Ev) {
        match event {
            Ev::Wall { ball, wall } => {
                let face = self.table.walls[usize::from(wall)].clone();
                self.impulse_surface(
                    usize::from(ball),
                    surface_normal(face.n),
                    face_kind(&face, wall),
                );
            }
            Ev::Tip { ball, tip } => {
                let tip = self.table.tips[usize::from(tip)];
                let p = self.balls[usize::from(ball)].p;
                let n = (p - tip).norm();
                if n == V3::ZERO || !within_point_slop(p - tip) {
                    // Coincident or not at the tip: a jaw's impulse has no contact to act at. The
                    // ticket's class — an impulse applied to a surface that is nowhere near the ball
                    // — must not be reachable through the event stream.
                    return;
                }
                let pocket = self.nearest_pocket(tip);
                self.impulse_surface(usize::from(ball), n, Face::Jaw { pocket });
            }
            Ev::Pair { i, j } => self.hit_pair(i as usize, j as usize),
            Ev::Land { ball } => self.land(usize::from(ball)),
            Ev::Drop { ball, pocket } => {
                self.drop(usize::from(ball), PocketId::ALL[usize::from(pocket)]);
            }
            Ev::SlideToRoll { ball } => self.slide_to_roll(usize::from(ball)),
            Ev::RollToStop { ball } => self.roll_to_stop(usize::from(ball)),
            Ev::SpinDown { ball } => self.spin_down(usize::from(ball)),
        }
    }

    /// Sliding → rolling (`physics.md` §3.1): the contact point stops slipping.
    fn slide_to_roll(&mut self, i: usize) {
        let (v, wz, id) = {
            let ball = &self.balls[i];
            (ball.v, ball.w.z, i as u8)
        };
        let speed = v.len();
        let (sleep_linear, sleep_angular) = (self.sleep_linear_mm_s, self.sleep_angular_rad_s);
        let ball = &mut self.balls[i];
        if speed <= sleep_linear {
            ball.v = V3::ZERO;
            ball.w.x = 0.0;
            ball.w.y = 0.0;
            if wz.abs() > sleep_angular {
                ball.mode = MotionMode::Spinning;
            } else {
                ball.w = V3::ZERO;
                ball.mode = MotionMode::Stationary;
            }
            self.push_event_fact(FactKind::RollToStop { ball: id });
            return;
        }
        ball.w.x = -v.y / BALL_RADIUS_MM;
        ball.w.y = v.x / BALL_RADIUS_MM;
        ball.mode = MotionMode::Rolling;
        self.push_event_fact(FactKind::SlideToRoll { ball: id });
    }

    /// Rolling → spinning or stationary.
    fn roll_to_stop(&mut self, i: usize) {
        let wz = self.balls[i].w.z;
        let sleep_angular = self.sleep_angular_rad_s;
        let ball = &mut self.balls[i];
        ball.v = V3::ZERO;
        ball.w.x = 0.0;
        ball.w.y = 0.0;
        if wz.abs() > sleep_angular {
            ball.mode = MotionMode::Spinning;
        } else {
            ball.w = V3::ZERO;
            ball.mode = MotionMode::Stationary;
        }
        self.push_event_fact(FactKind::RollToStop { ball: i as u8 });
    }

    /// Spinning → stationary.
    fn spin_down(&mut self, i: usize) {
        self.balls[i].w = V3::ZERO;
        self.balls[i].mode = MotionMode::Stationary;
        self.push_event_fact(FactKind::SpinDown { ball: i as u8 });
    }

    /// Landing (`physics.md` §3.7): the cloth returns the vertical velocity with `e_slate`, and a ball
    /// that has left the playing surface is done.
    fn land(&mut self, i: usize) {
        let e_slate = self.table.profile.e_slate;
        let mut kicked = false;
        {
            let ball = &mut self.balls[i];
            if ball.v.z < 0.0 {
                ball.v.z *= -e_slate;
            }
            ball.p.z = BALL_RADIUS_MM;
            if ball.v.z > 1e-6 {
                kicked = true;
            } else {
                ball.v.z = 0.0;
            }
            ball.mode = motion_mode(ball.p, ball.v, ball.w);
        }
        if kicked {
            self.push_event_fact(FactKind::Kick {
                ball: i as u8,
                cause: KickCause::Landing,
            });
        }
        let p = self.balls[i].p;
        if p.x.abs() > HALF_LEN_MM + 1e-6 || p.y.abs() > HALF_WIDTH_MM + 1e-6 {
            let ball = &mut self.balls[i];
            ball.off_table = true;
            ball.v = V3::ZERO;
            ball.w = V3::ZERO;
            ball.mode = MotionMode::Stationary;
            self.off_table_at_s[i] = self.t;
            self.push_event_fact(FactKind::OffTable { ball: i as u8 });
        }
    }

    /// The drop predicate fired: the ball's centre crossed into the mouth (`physics.md` §3.5.2).
    fn drop(&mut self, i: usize, pocket: PocketId) {
        {
            let ball = &mut self.balls[i];
            ball.pocketed = true;
            ball.v = V3::ZERO;
            ball.w = V3::ZERO;
            ball.mode = MotionMode::Stationary;
        }
        self.pocketed_at_s[i] = self.t;
        self.push_event_fact(FactKind::Pocketed {
            ball: i as u8,
            pocket,
        });
    }

    fn nearest_pocket(&self, p: V3) -> PocketId {
        let mut best = PocketId::ALL[0];
        let mut best_distance = f64::INFINITY;
        for pocket in &self.table.pockets {
            let distance = (p - pocket.mouth_center).len();
            if distance < best_distance {
                best_distance = distance;
                best = pocket.id;
            }
        }
        best
    }

    /// A frictional-inelastic surface impulse with the nose-height contact normal (`physics.md` §3.3).
    /// `n` is the inward unit normal (tilted down for a cushion), and the contact point is at `−R·n`.
    fn impulse_surface(&mut self, i: usize, n: V3, face: Face) {
        let profile = &self.table.profile;
        let (e_n, mu_c, e_slate) = (profile.e_n, profile.mu_c, profile.e_slate);
        let near_cloth = self.balls[i].p.z - BALL_RADIUS_MM <= 1.0;
        let mut popped = false;
        {
            let ball = &mut self.balls[i];
            let v_n = ball.v.dot(n);
            if v_n >= 0.0 {
                return; // separating: the group's re-check rejected this contact
            }
            let rc = n * (-BALL_RADIUS_MM);
            let jn = -(1.0 + e_n) * v_n * BALL_MASS_G;
            // The tangential channel is 3D (`physics.md` §3.3): the contact-point velocity is used in
            // full, including the roll-induced vertical component, so the impulse does vertical work.
            let u = ball.v + ball.w.cross(rc);
            let u_t = u - n * u.dot(n);
            let utl = u_t.len();
            let jt = if utl > 1e-9 {
                let stick = utl * BALL_MASS_G / 7.0;
                let cap = mu_c * jn;
                let magnitude = if stick < cap { stick } else { cap };
                u_t.norm() * (-magnitude)
            } else {
                V3::ZERO
            };
            let j = n * jn + jt;
            ball.v = ball.v + j / BALL_MASS_G;
            ball.w = ball.w + rc.cross(j) / BALL_INERTIA_G_MM2;
            if near_cloth && ball.v.z < 0.0 {
                // The tilted normal drives the ball into the cloth; the cloth returns it with the
                // vertical recovery `e_slate` (`physics.md` §3.4).
                ball.v.z *= -e_slate;
                if ball.v.z > 1e-6 {
                    popped = true;
                } else {
                    ball.v.z = 0.0;
                }
            }
            ball.mode = motion_mode(ball.p, ball.v, ball.w);
            if ball.mode == MotionMode::Stationary {
                ball.v = V3::ZERO;
                ball.w = V3::ZERO;
            }
        }
        match face {
            Face::Rail { rail, wall } => {
                let frozen = self.balls[i]
                    .frozen_walls
                    .iter()
                    .find(|(index, _)| *index == wall);
                self.push_event_fact(FactKind::RailContact {
                    ball: i as u8,
                    rail,
                    frozen_at_shot_start: frozen.is_some(),
                    left_since_shot_start: frozen.is_some_and(|(_, separated)| *separated),
                });
            }
            Face::Jaw { pocket } => {
                self.push_event_fact(FactKind::JawContact {
                    ball: i as u8,
                    pocket,
                });
            }
        }
        if popped {
            self.push_event_fact(FactKind::Kick {
                ball: i as u8,
                cause: KickCause::CushionPop,
            });
        }
    }

    /// A ball–ball impulse (`physics.md` §3.2): frictional inelastic, with the speed-dependent `μb`.
    fn hit_pair(&mut self, i: usize, j: usize) {
        let (pi, pj) = (self.balls[i].p, self.balls[j].p);
        if !within_contact_slop(pj - pi) {
            // Not in contact: two balls further apart than the contact slop have no contact to
            // impulse, and the fact is not emitted. This is the guard of last resort for the
            // phantom-contact class — a solved root extrapolated past its law, or a group re-check
            // against a state that has moved on — and it is the same contact test the candidate
            // filter ran, so the only way a real event can meet it is the group window's own travel
            // (≤ `SIMULTANEITY_EPS_S`, which the pair's own root is re-solved against at the next
            // group). A non-contact is never turned into one.
            return;
        }
        let (n, v_n) = pair_contact(pi, pj, self.balls[i].v, self.balls[j].v);
        if !is_approaching(v_n) {
            // Touching or already separating: the pair brings no impulse, but an overlap still has to
            // be resolved — the position-only separation step's job, here and after the group.
            self.separate_pair(i, j, n);
            return;
        }
        let e_b = self.table.profile.e_b;
        let jn = (1.0 + e_b) * v_n * BALL_MASS_G / 2.0;
        let ri = n * BALL_RADIUS_MM;
        let rj = n * (-BALL_RADIUS_MM);
        let ui = self.balls[i].v + self.balls[i].w.cross(ri);
        let uj = self.balls[j].v + self.balls[j].w.cross(rj);
        let u = ui - uj; // i's contact point relative to j's
        let u_t = u - n * u.dot(n);
        let utl = u_t.len();
        let jt = if utl > 1e-9 {
            let stick = utl * BALL_MASS_G / 7.0;
            let mu = self.table.profile.mu_b.eval(utl);
            let cap = mu * jn;
            let magnitude = if stick < cap { stick } else { cap };
            u_t.norm() * (-magnitude) // the friction impulse on i
        } else {
            V3::ZERO
        };
        let imp_i = n * (-jn) + jt;
        let imp_j = -imp_i;
        {
            let ball = &mut self.balls[i];
            ball.v = ball.v + imp_i / BALL_MASS_G;
            ball.w = ball.w + ri.cross(imp_i) / BALL_INERTIA_G_MM2;
            ball.mode = motion_mode(ball.p, ball.v, ball.w);
        }
        {
            let ball = &mut self.balls[j];
            ball.v = ball.v + imp_j / BALL_MASS_G;
            ball.w = ball.w + rj.cross(imp_j) / BALL_INERTIA_G_MM2;
            ball.mode = motion_mode(ball.p, ball.v, ball.w);
        }
        self.push_event_fact(FactKind::BallBall {
            a: i as u8,
            b: j as u8,
        });
    }
}

/// The highest a ball's centre rises above its resting height over one segment (mm), or `None` when it
/// never rises. The `z` law is a parabola in flight and affine otherwise, so the apex is closed-form.
fn segment_apex(segment: &Segment, index: usize, sim: &Sim, ball: usize) -> Option<f64> {
    let law = segment.laws[ball];
    if sim.out_of_play(ball, segment.t) {
        return None;
    }
    let end = sim.timeline.get(index + 1).map_or(sim.t, |next| next.t);
    let span = end - segment.t;
    let apex_tau = if law.a.z < 0.0 {
        (law.v0.z / -law.a.z).clamp(0.0, span)
    } else if law.a.z > 0.0 {
        if law.v0.z >= 0.0 { span } else { 0.0 }
    } else {
        if law.v0.z > 0.0 { span } else { 0.0 }
    };
    let rise = law.pos(apex_tau).z - BALL_RADIUS_MM;
    (rise > 0.0).then_some(rise)
}

/// The run's extreme measurements, accumulated group by group.
#[derive(Clone, Copy, Debug)]
struct Measurements {
    /// The largest cushion penetration (mm).
    max_penetration: f64,
    /// The smallest ball–ball surface gap (mm).
    min_pair_gap: f64,
    /// The largest rise of a ball's centre above its resting height (mm).
    max_hop: f64,
}

impl Default for Measurements {
    fn default() -> Self {
        Self {
            max_penetration: 0.0,
            min_pair_gap: f64::INFINITY,
            max_hop: 0.0,
        }
    }
}

/// The inward unit normal of a cushion contact: the horizontal face normal, scaled by the nose
/// height's horizontal share, tilted down by its vertical share (`physics.md` §3.3).
fn surface_normal(face_normal: V3) -> V3 {
    let horizontal = cushion_normal_horizontal();
    v3(
        face_normal.x * horizontal,
        face_normal.y * horizontal,
        -CUSHION_NORMAL_Z,
    )
}

/// Which face a wall event belongs to, for the fact it reports.
fn face_kind(wall: &Wall, index: u8) -> Face {
    match (wall.rail, wall.pocket) {
        (Some(rail), _) => Face::Rail {
            rail,
            wall: usize::from(index),
        },
        (None, Some(pocket)) => Face::Jaw { pocket },
        (None, None) => unreachable!("every face is a rail or a jaw"),
    }
}

/// The signed distance to a face's line and the coordinate along it.
fn face_coordinates(p: V3, wall: &Wall) -> [f64; 2] {
    let offset = p - wall.p;
    [offset.dot(wall.n), offset.dot(wall.t)]
}

/// The contact-point velocity at the cloth contact, `(0, 0, −R)` from the centre (`physics.md` §3.1).
fn contact_point_velocity(v: V3, w: V3) -> V3 {
    v3(v.x - BALL_RADIUS_MM * w.y, v.y + BALL_RADIUS_MM * w.x, 0.0)
}

/// Whether a ball is airborne: off the cloth, or rising (`physics.md` §3.7).
fn is_airborne(p: V3, v: V3) -> bool {
    p.z - BALL_RADIUS_MM > 1e-9 || v.z.abs() > 1e-7
}

/// The canonical mode from a state, used after every impulse (`physics.md` §3.1).
fn motion_mode(p: V3, v: V3, w: V3) -> MotionMode {
    if is_airborne(p, v) {
        return MotionMode::Airborne;
    }
    if contact_point_velocity(v, w).len() > 1e-6 {
        MotionMode::Sliding
    } else if v.len() > SLEEP_LINEAR_MM_S {
        MotionMode::Rolling
    } else if w.abs_max() > SLEEP_ANGULAR_RAD_S {
        MotionMode::Spinning
    } else {
        MotionMode::Stationary
    }
}

/// One ball's analytic law for a mode (`physics.md` §3.1/§3.7).
fn law_for(profile: &crate::profile::Profile, p: V3, v: V3, w: V3, mode: MotionMode) -> Law {
    match mode {
        MotionMode::Stationary => Law::at_rest(p),
        MotionMode::Spinning => Law {
            p0: p,
            v0: V3::ZERO,
            a: V3::ZERO,
            w0: v3(0.0, 0.0, w.z),
            wa: V3::ZERO,
            wz_decay: profile.spin_decay_rad_s2,
            mode,
        },
        MotionMode::Rolling => {
            let acc = v.norm() * (-profile.roll_decel_mm_s2());
            Law {
                p0: p,
                v0: v,
                a: acc,
                w0: v3(-v.y / BALL_RADIUS_MM, v.x / BALL_RADIUS_MM, w.z),
                wa: v3(-acc.y / BALL_RADIUS_MM, acc.x / BALL_RADIUS_MM, 0.0),
                wz_decay: profile.spin_decay_rad_s2,
                mode,
            }
        }
        MotionMode::Sliding => {
            let slip = contact_point_velocity(v, w);
            let acc = slip.norm() * (-profile.slide_decel_mm_s2());
            let wa = v3(acc.y, -acc.x, 0.0) * (5.0 / (2.0 * BALL_RADIUS_MM));
            Law {
                p0: p,
                v0: v,
                a: acc,
                w0: w,
                wa,
                wz_decay: profile.spin_decay_rad_s2,
                mode,
            }
        }
        MotionMode::Airborne => Law {
            p0: p,
            v0: v,
            a: v3(0.0, 0.0, -GRAVITY_MM_S2),
            w0: w,
            wa: V3::ZERO,
            wz_decay: 0.0,
            mode,
        },
    }
}

// ------------------------------------------------------------------ closed-form solvers

/// The smallest strictly positive root of `a·t² + b·t + c`.
fn smallest_positive_root(a: f64, b: f64, c: f64) -> Option<f64> {
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return None;
        }
        let t = -c / b;
        return if t > 0.0 { Some(t) } else { None };
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t1 = (-b - s) / (2.0 * a);
    let t2 = (-b + s) / (2.0 * a);
    let (lo, hi) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
    if lo > 0.0 {
        Some(lo)
    } else if hi > 0.0 {
        Some(hi)
    } else {
        None
    }
}

/// When a ball reaches a face: requires approach, so a ball sliding along a cushion at exactly `R`
/// does not re-trigger at `t = 0` (`physics.md` §1's stall guard).
fn solve_wall(p: V3, v: V3, acc: V3, wall: &Wall) -> Option<f64> {
    let d0 = (p - wall.p).dot(wall.n) - BALL_RADIUS_MM;
    let d1 = v.dot(wall.n);
    if d1 >= 0.0 {
        return None;
    }
    let t = if d0.abs() <= CONTACT_SLOP_MM {
        // At the face within rounding, and approaching: the contact is now. The window is
        // two-sided, and it has to be: a jaw face's plane runs across the table's interior, so a
        // ball *behind* such a plane by hundreds of mm is nowhere near the face. The one-sided
        // reading (`d0 <= slop`) fired a spurious jaw contact on any ball that approached a jaw's
        // plane from behind — a rebound off a long rail beside a side mouth, say — and applied the
        // jaw's impulse to it.
        0.0
    } else {
        let d2 = 0.5 * acc.dot(wall.n);
        let root = smallest_positive_root(d2, d1, d0)?;
        if root > 1e6 {
            return None;
        }
        root
    };
    let at_contact = p + v * t + acc * (0.5 * t * t);
    let s = (at_contact - wall.p).dot(wall.t);
    if s < -1e-9 || s > wall.len + 1e-9 {
        return None;
    }
    Some(t)
}

/// When a ball's centre reaches `R` of a jaw tip.
fn solve_point(p: V3, v: V3, acc: V3, tip: V3) -> Option<f64> {
    let dp = p - tip;
    if dp.dot(v) >= 0.0 {
        return None; // not approaching the tip
    }
    if within_point_slop(dp) {
        return Some(0.0); // at the tip within rounding, and approaching
    }
    let mut t = smallest_positive_root(
        v.dot(v),
        2.0 * dp.dot(v),
        dp.dot(dp) - BALL_RADIUS_MM * BALL_RADIUS_MM,
    )?;
    for _ in 0..ROOT_ITERATIONS {
        let pos = dp + v * t + acc * (0.5 * t * t);
        let vel = v + acc * t;
        let f = pos.dot(pos) - BALL_RADIUS_MM * BALL_RADIUS_MM;
        let fp = 2.0 * pos.dot(vel);
        if fp.abs() < 1e-12 {
            break;
        }
        t -= f / fp;
        if !t.is_finite() || t <= 0.0 {
            // The step crossed the origin: the iterate has left the positive root's basin. A tip
            // contact at `t = 0` is the caller's within-slop case (`within_point_slop`), not a root,
            // and a step at or below zero is never to be clamped into one — the clamp this replaces
            // is exactly what turned a runaway iterate into a contact that was not there.
            return None;
        }
    }
    let pos = dp + v * t + acc * (0.5 * t * t);
    let vel = v + acc * t;
    // A root is only a candidate if it is a real contact: at the root the tip's surface is reached
    // within the contact slop, and the ball is still approaching it (the refinement can otherwise
    // land on the exit root — the far side of the pass-through).
    if !within_point_slop(pos) || pos.dot(vel) >= 0.0 {
        return None;
    }
    Some(t)
}

/// A pair's contact normal (`i` → `j`) and their relative speed along it: positive is approaching.
///
/// The candidate filter and the impulse both read this one function, so they can never disagree on
/// the sign — a disagreement is an event that is proposed and then rejected, which is a group that
/// makes no progress.
fn pair_contact(pi: V3, pj: V3, vi: V3, vj: V3) -> (V3, f64) {
    let n = (pj - pi).norm();
    if n == V3::ZERO {
        // Coincident centres: a degenerate state, separated along the long axis so that the pair still
        // resolves deterministically instead of standing still for ever.
        return (v3(1.0, 0.0, 0.0), (vi - vj).x);
    }
    (n, (vi - vj).dot(n))
}

/// Whether a pair's relative normal speed is a real approach, past the noise floor of a tangential
/// contact.
fn is_approaching(v_n: f64) -> bool {
    v_n > APPROACH_FLOOR_MM_S
}

/// Whether two balls' surfaces are within the contact slop — the one test behind the candidate
/// filter's "contact now" case, its verification of a solved root, and the impulse's own guard.
///
/// `dp` is the centre-to-centre vector. The test is one-sided (an overlap counts), so a pair pushed
/// inside the slop by a previous impulse still reads as in contact.
fn within_contact_slop(dp: V3) -> bool {
    dp.dot(dp) - BALL_DIAMETER_MM * BALL_DIAMETER_MM <= 2.0 * BALL_DIAMETER_MM * CONTACT_SLOP_MM
}

/// Whether a ball's surface is within the contact slop of a point collider (a jaw tip), for
/// `dp = p − tip`. The one test behind [`solve_point`]'s contact-now case, its root verification, and
/// the tip impulse's own guard.
fn within_point_slop(dp: V3) -> bool {
    dp.dot(dp) - BALL_RADIUS_MM * BALL_RADIUS_MM <= 2.0 * BALL_RADIUS_MM * CONTACT_SLOP_MM
}

/// Whether a pair whose root has been solved is **really** in contact and approaching there. A root
/// that fails either test is not a contact at the instant it names — it is the leftover of a law
/// extrapolated past its validity, or of a refinement that never found a root because the pair's
/// closest approach stays outside the slop — so it is not an event, and a non-contact is never
/// turned into one.
fn pair_contact_at(pi: V3, pj: V3, vi: V3, vj: V3) -> bool {
    if !within_contact_slop(pj - pi) {
        return false;
    }
    let (_, v_n) = pair_contact(pi, pj, vi, vj);
    is_approaching(v_n)
}

/// When two **separated** balls' centres reach `2R` of each other. The caller has established that
/// the pair is approaching and not in contact (the contact-now case never reaches here): letting the
/// constant-velocity quadratic answer for a pair that is already touching would return its *second*
/// root — the far side of the pass-through — and the pair would interpenetrate for the interval
/// between, which is how a rack contact tunnels.
///
/// The root is only a candidate **inside both balls' segment validity**, which the caller bounds (a
/// difference of two laws holds no longer than the shorter of them) and verifies with
/// [`pair_contact_at`].
fn solve_pair(dp: V3, dv: V3, da: V3) -> Option<f64> {
    let diameter2 = BALL_DIAMETER_MM * BALL_DIAMETER_MM;
    let mut t = smallest_positive_root(dv.dot(dv), 2.0 * dp.dot(dv), dp.dot(dp) - diameter2)?;
    for _ in 0..ROOT_ITERATIONS {
        let pos = dp + dv * t + da * (0.5 * t * t);
        let vel = dv + da * t;
        let f = pos.dot(pos) - diameter2;
        let fp = 2.0 * pos.dot(vel);
        if fp.abs() < 1e-12 {
            break;
        }
        t -= f / fp;
        if !t.is_finite() || t <= 0.0 {
            // The step crossed the origin: the iterate has left the positive root's basin — which
            // is what a root extrapolated past its segment's validity does. The caller owns the
            // contact-now case (a pair within the slop and approaching), so there is nothing here
            // to report: rejected, never clamped. The clamp this replaces is precisely how a pair
            // 1634 mm apart was turned into a contact at `t ≈ 0`.
            return None;
        }
    }
    Some(t)
}

/// When an airborne ball reaches the cloth.
fn solve_z_landing(z: f64, vz: f64) -> Option<f64> {
    let d0 = z - BALL_RADIUS_MM;
    if d0 <= 0.0 && vz <= 0.0 {
        return Some(0.0); // at or below the cloth and not rising: land now
    }
    smallest_positive_root(-0.5 * GRAVITY_MM_S2, vz, d0)
}

/// The drop predicate of `physics.md` §3.5.2: root-found on the current analytic segment, with all
/// three conditions — (a) the crossing is inward, (b) the boundary is actually reached within the
/// tolerance, and (c) the crossing is inside the mouth's jaw-tip window.
fn solve_drop(p: V3, v: V3, acc: V3, pocket: &Pocket) -> Option<f64> {
    if pocket.shape.f(p) >= 0.0 {
        // Already inside the mouth: a drop fires on a crossing, never on a state. This is what keeps
        // a ball at rest in the rack — and any ball sitting in a mouth — from being pocketed twice.
        return None;
    }
    let mut t = match &pocket.shape {
        DropShape::Chord { mid, outward, .. } => {
            let closing = v.dot(*outward);
            if closing <= 0.0 {
                return None; // not heading into the mouth
            }
            -(p - *mid).dot(*outward) / closing
        }
        DropShape::Disc { centre, radius } => {
            let dp = p - *centre;
            smallest_positive_root(v.dot(v), 2.0 * dp.dot(v), dp.dot(dp) - radius * radius)?
        }
    };
    for _ in 0..ROOT_ITERATIONS {
        let pos = p + v * t + acc * (0.5 * t * t);
        let vel = v + acc * t;
        let f = pocket.shape.f(pos);
        let fp = pocket.shape.grad(pos).dot(vel);
        if fp.abs() < 1e-12 {
            break;
        }
        let step = f / fp;
        if !step.is_finite() {
            return None;
        }
        t -= step;
        if !t.is_finite() || t <= 0.0 {
            // The step crossed the origin: the iterate has left the crossing's basin. A drop is a
            // crossing and the ball is still outside the mouth here, so the crossing's own root is
            // re-solved from the (unchanged) state on the next pass — rejected, never clamped into
            // a `t ≈ 0` drop.
            return None;
        }
    }
    if t > 1e4 {
        return None;
    }
    let pos = p + v * t + acc * (0.5 * t * t);
    let vel = v + acc * t;
    if pocket.shape.f(pos).abs() > DROP_BOUNDARY_TOLERANCE_MM {
        return None; // (b) the boundary was not reached
    }
    if pocket.shape.grad(pos).dot(vel) <= 0.0 {
        return None; // (a) a separating solution
    }
    if !pocket.shape.lateral_ok(pos) {
        return None; // (c) outside the jaw-tip window
    }
    Some(t)
}

/// When a spinning ball's vertical spin decays below the sleep threshold.
fn spin_down_time(w: V3, spin_decay_rad_s2: f64, sleep_angular_rad_s: f64) -> f64 {
    if spin_decay_rad_s2 <= 0.0 {
        return f64::INFINITY;
    }
    let remaining = w.z.abs() - sleep_angular_rad_s;
    if remaining <= 0.0 {
        0.0
    } else {
        remaining / spin_decay_rad_s2
    }
}
