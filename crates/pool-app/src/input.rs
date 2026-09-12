//! The cue input machine (`architecture.md` §10's `input.rs`): the ruled gesture chain of
//! `ux-cue.md` §2, the face-on strike card of §3, the elevation gauge of §4, the guides of §5, the
//! miscue envelope's refusal of §6, and the convex pull → speed map of §2/§10.7.
//!
//! The only thing this module can do to the game is emit a `declaration` — the input log's
//! declaration body (`architecture.md` §6) — and hand it to `Session::request`. Everything else is
//! display math, which `architecture.md` §3 exempts from the numeric discipline precisely because it
//! never re-enters the game except through that logged input.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::math::{Isometry2d, Rot2};
use bevy::prelude::*;
use pool_match::{Declaration, Request};
use pool_rules::facts::PreBall;
use pool_rules::placement::{self, PlacementFault};
use pool_rules::{BallNumber, Call, PlacementDomain, PocketName, Spin, Vec2 as RulesVec2};
use pool_sim::constants as c;
use pool_sim::strike::StrikeDecl;
use pool_sim::{BallState, PocketId};

use crate::bridge::BallStates;
use crate::playback::Playback;
use crate::session::{Awaiting, Game};

// ---------------------------------------------------------------------------
// The spec's constants (`ux-cue.md` §2, §6, §10.3, §10.7)

/// The pull range's end (mm): the cue's full drag-back (§2).
pub const MAX_PULL_MM: f32 = 350.0;
/// The launch speed at full pull (mm/s): the convex map's top (§2/§10.7).
pub const MAX_SPEED_MM_S: f32 = 7000.0;
/// The convex map's exponent: `v = 7000 · (pull / 350)²` (§10.7's candidate). The exponent is a
/// playtest constant; the requirement it must meet — the 0–1500 mm/s band occupying at least 40 % of
/// the pull range — is spec, and the test at this module's foot checks it.
const POWER_EXPONENT: f32 = 2.0;
/// The top of the soft-shot band the requirement is stated on (mm/s).
pub const SOFT_BAND_MM_S: f32 = 1500.0;
/// The envelope test's tolerance (mm): the comparison is on the tip offset in millimetres,
/// `|offset_mm| ≤ ρ_max + 1e-3`, so a declaration authored exactly at the limit is legal
/// (§6/§10.4); the tolerance is ≈ 6.8e-5 in the envelope-fraction unit of §7.
pub const ENVELOPE_TOLERANCE_MM: f64 = 1e-3;
/// The elevation cap for authoring (§10.3): the `(a, b)` frame needs the world-up projected into
/// the face plane, which a vertical cue has not. It bounds authoring, not the model.
pub const MAX_ELEVATION_RAD: f32 = 75.0 * std::f32::consts::PI / 180.0;
/// One elevation notch: 5°, or 1° with `Shift` (§1's modal row).
const ELEVATION_STEP_RAD: f32 = 5.0 * std::f32::consts::PI / 180.0;
/// The fine notch.
const ELEVATION_FINE_RAD: f32 = std::f32::consts::PI / 180.0;
/// The spin selector's reach past the envelope, so §6's refused state is reachable.
const SPIN_SELECT_MAX: f32 = 1.25;
/// One spin key press (envelope fractions; `Shift` gives a fifth of it).
const SPIN_STEP: f32 = 0.05;
/// Within this of the cue ball the pointer does not re-aim (mm): a direction there is noise.
const AIM_DEAD_ZONE_MM: f32 = 40.0;

// ---------------------------------------------------------------------------
// The card's geometry (`ux-cue.md` §3/§10.2) and the palette

/// The card's side (logical pixels; the window's scale factor is 1, so logical = physical).
const CARD_PX: f32 = 320.0;
/// The ball disc's diameter on the card (px).
const CARD_BALL_PX: f32 = 300.0;
/// The envelope ring's radius on the card (px). The ring *is* the limit, and it is also the dot's
/// scale: the authored `(a, b)` is read off it, `1.0` at the ring (§7).
const CARD_RING_PX: f32 = 120.0;
/// The tip-contact dot's radius (px).
const CARD_DOT_R: f32 = 9.0;
/// The spin axis' half-length on the card (px).
const CARD_AXIS_PX: f32 = 118.0;
/// The card's centre, in its own pixels.
const CARD_CENTRE_PX: f32 = CARD_PX * 0.5;

/// The cue's painted length (mm, top-down projection; §4's foreshortening is the honest signal).
const STICK_LEN_MM: f32 = 1450.0;
/// The painter's order of the stick's parts: above the balls.
const Z_STICK: f32 = 2.0;

/// Colours: the guides, the gauge, the cue, the card.
const C_AIM: Color = Color::srgb(0.95, 0.95, 0.95);
const C_GHOST: Color = Color::srgb(0.65, 0.80, 1.00);
const C_OBJECT: Color = Color::srgb(0.95, 0.45, 0.90);
const C_TANGENT: Color = Color::srgb(0.35, 0.95, 0.95);
const C_GAUGE: Color = Color::srgb(1.00, 0.60, 0.20);
const C_GAUGE_REF: Color = Color::srgb(0.62, 0.62, 0.62);
const C_REFUSED: Color = Color::srgb(0.95, 0.15, 0.15);
const C_STICK: Color = Color::srgb(0.85, 0.68, 0.42);
const C_STICK_TIP: Color = Color::srgb(0.12, 0.12, 0.15);
const C_STICK_BUTT: Color = Color::srgb(0.40, 0.26, 0.13);

// ---------------------------------------------------------------------------
// The machine's state

/// The cue input machine's phase: `ux-cue.md` §2's gesture chain, exactly — aim-idle → power-drag →
/// commit, with §6's refusal on the commit path and §2's abandon out of a drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Aim-idle: the pointer aims (no button), the first press on the cloth starts the drag.
    AimIdle,
    /// Power-drag: the aim is locked and the drag back sets the pull; release commits, `Escape`
    /// abandons.
    PowerDrag,
}

/// The aim guides (`ux-cue.md` §5): the path's first contact, or the first cushion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Guide {
    /// A ball is on the path: the ghost ball, the object ball's line, and its tangent.
    Ball {
        /// The first ball the path meets (its sim id, 1..=15).
        ball: u8,
        /// Its centre (mm, table frame).
        target: Vec2,
        /// The ghost ball's centre: where the cue ball's centre is at contact.
        ghost: Vec2,
        /// The object ball's departure direction (unit).
        travel: Vec2,
        /// The tangent line a stun hit leaves the cue ball on (unit).
        tangent: Vec2,
        /// The cut angle (deg) between the cue ball's path and the object ball's line.
        cut_deg: f32,
    },
    /// No ball is on the path: the first cushion, and the reflection off it.
    Cushion {
        /// Where the path meets the cushion (mm).
        hit: Vec2,
        /// The reflected direction (unit).
        reflect: Vec2,
    },
}

/// The ball-in-hand proposal, while the machine awaits a placement (`rules.md` §6).
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The proposed cue-ball centre (mm, table frame).
    pub pos: RulesVec2,
    /// What the placement would draw, if anything: `None` is a legal placement.
    pub fault: Option<PlacementFault>,
}

