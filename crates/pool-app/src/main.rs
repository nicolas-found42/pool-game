//! The `pool` binary: the Bevy shell of `architecture.md` §10.
//!
//! This slice is the shell's **render slice**: the window, the top-down camera, the table, the rack
//! at rest, and the `--screenshot` mode the acceptance evidence comes from. `session.rs`, `input.rs`,
//! `playback.rs`, `ai_host.rs`, `ui.rs`, and `debug.rs` are the later slices of §10's module table:
//! this slice neither stubs them nor consumes their flags, it parses the flags for the CLI contract
//! and reports them unused.
//!
//! The ECS stops at this crate — `bevy` is a dependency of `pool-app` alone (§2), and no ECS type
//! reaches below it. Nothing here writes game state either: the shell's route in is a logged input
//! through `Session` (§10), which this slice does not have yet.

mod bridge;
mod render;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{PresentMode, WindowResolution};
use clap::Parser;
use std::path::PathBuf;

use bridge::BallStates;

/// The capture window's size in physical pixels. Fixed, one physical pixel per logical pixel: the
/// screenshots are the evidence, so they are the same size every run.
const WINDOW_W: u32 = 1920;
const WINDOW_H: u32 = 1080;

/// The clear colour: the room around the table.
const ROOM: Color = Color::srgb(0.07, 0.08, 0.09);

/// The frozen fixture seed list's size (`rules-break.md` §2.9): seeds 0–15.
const FIXTURE_SEEDS: u64 = 16;

// ---------------------------------------------------------------------------
// CLI (`architecture.md` §10, plus the screenshot mode)

/// The shell's CLI.
#[derive(Debug, Parser, Resource)]
#[command(name = "pool", about = "The pool-game shell of architecture.md §10")]
struct Args {
    /// The profile record the match runs under (`config/profiles/<name>.json`, §12).
    ///
    /// Parsed for §10's CLI; the session slice is what reads it.
    #[arg(long, value_name = "name")]
    profile: Option<String>,
    /// The policy artifact for the AI seat (`assets/policies/*.onnx`, §12).
    ///
    /// Parsed for §10's CLI; the `ai_host` slice loads it.
    #[arg(long, value_name = "path")]
    policy: Option<PathBuf>,
    /// The policy seat's difficulty level (`ai.md` §7).
    ///
    /// Parsed for §10's CLI; the session slice picks the checkpoint with it.
    #[arg(long, value_name = "level")]
    difficulty: Option<String>,
    /// Draw the candidate-set/per-candidate-score overlay (`ai.md` §8).
    ///
    /// Parsed for §10's CLI; the debug slice draws it.
    #[arg(long)]
    debug_candidates: bool,
    /// Write `<dir>/<state>.png` for every captured state and exit, instead of opening the window.
    #[arg(long, value_name = "dir")]
    screenshot: Option<PathBuf>,
    /// Which state to show: a state name, or `all` to walk the whole list.
    #[arg(long, default_value = "rack-0", value_name = "name|all", value_parser = parse_screen)]
    state: Screen,
    /// Frames to render before each capture, so the window has settled.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..))]
    frames: u32,
}

/// `--state`: which state the shell shows.
///
/// The render slice's state list is one state per frozen rack fixture seed (`rules-break.md` §2.9,
/// `rack-fixtures.json`'s 0–15). Later slices append the states their surface renders; the list is
/// this flag's vocabulary and stays fixed for a given build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    /// The rack at rest for one fixture seed.
    Rack(u64),
    /// Every state, in list order: the screenshot walk.
    All,
}

impl Screen {
    /// The seeds this selection covers, in list order.
    fn seeds(self) -> Vec<u64> {
        match self {
            Self::Rack(seed) => vec![seed],
            Self::All => (0..FIXTURE_SEEDS).collect(),
        }
    }

    /// The seed the window opens on.
    fn first_seed(self) -> u64 {
        match self {
            Self::Rack(seed) => seed,
            Self::All => 0,
        }
    }
}

/// A state's name: the fixture seed it draws.
fn state_name(seed: u64) -> String {
    format!("rack-{seed}")
}

