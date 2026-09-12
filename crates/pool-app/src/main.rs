//! The `pool` binary: the playable shell of `architecture.md` §10.
//!
//! The shell's route into game state is a logged input through `Session` (§10) and nothing else: the
//! cue input machine of `input.rs` emits a declaration, `session.rs` hands it to the drive loop,
//! `playback.rs` presents the computed shot exactly, and the HUD reads records. The ECS stops at this
//! crate — `bevy` is a dependency of `pool-app` alone (§2) — and no `FixedUpdate` exists anywhere
//! (§4): the systems of §10's module table run in `Update` as functions of session state and the
//! presentation clock.
//!
//! Two modes: interactive play (`cargo run -p pool-app`), and `--screenshot <dir>`, which drives the
//! same session into a named presentation state, holds it still for `--frames` frames, captures the
//! window, and exits.

mod bridge;
mod input;
mod playback;
mod render;
mod session;
mod ui;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{PresentMode, WindowResolution};
use clap::Parser;
use pool_match::{Difficulty, DifficultyLevel, MatchConfig, RACE_TARGET, Request};
use pool_rules::Vec2;
use std::path::PathBuf;

use bridge::BallStates;
use input::Cue;
use playback::Playback;
use session::{Awaiting, Game};

/// The capture window's size in physical pixels. Fixed, one physical pixel per logical pixel: the
/// screenshots are the evidence, so they are the same size every run.
const WINDOW_W: u32 = 1920;
const WINDOW_H: u32 = 1080;

/// The clear colour: the room around the table.
const ROOM: Color = Color::srgb(0.07, 0.08, 0.09);

/// The shell's system sets, in the order one frame needs (`architecture.md` §10's modules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum ShellSet {
    /// `playback.rs`: the presentation clock and the ball states it writes.
    Playback,
    /// `session.rs` and `input.rs`: one logged input at most, then the authored declaration.
    Input,
    /// `bridge.rs` and `input.rs`'s gizmos: the ECS transforms and the cloth's drawing.
    Draw,
    /// `ui.rs`: the HUD, and the screenshot walk.
    Hud,
}

// ---------------------------------------------------------------------------
// CLI (`architecture.md` §10, plus the screenshot mode)

/// The shell's CLI.
#[derive(Debug, Parser, Resource)]
#[command(
    name = "pool",
    about = "The playable pool shell of architecture.md §10"
)]
struct Args {
    /// The profile record the match runs under (`config/profiles/<name>.json`, §12).
    #[arg(long, value_name = "name", default_value = "default")]
    profile: String,
    /// The policy artifact for the AI seat (`assets/policies/*.onnx`, §12).
    ///
    /// Parsed for §10's CLI; the `ai_host` slice (M4) is what loads it.
    #[arg(long, value_name = "path")]
    policy: Option<PathBuf>,
    /// The policy seat's difficulty (`ai.md` §7's four levels).
    ///
    /// Recorded in the input log's header. It is inert while both seats are human (§5: execution
    /// noise applies to `from_policy` declarations only), which is every seat this slice can seat.
    #[arg(long, value_name = "level", default_value = "pro", value_parser = parse_difficulty)]
    difficulty: DifficultyLevel,
    /// The rack-construction seed: rack *i*'s seed derives from it (`rules-break.md` §2.7).
    #[arg(long, value_name = "u64", default_value_t = 0)]
    match_seed: u64,
    /// Racks needed to win the match (`rules.md` §8; `1` plays a single rack).
    #[arg(long, value_name = "n", default_value_t = RACE_TARGET)]
    race_target: u32,
    /// Draw the development panel: offsets in mm and envelope fractions, envelope arithmetic, guide
    /// geometry, the declaration JSON (`ux-cue.md` §8).
    #[arg(long)]
    debug_candidates: bool,
    /// Write `<dir>/<state>.png` for every captured state and exit, instead of opening the window.
    #[arg(long, value_name = "dir")]
    screenshot: Option<PathBuf>,
    /// Which presentation state to capture.
    #[arg(long, default_value = "play", value_name = "name|all", value_parser = Screen::parse)]
    state: Screen,
    /// Frames to render before each capture, so the window has settled.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..))]
    frames: u32,
}