/// The cue input machine's state: the authored declaration and the gesture it is in.
#[derive(Resource)]
pub struct Cue {
    /// The authored aim: a unit direction in the table frame.
    pub aim: [f32; 2],
    /// The authored pull-back (mm): the power gesture's raw value.
    pub pull_mm: f32,
    /// The authored tip offset `(a, b)`, as fractions of the miscue envelope (§7).
    pub spin: [f32; 2],
    /// The authored elevation (rad), capped at [`MAX_ELEVATION_RAD`].
    pub elevation_rad: f32,
    /// The gesture chain's phase.
    pub phase: Phase,
    /// The confirmed call (`rules.md` §11: a suggested call the player confirms).
    pub call: Call,
    /// The suggested call's options, in order; a safety is always last.
    call_options: Vec<Call>,
    /// Which option `call` holds.
    call_index: usize,
    /// The key the options were built for — the guide's first ball and the pocket it lines up with
    /// best — so the suggestion is rebuilt only when the aim's first contact moves.
    call_key: Option<(u8, &'static str)>,
    /// The tangent guide's toggle (§10.8: default off).
    pub tangent: bool,
    /// The profile's pivot length (mm): the squirt-corrected preview's parameter (§10.8).
    pivot_mm: f64,
    /// The last refused commit's message (§6).
    pub refusal: Option<String>,
    /// The authored guide, for the drawing and the call suggestion.
    pub guide: Guide,
    /// Whether the pointer is dragging inside the strike card.
    spin_drag: bool,
    /// The ball-in-hand proposal, while a placement is awaited.
    pub placement: Option<Proposal>,
}

impl Default for Cue {
    fn default() -> Self {
        Self {
            aim: [1.0, 0.0],
            pull_mm: 0.0,
            spin: [0.0, 0.0],
            elevation_rad: 0.0,
            phase: Phase::AimIdle,
            call: Call::Safety,
            call_options: vec![Call::Safety],
            call_index: 0,
            call_key: None,
            tangent: false,
            pivot_mm: 0.0,
            refusal: None,
            guide: Guide::Cushion {
                hit: Vec2::ZERO,
                reflect: Vec2::X,
            },
            spin_drag: false,
            placement: None,
        }
    }
}

impl Cue {
    /// The authored tip offset's norm, in envelope fractions.
    #[must_use]
    pub fn spin_norm(&self) -> f32 {
        (self.spin[0] * self.spin[0] + self.spin[1] * self.spin[1]).sqrt()
    }

    /// The authored offset in millimetres: `offset_mm = ρ_max · value` (§7's one conversion).
    #[must_use]
    pub fn offset_mm(&self) -> f64 {
        envelope_mm() * f64::from(self.spin_norm())
    }

    /// §6's limit test: `|offset_mm| ≤ ρ_max + 1e-3`. A declaration authored exactly at the limit is
    /// legal; past it the commit is refused.
    #[must_use]
    pub fn legal(&self) -> bool {
        self.offset_mm() <= envelope_mm() + ENVELOPE_TOLERANCE_MM
    }

    /// How far past the envelope the offset is (mm): positive is an overage, and the status line
    /// names it (§6's "offset 27.0 mm is 12.3 mm past the miscue envelope").
    #[must_use]
    pub fn overage_mm(&self) -> f64 {
        self.offset_mm() - envelope_mm()
    }

    /// The authored declaration — the input log's declaration body (`architecture.md` §6) — with
    /// `from_policy` false, the marker no noise applies to (§5).
    #[must_use]
    pub fn declaration(&self) -> Declaration {
        Declaration {
            from_policy: false,
            call: self.call.clone(),
            aim: RulesVec2 {
                x: f64::from(self.aim[0]),
                y: f64::from(self.aim[1]),
            },
            speed: f64::from(speed_from_pull(self.pull_mm)),
            spin: Spin {
                a: f64::from(self.spin[0]),
                b: f64::from(self.spin[1]),
            },
            elevation: f64::from(self.elevation_rad),
        }
    }

    /// Hand the shell the profile's pivot length (`physics.md` §5): the squirt-corrected preview
    /// resolves the launch direction with it (`ux-cue.md` §10.8).
    pub fn set_pivot(&mut self, pivot_mm: f64) {
        self.pivot_mm = pivot_mm;
    }

    /// Author the aim along `from → to` (the screenshot walk's framing; the pointer authors the aim
    /// in play).
    pub fn set_aim_towards(&mut self, from: RulesVec2, to: RulesVec2) {
        let (dx, dy) = ((to.x - from.x) as f32, (to.y - from.y) as f32);
        let length = (dx * dx + dy * dy).sqrt();
        if length > 1e-3 {
            self.aim = [dx / length, dy / length];
        }
    }

    /// Author the pull (mm), clamped into the map's range (§2).
    pub fn set_pull(&mut self, pull_mm: f32) {
        self.pull_mm = pull_mm.clamp(0.0, MAX_PULL_MM);
    }

    /// Author the tip offset, in envelope fractions (§7), clamped into the selectable disc.
    pub fn set_spin(&mut self, spin: [f32; 2]) {
        self.spin = clamp_spin(spin[0], spin[1]);
    }

    /// Author the elevation (rad), clamped to §10.3's authoring cap.
    pub fn set_elevation(&mut self, elevation_rad: f32) {
        self.elevation_rad = elevation_rad.clamp(0.0, MAX_ELEVATION_RAD);
    }

    /// The call the player confirmed, as the HUD words it.
    #[must_use]
    pub fn call_text(&self) -> String {
        call_text(&self.call)
    }
}

/// The miscue envelope's radius (mm), `ρ = R·μ/√(1+μ²)` (`physics.md` §3.6): the simulation's own
/// function, so the card's ring and the refusal can never disagree with the strike's limit.
#[must_use]
pub fn envelope_mm() -> f64 {
    pool_sim::miscue_envelope_mm()
}

/// The convex pull → speed map (§2/§10.7): `v = 7000 · (pull / 350)^2`.
#[must_use]
pub fn speed_from_pull(pull_mm: f32) -> f32 {
    let fraction = (pull_mm / MAX_PULL_MM).clamp(0.0, 1.0);
    MAX_SPEED_MM_S * fraction.powf(POWER_EXPONENT)
}

/// The pull a speed is authored at: the map's inverse, for the read-out and the test.
#[must_use]
pub fn pull_for_speed(speed_mm_s: f32) -> f32 {
    MAX_PULL_MM * (speed_mm_s / MAX_SPEED_MM_S).max(0.0).powf(1.0 / POWER_EXPONENT)
}

/// A call, as the HUD words it.
#[must_use]
pub fn call_text(call: &Call) -> String {
    match call {
        Call::Ball { ball, pocket } => format!("ball {} → {}", ball.0, pocket),
        Call::Safety => "safety".to_string(),
        Call::Break => "break".to_string(),
    }
}

// ---------------------------------------------------------------------------
// The systems

/// The cue systems, in the order one frame needs: the proposal and the authoring read the states
/// `playback` wrote, and the card and the table draw what they authored.
pub fn systems(app: &mut App) {
    app.init_resource::<Cue>()
        .add_systems(Startup, (spawn_cues, spawn_card))
        .add_systems(
            Update,
            (choose_option, propose_placement, author)
                .chain()
                .in_set(crate::ShellSet::Input),
        )
        .add_systems(
            Update,
            (sync_stick, sync_card, sync_labels, draw_table)
                .chain()
                .in_set(crate::ShellSet::Draw),
        );
}

/// The choice menus' keys (`rules-break.md` §3's trees). `ui.rs` presents the options; the digit
/// that picks one is an input, and every input the shell has goes through a `Request`.
fn choose_option(gestures: Gestures, mut game: ResMut<Game>) {
    let Awaiting::Choice { offers, .. } = game.awaiting() else {
        return;
    };
    let mut index = 0;
    for offer in &offers {
        for option in &offer.options {
            if index < MENU_KEYS.len() && gestures.keys.just_pressed(MENU_KEYS[index]) {
                let _ = game.request(Request::Option {
                    option_id: option.option.name().to_string(),
                });
                return;
            }
            index += 1;
        }
    }
}

/// The keys a menu option answers to, in the order `ui.rs` numbers them.
const MENU_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// The pointer and keyboard state a gesture reads. Bundled because the gesture chain reads them
/// together and a Bevy system's parameter list is not a signature.
#[derive(bevy::ecs::system::SystemParam)]
struct Gestures<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
    wheel: MessageReader<'w, 's, MouseWheel>,
    window: Single<'w, 's, &'static Window>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<Camera2d>>,
    card: Single<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<StrikeCard>>,
}