/// Parse `--state`.
fn parse_screen(value: &str) -> Result<Screen, String> {
    if value == "all" {
        return Ok(Screen::All);
    }
    if let Some(seed) = value
        .strip_prefix("rack-")
        .and_then(|number| number.parse::<u64>().ok())
        && seed < FIXTURE_SEEDS
    {
        return Ok(Screen::Rack(seed));
    }
    let known: Vec<String> = (0..FIXTURE_SEEDS).map(state_name).collect();
    Err(format!(
        "unknown state {value:?}; known: all, {}",
        known.join(", ")
    ))
}

// ---------------------------------------------------------------------------
// The screenshot walk

/// `--screenshot`'s walk: the states to capture, the one on screen, and the settle countdown.
#[derive(Resource)]
struct Capture {
    /// The seeds to capture, in order.
    seeds: Vec<u64>,
    /// The state on screen: an index into `seeds`.
    pos: usize,
    /// Frames to render before each capture.
    frames: u32,
    /// The countdown to the next capture.
    settle: u32,
    /// Where the PNGs go: `<dir>/<state>.png`.
    dir: PathBuf,
    /// True while a capture is on its way to disk.
    waiting: bool,
}

/// Set by the screenshot observer once a capture has been written.
#[derive(Resource, Default)]
struct Captured(bool);

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

    let capture = args.screenshot.as_ref().map(|dir| {
        // `save_to_disk` does not create directories: make the output directory first.
        if let Err(error) = std::fs::create_dir_all(dir) {
            eprintln!("[pool] cannot create {}: {error}", dir.display());
            std::process::exit(2);
        }
        let seeds = args.state.seeds();
        println!(
            "[pool] screenshot mode: {} state(s) -> {}",
            seeds.len(),
            dir.display()
        );
        Capture {
            seeds,
            pos: 0,
            frames: args.frames,
            settle: args.frames,
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
    app.insert_resource(args);
    app.add_systems(Startup, (report_pending_flags, setup));
    app.add_systems(Update, bridge::sync_ball_transforms);
    if let Some(capture) = capture {
        app.insert_resource(capture);
        app.init_resource::<Captured>();
        app.add_systems(Update, capture_driver);
    }
    app.run();
}

// ---------------------------------------------------------------------------
// startup

/// Build the scene: the camera, the table, and the rack the selected state asks for.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    args: Res<Args>,
) {
    let aspect = WINDOW_W as f32 / WINDOW_H as f32;
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: render::viewport_height(aspect),
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    render::spawn_table(&mut commands, &mut meshes, &mut materials);
    let states = render::rack_states(args.state.first_seed());
    bridge::spawn_balls(&mut commands, &mut meshes, &mut materials, &states);
}

/// Report the flags this slice parses but cannot act on: each is named with the slice that consumes
/// it, so a run never looks like it honoured a flag it ignored.
fn report_pending_flags(args: Res<Args>) {
    let pending = [
        ("--profile", args.profile.is_some(), "session"),
        ("--policy", args.policy.is_some(), "ai_host"),
        ("--difficulty", args.difficulty.is_some(), "session"),
        ("--debug-candidates", args.debug_candidates, "debug"),
    ];
    for (flag, given, slice) in pending {
        if given {
            println!("[pool] {flag} is parsed but unused until the {slice} slice lands");
        }
    }
}

// ---------------------------------------------------------------------------
// the screenshot walk

/// Walk the state list: settle, ask for a screenshot, wait for it, advance; exit when done.
fn capture_driver(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    mut states: ResMut<BallStates>,
    mut captured: ResMut<Captured>,
    mut exit: MessageWriter<AppExit>,
) {
    if capture.waiting {
        if !captured.0 {
            return; // the file is still on its way
        }
        captured.0 = false;
        capture.waiting = false;
        capture.pos += 1;
        if capture.pos >= capture.seeds.len() {
            println!(
                "[pool] captured {} state(s) -> {}",
                capture.seeds.len(),
                capture.dir.display()
            );
            exit.write(AppExit::Success);
            return;
        }
        states.0 = render::rack_states(capture.seeds[capture.pos]);
        capture.settle = capture.frames;
        return;
    }
    if capture.settle > 0 {
        capture.settle -= 1;
        return;
    }
    let seed = capture.seeds[capture.pos];
    let name = state_name(seed);
    let path = capture.dir.join(format!("{name}.png"));
    println!("[pool] capture {name} -> {}", path.display());
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| captured.0 = true);
    capture.waiting = true;
}