/// The presentation states the screenshot walk captures: the play surface's own states, each driven
/// through the real session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// The rack's start: the cue ball in hand above the head string (`rules.md` §6).
    BallInHand,
    /// The declaration being authored: card, gauge, guides, and the 3-line HUD (`ux-cue.md` §8).
    Play,
    /// Draw and right english authored on the card (`ux-cue.md` §3).
    Spin,
    /// An elevated cue at 45°: the gauge and its tick scale (`ux-cue.md` §4/§10.1).
    Elevation,
    /// An offset past the miscue envelope: the refusal state (`ux-cue.md` §6).
    Refused,
    /// A shot being presented: `state_at(t)` mid-flight (`architecture.md` §10's `playback.rs`).
    InFlight,
    /// A break that fouled: the incoming player's choice tree on screen (`rules-break.md` §3).
    FoulChoice,
    /// Every state, in list order.
    All,
}

impl Screen {
    /// The walk's states, in list order: the rack's start first, the shot last.
    const LIST: [Self; 7] = [
        Self::BallInHand,
        Self::Play,
        Self::Spin,
        Self::Elevation,
        Self::Refused,
        Self::InFlight,
        Self::FoulChoice,
    ];

    /// The state's name, as the flag and the PNG file read.
    const fn name(self) -> &'static str {
        match self {
            Self::BallInHand => "ball-in-hand",
            Self::Play => "play",
            Self::Spin => "spin",
            Self::Elevation => "elevation",
            Self::Refused => "refused",
            Self::InFlight => "in-flight",
            Self::FoulChoice => "foul-choice",
            Self::All => "all",
        }
    }

    /// The states this selection captures, in order.
    fn screens(self) -> Vec<Self> {
        match self {
            Self::All => Self::LIST.to_vec(),
            one => vec![one],
        }
    }

    /// Parse `--state`.
    fn parse(value: &str) -> Result<Self, String> {
        if value == "all" {
            return Ok(Self::All);
        }
        let states = [
            Self::BallInHand,
            Self::Play,
            Self::Spin,
            Self::Elevation,
            Self::Refused,
            Self::InFlight,
            Self::FoulChoice,
        ];
        states
            .into_iter()
            .find(|state| state.name() == value)
            .ok_or_else(|| {
                let known: Vec<&str> = states.iter().map(|state| state.name()).collect();
                format!("unknown state {value:?}; known: {}, all", known.join(", "))
            })
    }
}

/// Parse `--difficulty` (`ai.md` §7's four levels).
fn parse_difficulty(value: &str) -> Result<DifficultyLevel, String> {
    match value {
        "beginner" => Ok(DifficultyLevel::Beginner),
        "intermediate" => Ok(DifficultyLevel::Intermediate),
        "advanced" => Ok(DifficultyLevel::Advanced),
        "pro" => Ok(DifficultyLevel::Pro),
        other => Err(format!(
            "unknown difficulty {other:?}; known: beginner, intermediate, advanced, pro"
        )),
    }
}

// ---------------------------------------------------------------------------
// The screenshot walk

/// `--screenshot`'s walk: the states to capture, the one on screen, and the settle countdown.
#[derive(Resource)]
struct Capture {
    /// The states to capture, in order.
    screens: Vec<Screen>,
    /// The state on screen: an index into `screens`.
    pos: usize,
    /// Frames to render before each capture.
    frames: u32,
    /// The countdown to the next capture.
    settle: u32,
    /// Whether the first state has been driven into the session.
    applied: bool,
    /// Where the PNGs go: `<dir>/<state>.png`.
    dir: PathBuf,
    /// True while a capture is on its way to disk.
    waiting: bool,
}

/// Set by the screenshot observer once a capture has been written.
#[derive(Resource, Default)]
struct Captured(bool);

/// The state the screenshot walk drives: the session, the authored declaration, the presentation
/// clock, and the bridge's mirror.
#[derive(bevy::ecs::system::SystemParam)]
struct Walk<'w> {
    game: ResMut<'w, Game>,
    cue: ResMut<'w, Cue>,
    playback: ResMut<'w, Playback>,
    states: ResMut<'w, BallStates>,
}

// ---------------------------------------------------------------------------
// main