impl Gestures<'_, '_> {
    /// The cursor in the table frame (mm), when it is over the window.
    fn cursor_world(&self) -> Option<Vec2> {
        let cursor = self.window.cursor_position()?;
        let (camera, transform) = *self.camera;
        camera.viewport_to_world_2d(transform, cursor).ok()
    }

    /// The cursor's position in the card's own pixels (y down), when it is over the card.
    fn card_local(&self) -> Option<Vec2> {
        let cursor = self.window.cursor_position()?;
        let (node, transform) = *self.card;
        node.contains_point(*transform, cursor)
            .then(|| node.normalize_point(*transform, cursor))
            .flatten()
            .map(|normalized| {
                // The normalised point spans [-0.5, 0.5]; the card's pixels span [0, CARD_PX].
                (normalized + Vec2::splat(0.5)) * CARD_PX
            })
    }

    /// `Shift`: the fine step modifier.
    fn fine(&self) -> bool {
        self.keys.pressed(KeyCode::ShiftLeft) || self.keys.pressed(KeyCode::ShiftRight)
    }
}

// ---------------------------------------------------------------------------
// Ball in hand (`rules.md` §6): the proposal the pointer drags

/// The placement state machine: while the machine awaits a placement, the cue ball follows the
/// pointer inside the domain, and a click submits the placement it proposes. An illegal proposal is
/// drawn in red and its click is refused at the boundary — the machine's own validation
/// (`pool_rules::placement`), never a second rule set.
fn propose_placement(
    gestures: Gestures,
    mut game: ResMut<Game>,
    mut cue: ResMut<Cue>,
    mut states: ResMut<BallStates>,
) {
    let Awaiting::Placement { domain, .. } = game.awaiting() else {
        cue.placement = None;
        return;
    };
    let balls = pre_balls(game.positions());
    let proposal = cue
        .placement
        .get_or_insert_with(|| Proposal {
            pos: RulesVec2 {
                x: game.positions()[0].pos_mm[0],
                y: game.positions()[0].pos_mm[1],
            },
            fault: None,
        });
    if let Some(cursor) = gestures.cursor_world() {
        proposal.pos = clamp_to_domain(
            domain,
            RulesVec2 {
                x: f64::from(cursor.x),
                y: f64::from(cursor.y),
            },
        );
    }
    proposal.fault = placement::validate(domain, proposal.pos, &balls).err();

    // The cue ball is drawn where the proposal is: the shell's one visual write, and `playback`
    // rewrites it from the session the frame after a placement lands.
    states.0[0] = BallState::at_rest([proposal.pos.x, proposal.pos.y, c::BALL_RADIUS_MM]);
    if proposal.fault.is_none() && gestures.buttons.just_pressed(MouseButton::Left) {
        let request = Request::Placement {
            domain,
            pos: proposal.pos,
        };
        if game.request(request).is_ok() {
            cue.placement = None;
        }
    }
}

/// The domain's ink: a proposed centre outside the playing surface or the placement's domain is
/// clamped back into it, one millimetre clear of the head string's knife edge (2.13 reads a ball
/// *on* the line as not above it).
fn clamp_to_domain(domain: PlacementDomain, pos: RulesVec2) -> RulesVec2 {
    let margin = c::BALL_RADIUS_MM;
    let mut clamped = RulesVec2 {
        x: pos.x.clamp(-c::HALF_LEN_MM + margin, c::HALF_LEN_MM - margin),
        y: pos.y.clamp(-c::HALF_WIDTH_MM + margin, c::HALF_WIDTH_MM - margin),
    };
    if domain == PlacementDomain::AboveHeadString {
        clamped.x = clamped.x.min(c::HEAD_STRING_X_MM - 1.0);
    }
    clamped
}

/// The rules layer's ball array, from the session's position (`rules.md` §10.8).
fn pre_balls(positions: &[BallState; 16]) -> Vec<PreBall> {
    positions
        .iter()
        .enumerate()
        .filter(|(_, state)| state.in_play())
        .map(|(id, state)| PreBall {
            id: id as u8,
            x_mm: state.pos_mm[0],
            y_mm: state.pos_mm[1],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The gesture chain (`ux-cue.md` §2)

/// Aim, power, spin, elevation, the call, and the commit path — the whole chain of §2 in one system,
/// because its transitions are ordered: aim is locked by the press, the press starts the pull, the
/// pull's release commits, and every one of them reads the same pointer state.
fn author(
    mut gestures: Gestures,
    mut game: ResMut<Game>,
    mut cue: ResMut<Cue>,
    mut playback: ResMut<Playback>,
) {
    let Awaiting::Shot { is_break, .. } = game.awaiting() else {
        cue.phase = Phase::AimIdle;
        return;
    };
    if !playback.at_rest() {
        return; // the shot is being presented; the next authoring opens at rest
    }
    let before = (cue.aim, cue.pull_mm, cue.spin, cue.elevation_rad);
    let cue_ball = Vec2::new(
        game.positions()[0].pos_mm[0] as f32,
        game.positions()[0].pos_mm[1] as f32,
    );
    let card_local = gestures.card_local();

    author_aim(&gestures, &mut cue, cue_ball, card_local);
    author_spin(&mut gestures, &mut cue, card_local);
    author_elevation(&mut gestures, &mut cue);
    let mut commit_now = author_power(&gestures, &mut cue, cue_ball);
    author_guide(&mut gestures, &mut cue, &game, is_break);

    // ---- the abandon and the keyboard commit (§2/§10.7).
    if gestures.keys.just_pressed(KeyCode::Escape) && cue.phase == Phase::PowerDrag {
        cue.phase = Phase::AimIdle;
        cue.pull_mm = 0.0;
        cue.refusal = None;
    }
    if gestures.keys.just_pressed(KeyCode::Enter) {
        commit_now = true;
    }
    if commit_now {
        commit(&mut game, &mut cue, &mut playback);
    }

    // Any change to the authored declaration withdraws a refusal: it described the declaration as it
    // was (§6).
    if before != (cue.aim, cue.pull_mm, cue.spin, cue.elevation_rad) {
        cue.refusal = None;
    }
}

/// §2's aim: the pointer always aims, so the first press is free for power. While the pointer is
/// inside the card the aim is not touched — the card is a separate, non-modal surface.
fn author_aim(gestures: &Gestures, cue: &mut Cue, cue_ball: Vec2, card_local: Option<Vec2>) {
    if cue.phase != Phase::AimIdle || cue.spin_drag || card_local.is_some() {
        return;
    }
    let Some(cursor) = gestures.cursor_world() else {
        return;
    };
    let direction = cursor - cue_ball;
    if direction.length() > AIM_DEAD_ZONE_MM {
        cue.aim = [direction.x, direction.y];
        cue.aim = normalize(cue.aim);
    }
}

/// §2's spin gesture: the drag inside the card, plus §1's keyboard pair.
fn author_spin(gestures: &mut Gestures, cue: &mut Cue, card_local: Option<Vec2>) {
    if let Some(local) = card_local {
        if gestures.buttons.just_pressed(MouseButton::Left) {
            cue.spin_drag = true;
        }
        if cue.spin_drag {
            cue.spin = clamp_spin(
                (local.x - CARD_CENTRE_PX) / CARD_RING_PX,
                -(local.y - CARD_CENTRE_PX) / CARD_RING_PX,
            );
        }
    }
    if cue.spin_drag && gestures.buttons.just_released(MouseButton::Left) {
        cue.spin_drag = false;
    }
    let step = if gestures.fine() { SPIN_STEP / 5.0 } else { SPIN_STEP };
    let (mut across, mut up) = (0.0, 0.0);
    if gestures.keys.just_pressed(KeyCode::KeyD) {
        across += step;
    }
    if gestures.keys.just_pressed(KeyCode::KeyA) {
        across -= step;
    }
    if gestures.keys.just_pressed(KeyCode::KeyW) {
        up += step;
    }
    if gestures.keys.just_pressed(KeyCode::KeyS) {
        up -= step;
    }
    if across != 0.0 || up != 0.0 {
        cue.spin = clamp_spin(cue.spin[0] + across, cue.spin[1] + up);
    }
    if gestures.keys.just_pressed(KeyCode::KeyC) {
        cue.spin = [0.0, 0.0];
    }
}

/// §1's modal row, one notch at a time: the wheel and Up/Down, capped at 75° for authoring (§10.3).
fn author_elevation(gestures: &mut Gestures, cue: &mut Cue) {
    let step = if gestures.fine() {
        ELEVATION_FINE_RAD
    } else {
        ELEVATION_STEP_RAD
    };
    let mut delta = 0.0;
    if gestures.keys.just_pressed(KeyCode::ArrowUp) {
        delta += step;
    }
    if gestures.keys.just_pressed(KeyCode::ArrowDown) {
        delta -= step;
    }
    for event in gestures.wheel.read() {
        if event.unit == MouseScrollUnit::Line {
            delta += event.y * step * 0.5;
        }
    }
    if delta != 0.0 {
        cue.elevation_rad = (cue.elevation_rad + delta).clamp(0.0, MAX_ELEVATION_RAD);
    }
}

/// §2's power gesture: press on the cloth locks the aim and starts the drag; release commits. The
/// drag-back distance is the pull, and the convex map turns it into the speed (§10.7).
fn author_power(gestures: &Gestures, cue: &mut Cue, cue_ball: Vec2) -> bool {
    if gestures.buttons.just_pressed(MouseButton::Left) && gestures.card_local().is_none() {
        cue.phase = Phase::PowerDrag;
        cue.pull_mm = 0.0;
    }
    if cue.phase != Phase::PowerDrag {
        return false;
    }
    if gestures.buttons.pressed(MouseButton::Left)
        && let Some(cursor) = gestures.cursor_world()
    {
        let back = (cue_ball - cursor).dot(Vec2::new(cue.aim[0], cue.aim[1]));
        cue.pull_mm = back.clamp(0.0, MAX_PULL_MM);
    }
    gestures.buttons.just_released(MouseButton::Left)
}

/// §5's guides and `rules.md` §11's suggested call: the call follows the aim's first contact and the
/// pocket it lines up with, and `Tab` cycles the suggestions (a safety is always one of them).
fn author_guide(gestures: &mut Gestures, cue: &mut Cue, game: &Game, is_break: bool) {
    cue.guide = aim_guide(&game.positions()[1..], cue_ball_of(game), path_direction(cue));
    refresh_call(cue, is_break);
    if gestures.keys.just_pressed(KeyCode::Tab) {
        cue.call_index = (cue.call_index + 1) % cue.call_options.len();
    }
    if gestures.keys.just_pressed(KeyCode::KeyT) {
        cue.tangent = !cue.tangent;
    }
    cue.call = if is_break {
        Call::Break
    } else {
        cue.call_options[cue.call_index].clone()
    };
}

/// The cue ball's centre (mm, table frame) at the session's current position.
fn cue_ball_of(game: &Game) -> Vec2 {
    Vec2::new(
        game.positions()[0].pos_mm[0] as f32,
        game.positions()[0].pos_mm[1] as f32,
    )
}

/// Normalise a 2-vector the display math carries.
fn normalize(v: [f32; 2]) -> [f32; 2] {
    let length = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if length > 1e-6 {
        [v[0] / length, v[1] / length]
    } else {
        v
    }
}

/// Clamp the authored offset into the selectable disc of radius [`SPIN_SELECT_MAX`].
fn clamp_spin(a: f32, b: f32) -> [f32; 2] {
    let norm = (a * a + b * b).sqrt();
    if norm > SPIN_SELECT_MAX {
        [a * SPIN_SELECT_MAX / norm, b * SPIN_SELECT_MAX / norm]
    } else {
        [a, b]
    }
}

/// The commit path (§2's release, §10.7's `Enter`): an illegal declaration is visibly refused and
/// cannot commit, and a zero-power one is no strike at all.
fn commit(game: &mut Game, cue: &mut Cue, playback: &mut Playback) {
    cue.phase = Phase::AimIdle;
    if !cue.legal() {
        cue.refusal = Some(refusal_text(cue));
        return;
    }
    if speed_from_pull(cue.pull_mm) <= 0.0 {
        cue.refusal = Some("no power: press on the cloth and drag back to set it".to_string());
        return;
    }
    let declaration = cue.declaration();
    match game.request(Request::Declaration(declaration)) {
        Ok(()) => {
            cue.refusal = None;
            cue.pull_mm = 0.0;
            if let Some(shot) = game.last_shot() {
                playback.begin(shot.clone());
            }
        }
        Err(error) => cue.refusal = Some(error.to_string()),
    }
}

/// §6's refusal line: the offset, the envelope, and the overage it names.
#[must_use]
pub fn refusal_text(cue: &Cue) -> String {
    let envelope = envelope_mm();
    format!(
        "offset {:.1} mm is {:.1} mm past the miscue envelope ({:.1} mm = {:.3} R, mu {:.1}): the declaration cannot be committed",
        cue.offset_mm(),
        cue.overage_mm(),
        envelope,
        envelope / c::BALL_RADIUS_MM,
        c::TIP_FRICTION_MU,
    )
}

// ---------------------------------------------------------------------------
// The guides (`ux-cue.md` §5)

/// The cue-ball path's horizontal direction the guides follow: the squirt-corrected launch direction
/// (§10.8 — the aim line and the ghost ball follow the corrected path while the stick stays on the
/// input aim). The direction is speed-independent, so a unit speed resolves it; the simulation's own
/// conversion is the one authority on squirt.
fn path_direction(cue: &Cue) -> Vec2 {
    let aim = Vec2::new(cue.aim[0], cue.aim[1]);
    let declaration = StrikeDecl {
        aim: [f64::from(cue.aim[0]), f64::from(cue.aim[1])],
        speed_mm_s: 1.0,
        spin: [f64::from(cue.spin[0]), f64::from(cue.spin[1])],
        elevation_rad: f64::from(cue.elevation_rad),
    };
    let Ok(outcome) = declaration.resolve(cue.pivot_mm) else {
        return aim;
    };
    let horizontal = Vec2::new(outcome.v_mm_s.x as f32, outcome.v_mm_s.y as f32);
    if horizontal.length() > 1e-3 {
        horizontal.normalize()
    } else {
        aim
    }
}

/// The first contact of the path (`ux-cue.md` §5): a ball's ghost position, or the first cushion.
/// The objects are the object balls in play, in the sim's canonical order.
#[must_use]
pub fn aim_guide(objects: &[BallState], cue: Vec2, dir: Vec2) -> Guide {
    let mut best: Option<(f32, u8, Vec2)> = None;
    for (index, state) in objects.iter().enumerate() {
        if !state.in_play() {
            continue;
        }
        let centre = Vec2::new(state.pos_mm[0] as f32, state.pos_mm[1] as f32);
        let to_ball = centre - cue;
        let along = to_ball.dot(dir);
        if along <= 0.0 {
            continue;
        }
        let perpendicular_sq = to_ball.length_squared() - along * along;
        let contact = 2.0 * c::BALL_RADIUS_MM as f32;
        if perpendicular_sq > contact * contact {
            continue;
        }
        let t = along - (contact * contact - perpendicular_sq).sqrt();
        if t < 0.0 || best.is_some_and(|(closest, _, _)| t >= closest) {
            continue;
        }
        best = Some((t, index as u8 + 1, centre));
    }
    if let Some((t, ball, centre)) = best {
        let ghost = cue + dir * t;
        let travel = (centre - ghost).normalize();
        let mut tangent = Vec2::new(-travel.y, travel.x);
        if tangent.dot(dir) < 0.0 {
            tangent = -tangent;
        }
        let cut = travel.dot(dir).clamp(-1.0, 1.0).acos().to_degrees();
        return Guide::Ball {
            ball,
            target: centre,
            ghost,
            travel,
            tangent,
            cut_deg: cut,
        };
    }
    let mut best_t = f32::INFINITY;
    let mut normal = Vec2::X;
    for (axis, limit) in [(0usize, c::HALF_LEN_MM as f32), (1usize, c::HALF_WIDTH_MM as f32)] {
        for sign in [1.0f32, -1.0] {
            let component = if axis == 0 { dir.x } else { dir.y } * sign;
            if component <= 1e-6 {
                continue;
            }
            let position = if axis == 0 { cue.x } else { cue.y };
            let t = (limit * sign - position) / component;
            if t > 0.0 && t < best_t {
                best_t = t;
                normal = if axis == 0 {
                    Vec2::new(sign, 0.0)
                } else {
                    Vec2::new(0.0, sign)
                };
            }
        }
    }
    if !best_t.is_finite() {
        best_t = 0.0;
    }
    let hit = cue + dir * best_t;
    Guide::Cushion {
        hit,
        reflect: dir - 2.0 * dir.dot(normal) * normal,
    }
}

/// The six pockets' mouths, in the frame's vocabulary (`physics.md` §2, `rules.md` §10.3).
#[must_use]
pub fn pockets() -> [(Vec2, PocketId); 6] {
    let half_len = c::HALF_LEN_MM as f32;
    let half_width = c::HALF_WIDTH_MM as f32;
    [
        (Vec2::new(half_len, half_width), PocketId::FootPlusY),
        (Vec2::new(half_len, -half_width), PocketId::FootMinusY),
        (Vec2::new(-half_len, half_width), PocketId::HeadPlusY),
        (Vec2::new(-half_len, -half_width), PocketId::HeadMinusY),
        (Vec2::new(0.0, -half_width), PocketId::SideMinusY),
        (Vec2::new(0.0, half_width), PocketId::SidePlusY),
    ]
}

/// The suggested calls (`rules.md` §11: "a suggested call the player confirms"): the guide's first
/// ball against each pocket it lines up with, best-aligned first, and a safety last. On the break the
/// call is not the player's (`rules.md` §1: the call is `Break`).
fn suggested_calls(guide: &Guide) -> Vec<Call> {
    let mut calls = Vec::with_capacity(7);
    if let Guide::Ball {
        ball,
        target,
        travel,
        ..
    } = guide
    {
        let mut ranked: Vec<(f32, PocketId)> = pockets()
            .into_iter()
            .filter_map(|(mouth, pocket)| {
                let to_pocket = (mouth - *target).normalize_or_zero();
                let alignment = to_pocket.dot(*travel);
                (alignment > 0.0).then_some((alignment, pocket))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        calls.extend(ranked.into_iter().map(|(_, pocket)| Call::Ball {
            ball: BallNumber(*ball),
            pocket: PocketName::from(pocket).name().to_string(),
        }));
    }
    calls.push(Call::Safety);
    calls
}

/// Rebuild the suggested calls when the aim's first contact moves, and keep the confirmed call
/// pointing at a live option.
fn refresh_call(cue: &mut Cue, is_break: bool) {
    if is_break {
        cue.call_index = 0;
        return;
    }
    let key = match cue.guide {
        Guide::Ball {
            ball,
            target,
            travel,
            ..
        } => {
            let best = pockets()
                .into_iter()
                .map(|(mouth, pocket)| ((mouth - target).normalize_or_zero().dot(travel), pocket))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            best.map(|(_, pocket)| (ball, PocketName::from(pocket).name()))
        }
        Guide::Cushion { .. } => None,
    };
    if cue.call_key != key {
        cue.call_key = key;
        cue.call_index = 0;
        cue.call_options = suggested_calls(&cue.guide);
    }
    cue.call_index = cue.call_index.min(cue.call_options.len() - 1);
}

// ---------------------------------------------------------------------------
// The table's drawing: the cue, the guides, the gauge

/// The cue stick's parts (the top-down projection of §4's foreshortening).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Stick {
    /// The shaft.
    Body,
    /// The tip.
    Tip,
    /// The butt.
    Butt,
}

/// The gauge's label and the pull's, in world space so they read as labels, not as guides.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Label {
    /// `elevation 45 deg` (§10.1's numeric label).
    Elevation,
    /// `pull 175 mm`, on the cue's side of the gap (§9's disposition: not next to the cue ball).
    Pull,
    /// A tick of the gauge's scale (0/30/60/90), by index.
    Tick(usize),
}

/// The gauge's tick scale (`ux-cue.md` §10.1): 0/30/60/90 degrees.
const GAUGE_TICKS: [f32; 4] = [0.0, 30.0, 60.0, 90.0];
/// The gauge's arm (mm) and the distance its label sits beyond it.
const GAUGE_ARM_MM: f32 = 95.0;
/// Where the gauge hangs off the cue ball (mm).
const GAUGE_OFFSET: Vec2 = Vec2::new(-30.0, -150.0);

/// Spawn the cue stick's three parts and the gauge's labels.
fn spawn_cues(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let unit = meshes.add(Rectangle::new(1.0, 1.0));
    for (part, color, z) in [
        (Stick::Body, C_STICK, Z_STICK),
        (Stick::Tip, C_STICK_TIP, Z_STICK + 0.1),
        (Stick::Butt, C_STICK_BUTT, Z_STICK),
    ] {
        commands.spawn((
            Mesh2d(unit.clone()),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(0.0, 0.0, z),
            part,
        ));
    }
    commands.spawn((
        Text2d::new(""),
        TextFont::from_font_size(30.0),
        TextColor(C_GAUGE),
        Transform::from_xyz(0.0, 0.0, 3.0),
        Label::Elevation,
    ));
    commands.spawn((
        Text2d::new(""),
        TextFont::from_font_size(28.0),
        TextColor(C_AIM),
        Transform::from_xyz(0.0, 0.0, 3.0),
        Label::Pull,
    ));
    for (index, degrees) in GAUGE_TICKS.iter().enumerate() {
        commands.spawn((
            Text2d::new(format!("{degrees:.0}")),
            TextFont::from_font_size(20.0),
            TextColor(C_GAUGE_REF),
            Transform::from_xyz(0.0, 0.0, 3.0),
            Label::Tick(index),
        ));
    }
}

/// Where the authored declaration stands: the stick, the elevation, and the pull, in the table's own
/// millimetres.
struct TableCue {
    /// The cue ball's centre.
    cue_ball: Vec2,
    /// The aim (unit).
    aim: Vec2,
    /// The elevation.
    elevation_rad: f32,
    /// The pull-back.
    pull_mm: f32,
}

impl TableCue {
    /// The tip's position: the ball's near surface plus the pull, foreshortened by the elevation.
    fn tip(&self) -> Vec2 {
        let gap = (c::BALL_RADIUS_MM as f32 * 0.97 + self.pull_mm) * self.elevation_rad.cos();
        self.cue_ball - self.aim * gap
    }
}

/// The drawing condition: the cue is on the cloth while a shot is authored.
fn table_cue(game: &Game, cue: &Cue, playback: &Playback) -> Option<TableCue> {
    if !playback.at_rest() || !matches!(game.awaiting(), Awaiting::Shot { .. }) {
        return None;
    }
    Some(TableCue {
        cue_ball: Vec2::new(
            game.positions()[0].pos_mm[0] as f32,
            game.positions()[0].pos_mm[1] as f32,
        ),
        aim: Vec2::new(cue.aim[0], cue.aim[1]),
        elevation_rad: cue.elevation_rad,
        pull_mm: cue.pull_mm,
    })
}

/// Whether the aim guides are on the cloth — the condition `ux-cue.md` §5's legend follows.
#[must_use]
pub fn guides_drawn(game: &Game, cue: &Cue, playback: &Playback) -> bool {
    table_cue(game, cue, playback).is_some()
}

/// The top-down cue: its projected length shrinks by `cos(elevation)` (`ux-cue.md` §4's honest,
/// insufficient signal, kept as the supporting one).
fn sync_stick(
    game: Res<Game>,
    cue: Res<Cue>,
    playback: Res<Playback>,
    mut sticks: Query<(&Stick, &mut Transform, &mut Visibility)>,
) {
    let Some(table) = table_cue(&game, &cue, &playback) else {
        for (_, _, mut visibility) in &mut sticks {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let cos_e = table.elevation_rad.cos();
    let tip = table.tip();
    let rotation = Quat::from_rotation_z(table.aim.y.atan2(table.aim.x));
    for (part, mut transform, mut visibility) in &mut sticks {
        *visibility = Visibility::Inherited;
        let (center, length, width) = match part {
            Stick::Body => (tip - table.aim * (STICK_LEN_MM * cos_e * 0.5), STICK_LEN_MM * cos_e, 16.0),
            Stick::Tip => (tip - table.aim * (9.0 * cos_e), 18.0 * cos_e, 12.0),
            Stick::Butt => (
                tip - table.aim * (STICK_LEN_MM * cos_e - 30.0 * cos_e),
                60.0 * cos_e,
                18.0,
            ),
        };
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        transform.rotation = rotation;
        transform.scale = Vec3::new(length.max(1.0), width, 1.0);
    }
}

/// The elevation gauge (§4's required signal, §10.1's tick scale), its label, and the pull's
/// dimension bracket, all in world space beside the cue ball.
fn sync_labels(
    game: Res<Game>,
    cue: Res<Cue>,
    playback: Res<Playback>,
    mut labels: Query<(&Label, &mut Text2d, &mut Transform, &mut TextColor)>,
) {
    let Some(table) = table_cue(&game, &cue, &playback) else {
        for (_, mut text, _, _) in &mut labels {
            if !text.0.is_empty() {
                text.0 = String::new();
            }
        }
        return;
    };
    let origin = table.cue_ball + GAUGE_OFFSET;
    let tip = table.tip();
    let side = Vec2::new(-table.aim.y, table.aim.x);
    for (label, mut text, mut transform, mut color) in &mut labels {
        let (wanted, at, ink) = match label {
            Label::Elevation => (
                format!("elevation {:.0} deg", table.elevation_rad.to_degrees()),
                origin + Vec2::new(GAUGE_ARM_MM + 10.0, GAUGE_ARM_MM * 0.55),
                C_GAUGE,
            ),
            Label::Pull => (
                if table.pull_mm >= 5.0 {
                    format!("pull {:.0} mm", table.pull_mm)
                } else {
                    String::new()
                },
                tip + side * 46.0,
                C_AIM,
            ),
            Label::Tick(index) => {
                let radians = GAUGE_TICKS[*index].to_radians();
                let direction = Vec2::new(radians.cos(), radians.sin());
                (
                    format!("{:.0}", GAUGE_TICKS[*index]),
                    origin + direction * (GAUGE_ARM_MM + 26.0),
                    C_GAUGE_REF,
                )
            }
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
        color.0 = ink;
        transform.translation.x = at.x;
        transform.translation.y = at.y;
    }
}

/// The guides, the gauge's ink, and the placement's ring: everything `ux-cue.md` §5 draws on the
/// cloth. The legend that must stay on whenever guides are drawn is the HUD's (`ui.rs`).
fn draw_table(
    mut gizmos: Gizmos,
    game: Res<Game>,
    cue: Res<Cue>,
    playback: Res<Playback>,
) {
    if let Awaiting::Placement { .. } = game.awaiting() {
        let cue_ball = Vec2::new(
            game.positions()[0].pos_mm[0] as f32,
            game.positions()[0].pos_mm[1] as f32,
        );
        let ink = if cue.placement.as_ref().is_some_and(|p| p.fault.is_some()) {
            C_REFUSED
        } else {
            C_GHOST
        };
        gizmos.circle_2d(Isometry2d::from_translation(cue_ball), 2.0 * c::BALL_RADIUS_MM as f32, ink);
        return;
    }
    let Some(table) = table_cue(&game, &cue, &playback) else {
        return;
    };
    let aim = table.aim;
    match cue.guide {
        Guide::Ball {
            target,
            ghost,
            travel,
            tangent,
            ..
        } => {
            gizmos.line_2d(table.cue_ball, ghost, C_AIM);
            gizmos.circle_2d(Isometry2d::from_translation(ghost), 2.0 * c::BALL_RADIUS_MM as f32, C_GHOST);
            gizmos.arrow_2d(target, target + travel * 320.0, C_OBJECT);
            if cue.tangent {
                gizmos.arrow_2d(ghost, ghost + tangent * 300.0, C_TANGENT);
            }
        }
        Guide::Cushion { hit, reflect } => {
            gizmos.line_2d(table.cue_ball, hit, C_AIM);
            gizmos.circle_2d(Isometry2d::from_translation(hit), 14.0, C_GHOST);
            gizmos.arrow_2d(hit, hit + reflect * 420.0, C_TANGENT);
        }
    }

    // The elevation gauge (§4): the cloth baseline, the cue at its real angle, the arc between them,
    // and §10.1's tick scale.
    let origin = table.cue_ball + GAUGE_OFFSET;
    let radians = table.elevation_rad;
    gizmos.line_2d(origin, origin + Vec2::new(GAUGE_ARM_MM, 0.0), C_GAUGE_REF);
    let direction = Vec2::new(radians.cos(), radians.sin());
    gizmos.line_2d(origin, origin + direction * GAUGE_ARM_MM, C_GAUGE);
    for degrees in GAUGE_TICKS {
        let at = degrees.to_radians();
        let tick = Vec2::new(at.cos(), at.sin());
        gizmos.line_2d(
            origin + tick * GAUGE_ARM_MM,
            origin + tick * (GAUGE_ARM_MM + 10.0),
            C_GAUGE_REF,
        );
    }
    if radians > 0.01 {
        gizmos.short_arc_2d_between(
            origin,
            origin + Vec2::X * (GAUGE_ARM_MM * 0.55),
            origin + direction * (GAUGE_ARM_MM * 0.55),
            C_GAUGE,
        );
    }

    // The pull's dimension bracket (§2: the gap alone is not self-calibrating), on the cue's side of
    // the gap so it cannot be read as another ball.
    if table.pull_mm > 1.0 {
        let side = Vec2::new(-aim.y, aim.x);
        let tip = table.tip();
        let near = table.cue_ball - aim * (c::BALL_RADIUS_MM as f32 * 0.97);
        let bracket = (near - tip).length();
        let _ = bracket;
        let cap = 14.0;
        gizmos.line_2d(near, tip, C_AIM);
        gizmos.line_2d(near - side * cap, near + side * cap, C_AIM);
        gizmos.line_2d(tip - side * cap, tip + side * cap, C_AIM);
    }
}

// ---------------------------------------------------------------------------
// The strike card (`ux-cue.md` §3/§10.2)

/// The card's frame: face-on, so it has no shaft across it, and `(a, b)` read directly off the ring.
#[derive(Component)]
pub struct StrikeCard;

/// The tip-contact dot.
#[derive(Component)]
struct CardDot;

/// The dot's halo, so it reads on the white ball face too.
#[derive(Component)]
struct CardHalo;

/// The spin axis' rotating container.
#[derive(Component)]
struct CardAxis;

/// The card's read-out: the axis glyphs and the numeric value under the card.
#[derive(Component)]
struct CardGlyph;

/// The card's numeric line.
#[derive(Component)]
struct CardValue;

/// A card part: an absolutely positioned box inside the card, in the card's own pixels.
fn part(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(left),
        top: px(top),
        width: px(width),
        height: px(height),
        ..default()
    }
}

/// The card's ball face (`ux-cue.md` §3's widget, drawn face-on per §10.2).
fn card_ball() -> impl Bundle {
    let inset = (CARD_PX - CARD_BALL_PX) * 0.5;
    (
        Node {
            border_radius: BorderRadius::MAX,
            ..part(inset, inset, CARD_BALL_PX, CARD_BALL_PX)
        },
        BackgroundColor(Color::srgb(0.93, 0.93, 0.90)),
    )
}

/// The miscue envelope's ring: the limit itself (§6), and the dot's scale (§7).
fn card_ring() -> impl Bundle {
    let diameter = 2.0 * CARD_RING_PX;
    (
        Node {
            border_radius: BorderRadius::MAX,
            border: px(2).all(),
            ..part(
                CARD_CENTRE_PX - CARD_RING_PX,
                CARD_CENTRE_PX - CARD_RING_PX,
                diameter,
                diameter,
            )
        },
        BorderColor::all(C_GAUGE),
    )
}

/// One bar of the face-centre crosshair: the fixed reference the dot is read against.
fn card_cross(horizontal: bool) -> impl Bundle {
    let arm = 2.0 * CARD_RING_PX + 28.0;
    let (width, height, left, top) = if horizontal {
        (arm, 1.0, CARD_CENTRE_PX - arm * 0.5, CARD_CENTRE_PX - 0.5)
    } else {
        (1.0, arm, CARD_CENTRE_PX - 0.5, CARD_CENTRE_PX - arm * 0.5)
    };
    (
        part(left, top, width, height),
        BackgroundColor(Color::srgb(0.35, 0.35, 0.38)),
    )
}

/// The spin axis: a double-headed arrow, because it is an axis and not a direction (§3).
fn card_axis() -> impl Bundle {
    let ink = Color::srgb(0.35, 0.95, 0.45);
    (
        CardAxis,
        part(
            CARD_CENTRE_PX - CARD_AXIS_PX,
            CARD_CENTRE_PX - 1.5,
            2.0 * CARD_AXIS_PX,
            3.0,
        ),
        BackgroundColor(ink),
        children![card_head(-5.0, ink), card_head(CARD_AXIS_PX * 2.0 - 5.0, ink)],
    )
}

/// One arrowhead of the spin axis: a small square turned onto its corner.
fn card_head(left: f32, ink: Color) -> impl Bundle {
    (
        part(left, -2.5, 8.0, 8.0),
        BackgroundColor(ink),
        UiTransform::from_rotation(Rot2::degrees(45.0)),
    )
}

/// The tip-contact marker: the dot, and the halo that lets it read on the white ball face too.
fn card_dot() -> impl Bundle {
    (
        CardDot,
        Node {
            border_radius: BorderRadius::MAX,
            ..part(
                CARD_CENTRE_PX - CARD_DOT_R,
                CARD_CENTRE_PX - CARD_DOT_R,
                2.0 * CARD_DOT_R,
                2.0 * CARD_DOT_R,
            )
        },
        BackgroundColor(Color::srgb(0.05, 0.05, 0.06)),
    )
}

/// The tip-contact marker's halo.
fn card_halo() -> impl Bundle {
    let radius = CARD_DOT_R + 3.0;
    (
        CardHalo,
        Node {
            border_radius: BorderRadius::MAX,
            ..part(
                CARD_CENTRE_PX - radius,
                CARD_CENTRE_PX - radius,
                2.0 * radius,
                2.0 * radius,
            )
        },
        BackgroundColor(Color::srgb(0.98, 0.98, 0.98)),
    )
}

/// One axis glyph (§10.3): `R`/`L` on the face's left and right edges, `T`/`D` on its top and bottom.
fn card_glyph(letter: &'static str, left: f32, top: f32) -> impl Bundle {
    (
        Text::new(letter),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(0.75, 0.75, 0.70)),
        part(left, top, 14.0, 20.0),
        CardGlyph,
    )
}

/// Spawn the card: the ball, the envelope ring, the face-centre crosshair, the dot, the spin axis,
/// the axis glyphs, and the numeric line.
fn spawn_card(mut commands: Commands) {
    commands.spawn((
        StrikeCard,
        Node {
            position_type: PositionType::Absolute,
            right: px(16),
            bottom: px(16),
            width: px(CARD_PX),
            height: px(CARD_PX),
            border: px(1).all(),
            ..default()
        },
        BackgroundColor(Color::srgb(0.03, 0.04, 0.05)),
        BorderColor::all(Color::srgb(0.35, 0.35, 0.45)),
        children![
            card_ball(),
            card_ring(),
            card_cross(true),
            card_cross(false),
            card_axis(),
            card_dot(),
            card_halo(),
            card_glyph("R", CARD_PX - 20.0, CARD_CENTRE_PX - 10.0),
            card_glyph("L", 8.0, CARD_CENTRE_PX - 10.0),
            card_glyph("T", CARD_CENTRE_PX - 5.0, 6.0),
            card_glyph("D", CARD_CENTRE_PX - 5.0, CARD_PX - 24.0),
            (
                CardValue,
                Text::new(""),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.80, 0.80, 0.75)),
                part(10.0, CARD_PX - 22.0, CARD_PX - 20.0, 16.0),
            ),
        ],
    ));
}

/// Sync the card: the dot at `(a, b)` scaled by the ring, red past the envelope, the spin axis
/// rotated to the angular velocity the strike imparts, and the numeric line.
fn sync_card(
    cue: Res<Cue>,
    mut dots: Query<&mut Node, (With<CardDot>, Without<CardHalo>)>,
    mut halos: Query<&mut Node, (With<CardHalo>, Without<CardDot>)>,
    mut dot_colors: Query<&mut BackgroundColor, With<CardDot>>,
    mut axes: Query<(&mut UiTransform, &mut Visibility), With<CardAxis>>,
    mut values: Query<(&mut Text, &mut TextColor), With<CardValue>>,
) {
    let (a, b) = (cue.spin[0], cue.spin[1]);
    let x = CARD_CENTRE_PX + a * CARD_RING_PX;
    let y = CARD_CENTRE_PX - b * CARD_RING_PX;
    let ink = if cue.legal() { Color::srgb(0.05, 0.05, 0.06) } else { C_REFUSED };
    for mut node in &mut dots {
        node.left = px(x - CARD_DOT_R);
        node.top = px(y - CARD_DOT_R);
    }
    for mut node in &mut halos {
        node.left = px(x - CARD_DOT_R - 3.0);
        node.top = px(y - CARD_DOT_R - 3.0);
    }
    for mut color in &mut dot_colors {
        color.0 = ink;
    }
    let spin = spin_axis(&cue);
    let visible = (a * a + b * b).sqrt() > 0.02;
    for (mut transform, mut visibility) in &mut axes {
        *visibility = if visible { Visibility::Inherited } else { Visibility::Hidden };
        // The card's screen y runs down: the axis' `b` component is negated for the rotation.
        transform.rotation = Rot2::radians((-spin.y).atan2(spin.x));
    }
    let text = format!(
        "tip {:.2} R  |  offset {:.1} mm / {:.1} mm",
        cue.spin_norm(),
        cue.offset_mm(),
        envelope_mm(),
    );
    for (mut value, mut color) in &mut values {
        if value.0 != text {
            value.0.clone_from(&text);
        }
        color.0 = if cue.legal() {
            Color::srgb(0.80, 0.80, 0.75)
        } else {
            C_REFUSED
        };
    }
}

/// The spin axis (`ux-cue.md` §3): the direction of the angular velocity the strike imparts,
/// `contact × cue axis`, projected onto the card's frame — which is face-on, so the projection is
/// the plain `(a, b)` pair of components.
fn spin_axis(cue: &Cue) -> Vec2 {
    let aim = Vec3::new(cue.aim[0], cue.aim[1], 0.0);
    let elevation = cue.elevation_rad;
    let (sin_e, cos_e) = elevation.sin_cos();
    // The cue's push direction: raised, it tips down into the cloth.
    let axis = Vec3::new(aim.x * cos_e, aim.y * cos_e, -sin_e);
    // The in-plane axes of §10.3: `a` is the shooter's right, `b` is world up carried along with the
    // cue as it stands up.
    let side = Vec3::new(aim.y, -aim.x, 0.0);
    let up = side.cross(axis);
    let offset = side * cue.spin[0] + up * cue.spin[1];
    let radius = c::BALL_RADIUS_MM as f32;
    let contact = offset * (radius / (radius * radius).sqrt());
    let spin = contact.cross(axis);
    Vec2::new(spin.dot(side), spin.dot(up)).normalize_or_zero()
}

// ---------------------------------------------------------------------------
// Tests: the two spec numbers this module carries

#[cfg(test)]
mod tests {
    use super::*;

    /// `ux-cue.md` §2/§10.7: the 0–1500 mm/s band occupies at least 40 % of the pull range.
    #[test]
    fn the_soft_band_occupies_its_share_of_the_pull() {
        /// The share of the pull range §2/§10.7 requires for the soft band.
        const SOFT_BAND_SHARE: f32 = 0.40;
        let soft = pull_for_speed(SOFT_BAND_MM_S) / MAX_PULL_MM;
        assert!(
            soft >= SOFT_BAND_SHARE,
            "the soft band ends at {:.0} % of the pull range, spec wants at least {:.0} %",
            soft * 100.0,
            SOFT_BAND_SHARE * 100.0
        );
        // And the map is the convex one: full pull is the map's top, half the pull a quarter of it.
        assert_eq!(speed_from_pull(MAX_PULL_MM), MAX_SPEED_MM_S);
        assert!((speed_from_pull(MAX_PULL_MM * 0.5) - MAX_SPEED_MM_S * 0.25).abs() < 1e-3);
        assert_eq!(speed_from_pull(0.0), 0.0);
    }

    /// `ux-cue.md` §6/§10.4: the limit test is `|offset_mm| ≤ ρ_max + 1e-3` — a declaration authored
    /// exactly at the limit is legal, and one past it is refused.
    #[test]
    fn the_envelope_admits_its_limit_and_refuses_past_it() {
        let envelope = envelope_mm();
        // Exactly at the limit: 1.0 envelope fractions.
        let at_limit = Cue {
            spin: [1.0, 0.0],
            ..Cue::default()
        };
        assert!(at_limit.legal(), "exactly at the limit is legal");
        let past = (1.0 + 2.0 * ENVELOPE_TOLERANCE_MM / envelope) as f32;
        let refused = Cue {
            spin: [past, 0.0],
            ..Cue::default()
        };
        assert!(!refused.legal(), "past the tolerance is refused");
        assert!(refused.overage_mm() > 0.0);
    }

    /// `ux-cue.md` §5: the guides follow the first ball on the path, and the ghost ball sits one
    /// diameter from the object ball's centre.
    #[test]
    fn the_guide_finds_the_first_ball_on_the_path() {
        let mut states = [BallState::at_rest([0.0, 0.0, c::BALL_RADIUS_MM]); 16];
        for state in &mut states {
            state.pocketed = true;
        }
        states[1] = BallState::at_rest([500.0, 0.0, c::BALL_RADIUS_MM]);
        states[2] = BallState::at_rest([900.0, 0.0, c::BALL_RADIUS_MM]);
        let guide = aim_guide(&states[1..], Vec2::ZERO, Vec2::X);
        let Guide::Ball { ball, ghost, travel, .. } = guide else {
            panic!("the path meets ball 1");
        };
        assert_eq!(ball, 1);
        assert!((ghost.x - (500.0 - 2.0 * c::BALL_RADIUS_MM as f32)).abs() < 1e-3);
        assert!((travel - Vec2::X).length() < 1e-6);
    }

    /// The suggested call follows the first ball of the guide and always offers the safety.
    #[test]
    fn the_suggested_calls_lead_with_the_aligned_pocket() {
        let guide = Guide::Ball {
            ball: 3,
            target: Vec2::new(0.0, 0.0),
            ghost: Vec2::new(-57.15, 0.0),
            travel: Vec2::X,
            tangent: Vec2::Y,
            cut_deg: 0.0,
        };
        let calls = suggested_calls(&guide);
        assert!(matches!(calls.last(), Some(Call::Safety)));
        match &calls[0] {
            Call::Ball { ball, pocket } => {
                assert_eq!(ball.0, 3);
                assert_eq!(pocket, "foot_right");
            }
            other => panic!("the first suggestion is a ball call, not {other:?}"),
        }
    }
}