fn main() {
    let args = Args::parse();
    if args.screenshot.is_none() && args.state == Screen::All {
        eprintln!(
            "[pool] --state all is the screenshot walk; pass --screenshot <dir>, or name one state"
        );
        std::process::exit(2);
    }
    let profile = match load_profile(&args.profile) {
        Ok(profile) => profile,
        Err(message) => {
            eprintln!("[pool] {message}");
            std::process::exit(2);
        }
    };
    let config = MatchConfig {
        profile,
        match_seed: args.match_seed,
        // Inert while both seats are human (§5: one draw per `from_policy` declaration).
        noise_seed: 0,
        difficulty: Difficulty {
            level: args.difficulty,
            // No policy artifact is served in this slice; the header says so instead of naming a
            // file nothing loaded (M4's `ai_host` fills this in).
            checkpoint: "none".to_string(),
        },
        race_target: args.race_target,
    };

    let capture = args.screenshot.as_ref().map(|dir| {
        // `save_to_disk` does not create directories: make the output directory first.
        if let Err(error) = std::fs::create_dir_all(dir) {
            eprintln!("[pool] cannot create {}: {error}", dir.display());
            std::process::exit(2);
        }
        let screens = args.state.screens();
        println!(
            "[pool] screenshot mode: {} state(s) -> {}",
            screens.len(),
            dir.display()
        );
        Capture {
            screens,
            pos: 0,
            frames: args.frames,
            settle: 0,
            applied: false,
            dir: dir.clone(),
            waiting: false,
        }
    });

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "pool".to_string(),
            resolution: WindowResolution::new(WINDOW_W, WINDOW_H).with_scale_factor_override(1.0),
            resizable: false,
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    }));
    app.insert_resource(ClearColor(ROOM));
    app.insert_resource(Game::new(config));
    app.insert_resource(args);
    app.configure_sets(
        Update,
        (
            ShellSet::Playback,
            ShellSet::Input,
            ShellSet::Draw,
            ShellSet::Hud,
        )
            .chain(),
    );
    app.add_systems(
        Startup,
        (report_pending_flags, install_profile, setup).chain(),
    );
    app.add_systems(Update, bridge::sync_ball_transforms.in_set(ShellSet::Draw));
    playback::systems(&mut app);
    input::systems(&mut app);
    ui::systems(&mut app);
    if let Some(capture) = capture {
        app.insert_resource(capture);
        app.init_resource::<Captured>();
        app.add_systems(Update, capture_driver.in_set(ShellSet::Hud));
    }
    app.run();
}

/// Load `config/profiles/<name>.json` (`architecture.md` §12: the binaries do the file I/O).
fn load_profile(name: &str) -> Result<pool_sim::Profile, String> {
    let path = PathBuf::from("config/profiles").join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run the shell from the repository root)",
            path.display()
        )
    })?;
    let profile: pool_sim::Profile = serde_json::from_str(&text)
        .map_err(|error| format!("{} does not parse: {error}", path.display()))?;
    if profile.id != name {
        return Err(format!(
            "{} holds profile {:?}, but {name:?} was asked for",
            path.display(),
            profile.id
        ));
    }
    Ok(profile)
}

// ---------------------------------------------------------------------------
// startup

/// Hand the profile's constants to the shell's display math: the squirt-corrected preview needs the
/// pivot length (`ux-cue.md` §10.8), and it is the profile's, not the shell's, number.
fn install_profile(game: Res<Game>, mut cue: ResMut<Cue>) {
    cue.set_pivot(game.config().profile.pivot_mm);
}

/// Build the scene: the camera, the table, and the balls at the session's opening position.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    game: Res<Game>,
) {
    let aspect = WINDOW_W as f32 / WINDOW_H as f32;
    commands.spawn((
        Camera2d,
        IsDefaultUiCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: render::viewport_height(aspect),
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    render::spawn_table(&mut commands, &mut meshes, &mut materials);
    bridge::spawn_balls(&mut commands, &mut meshes, &mut materials, game.positions());
}

/// Report the flags this slice parses but cannot act on: each is named with the slice that consumes
/// it, so a run never looks like it honoured a flag it ignored.
fn report_pending_flags(args: Res<Args>) {
    if args.policy.is_some() {
        println!("[pool] --policy is parsed but unused until the ai_host slice lands (M4)");
    }
}

// ---------------------------------------------------------------------------
// the screenshot walk

/// Walk the state list: drive the session into the state, settle, ask for a screenshot, wait for it,
/// advance; exit when done.
fn capture_driver(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    mut captured: ResMut<Captured>,
    mut walk: Walk,
    mut exit: MessageWriter<AppExit>,
) {
    if !capture.applied {
        capture.applied = true;
        apply_screen(
            capture.screens[capture.pos],
            &mut walk.game,
            &mut walk.cue,
            &mut walk.playback,
            &mut walk.states,
        );
        capture.settle = capture.frames;
        return;
    }
    if capture.waiting {
        if !captured.0 {
            return; // the file is still on its way
        }
        captured.0 = false;
        capture.waiting = false;
        capture.pos += 1;
        if capture.pos >= capture.screens.len() {
            println!(
                "[pool] captured {} state(s) -> {}",
                capture.screens.len(),
                capture.dir.display()
            );
            exit.write(AppExit::Success);
            return;
        }
        apply_screen(
            capture.screens[capture.pos],
            &mut walk.game,
            &mut walk.cue,
            &mut walk.playback,
            &mut walk.states,
        );
        capture.settle = capture.frames;
        return;
    }
    if capture.settle > 0 {
        capture.settle -= 1;
        return;
    }
    let screen = capture.screens[capture.pos];
    let path = capture.dir.join(format!("{}.png", screen.name()));
    println!("[pool] capture {} -> {}", screen.name(), path.display());
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| captured.0 = true);
    capture.waiting = true;
}

/// Drive the session into a presentation state.
///
/// Every state after the rack's start begins from a fresh session, so the walk is order-independent:
/// the cue ball is placed through a real `Request::Placement` and the shot states author a
/// declaration the same way — the inputs a human's clicks produce, and the only route into the
/// session there is. Nothing here reaches past the loop.
fn apply_screen(
    screen: Screen,
    game: &mut Game,
    cue: &mut Cue,
    playback: &mut Playback,
    states: &mut BallStates,
) {
    if screen == Screen::BallInHand || screen == Screen::All {
        return;
    }
    let config = game.config().clone();
    *game = Game::new(config);
    *playback = Playback::default();
    cue.placement = None;

    if let Awaiting::Placement { domain, .. } = game.awaiting() {
        let parked = game.positions()[0].pos_mm;
        let _ = game.request(Request::Placement {
            domain,
            pos: Vec2 {
                x: parked[0],
                y: parked[1],
            },
        });
    }
    let cue_ball = Vec2 {
        x: game.positions()[0].pos_mm[0],
        y: game.positions()[0].pos_mm[1],
    };
    // The rack's apex: the object ball nearest the head side (`rules-break.md` §2's apex sits on the
    // foot spot and the rows stand behind it).
    let apex = game
        .positions()
        .iter()
        .skip(1)
        .filter(|state| state.in_play())
        .map(|state| Vec2 {
            x: state.pos_mm[0],
            y: state.pos_mm[1],
        })
        .reduce(|a, b| if a.x <= b.x { a } else { b })
        .unwrap_or(Vec2 { x: 0.0, y: 0.0 });
    cue.set_aim_towards(cue_ball, apex);
    cue.set_pull(240.0);
    cue.set_spin([0.0, 0.0]);
    cue.set_elevation(0.0);
    // Every scripted state starts from rack 0, so its shot is the break (`rules.md` §1).
    cue.call = pool_rules::Call::Break;
    match screen {
        Screen::Spin => cue.set_spin([0.50, -0.42]),
        Screen::Elevation => cue.set_elevation(45.0_f32.to_radians()),
        // Past the envelope: 1.082 envelope fractions is 1.082 · 14.70 mm, an 8 % overage.
        Screen::Refused => cue.set_spin([0.90, 0.60]),
        // A break struck away from the rack and into the head corner: nothing is pocketed and no
        // object ball reaches a rail, so the break is illegal and the incoming player's tree is up.
        Screen::FoulChoice => {
            cue.set_pull(320.0);
            cue.set_aim_towards(
                cue_ball,
                Vec2 {
                    x: -pool_sim::constants::HALF_LEN_MM,
                    y: pool_sim::constants::HALF_WIDTH_MM,
                },
            );
        }
        Screen::Play => {}
        Screen::InFlight => cue.set_pull(320.0),
        Screen::BallInHand | Screen::All => return,
    }
    let strike = matches!(screen, Screen::InFlight | Screen::FoulChoice);
    if strike {
        let declaration = cue.declaration();
        if game.request(Request::Declaration(declaration)).is_ok()
            && let Some(shot) = game.last_shot()
        {
            playback.hold(shot.clone(), 0.45);
        }
    }
    states.0 = *game.positions();
}
