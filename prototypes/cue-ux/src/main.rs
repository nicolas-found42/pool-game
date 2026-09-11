//! Cue-UX prototype for wayfinder ticket #10 - THROWAWAY, not game code.
//!
//! Top-down table, drag-back cue for power, elevation control, a 3D cue-ball strike-point
//! marker rendered into a UI card, aim guides, and a read-out of the strike declaration
//! being authored. No physics: the prototype exists to judge how the shot-authoring flow
//! feels and how spin/masse/elevation read in a top-down view.
//!
//! Interactive:  cargo run
//! Screenshots:  cargo run -- --screenshot shots --state all --frames 30

mod shot;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::window::{PresentMode, WindowResolution};
use std::path::PathBuf;

use shot::*;

// ---------------------------------------------------------------------------
// CLI

#[derive(Resource, Clone, Debug)]
struct Args {
    screenshot: Option<PathBuf>,
    state: String,
    frames: u32,
}

impl Args {
    fn parse() -> Self {
        let mut out = Args {
            screenshot: None,
            state: "all".to_string(),
            frames: 30,
        };
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--screenshot" => out.screenshot = it.next().map(PathBuf::from),
                "--state" => out.state = it.next().unwrap_or_else(|| "all".into()),
                "--frames" => {
                    out.frames = it.next().and_then(|v| v.parse().ok()).unwrap_or(30)
                }
                other => eprintln!("[cue-ux] ignoring unknown argument {other:?}"),
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// State

#[derive(Resource)]
struct ShotUi {
    decl: Declaration,
    /// Drag-back distance in mm (power being authored).
    pull_mm: f32,
    /// Label of the state on screen (scripted preset name, or free play).
    label: String,
    /// Last committed declaration, shown after release/Enter.
    committed: Option<String>,
    /// Decays after a commit, so the read-out can flash.
    flash: f32,
    power_drag: bool,
    spin_drag: bool,
    /// Index of the last scripted state loaded (Tab cycles from here).
    preset_idx: usize,
}

impl ShotUi {
    fn speed_from_pull(pull_mm: f32) -> f32 {
        (pull_mm / MAX_DRAG).clamp(0.0, 1.0) * MAX_SPEED
    }
}

#[derive(Resource, Default)]
struct CaptureFlags {
    saved: bool,
}

#[derive(Resource)]
struct Script {
    /// Indices into `presets()` to walk, in order.
    indices: Vec<usize>,
    pos: usize,
    frames: u32,
    settle: u32,
    out: PathBuf,
    dir_mode: bool,
}

impl Script {
    fn path_for(&self, preset_idx: usize) -> PathBuf {
        let name = presets()[preset_idx].file;
        if self.dir_mode {
            self.out.join(format!("{name}.png"))
        } else {
            self.out.clone()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Settle,
    Capturing,
}

#[derive(Resource)]
struct ScriptPhase(Phase);

// ---------------------------------------------------------------------------
// Components

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum WPart {
    Camera,
    Ball,
    Ring,
    Marker,
    MarkerHalo,
    Shaft,
    ShaftTip,
    SpinBar,
    SpinCone,
    CrossBar,
}

#[derive(Component)]
struct MarkerMat;

#[derive(Component)]
struct ReadoutText;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct WidgetCard;

#[derive(Component)]
struct StickBody;

#[derive(Component)]
struct StickTip;

#[derive(Component)]
struct StickButt;

#[derive(Component)]
struct WedgeLabel;

#[derive(Component)]
struct PullLabel;

// ---------------------------------------------------------------------------
// main

fn main() {
    let args = Args::parse();
    let scripted = args.screenshot.is_some();

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cue-ux prototype - ticket #10 (throwaway)".into(),
            // 1 physical pixel per logical pixel: the PNGs are 1600x920 and text stays crisp
            resolution: WindowResolution::new(1600, 920).with_scale_factor_override(1.0),
            resizable: false,
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    }));

    // The declaration each mode starts from.
    let presets = presets();
    let script_start = if scripted {
        if args.state == "all" {
            0
        } else {
            match presets.iter().position(|p| p.file == args.state) {
                Some(i) => i,
                None => {
                    eprintln!(
                        "[cue-ux] unknown --state {:?}; valid names: {}",
                        args.state,
                        presets
                            .iter()
                            .map(|p| p.file)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    std::process::exit(2);
                }
            }
        }
    } else {
        0
    };
    let start = if scripted {
        presets[script_start].decl
    } else {
        Declaration::default()
    };
    let label = if scripted {
        presets[script_start].label.to_string()
    } else {
        "free play (mouse + keys)".to_string()
    };

    app.insert_resource(ShotUi {
        decl: start,
        pull_mm: if scripted {
            presets[script_start].pull_mm
        } else {
            0.0
        },
        label,
        committed: None,
        flash: 0.0,
        power_drag: false,
        spin_drag: false,
        preset_idx: script_start,
    });
    app.init_resource::<CaptureFlags>();

    if scripted {
        let path = args.screenshot.clone().unwrap();
        let indices: Vec<usize> = if args.state == "all" {
            (0..presets.len()).collect()
        } else {
            vec![script_start]
        };
        app.insert_resource(Script {
            indices,
            pos: 0,
            frames: args.frames,
            settle: args.frames,
            out: path.clone(),
            dir_mode: args.state == "all",
        });
        app.insert_resource(ScriptPhase(Phase::Settle));
        println!(
            "[cue-ux] scripted mode: {} state(s) -> {}",
            if args.state == "all" {
                presets.len().to_string()
            } else {
                "1".to_string()
            },
            path.display()
        );
    }
    app.insert_resource(args);

    app.add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                decay_flash,
                interactive_input.run_if(not(resource_exists::<Script>)),
                script_driver.run_if(resource_exists::<Script>),
                sync_widget,
                sync_stick,
                sync_wedge_label,
                draw_guides,
                update_hud,
            )
                .chain(),
        );
    app.run();
}

// ---------------------------------------------------------------------------
// setup

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut m2: ResMut<Assets<ColorMaterial>>,
    mut m3: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {

    // ---- top-down 2D scene (world units are millimetres) -------------------
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 1700.0,
            },
            ..OrthographicProjection::default_2d()
        }),
        IsDefaultUiCamera,
    ));

    // rails, cloth, pockets
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(
            2.0 * TABLE_HALF_X + 140.0,
            2.0 * TABLE_HALF_Y + 140.0,
        ))),
        MeshMaterial2d(m2.add(Color::srgb(0.26, 0.16, 0.09))),
        Transform::from_xyz(0.0, 0.0, -1.0),
    ));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0 * TABLE_HALF_X, 2.0 * TABLE_HALF_Y))),
        MeshMaterial2d(m2.add(Color::srgb(0.06, 0.33, 0.20))),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    let pocket_mesh = meshes.add(Circle::new(60.0));
    let pocket_mat = m2.add(Color::srgb(0.02, 0.02, 0.03));
    for (px, py) in [
        (-TABLE_HALF_X, -TABLE_HALF_Y),
        (TABLE_HALF_X, -TABLE_HALF_Y),
        (-TABLE_HALF_X, TABLE_HALF_Y),
        (TABLE_HALF_X, TABLE_HALF_Y),
        (0.0, -TABLE_HALF_Y),
        (0.0, TABLE_HALF_Y),
    ] {
        commands.spawn((
            Mesh2d(pocket_mesh.clone()),
            MeshMaterial2d(pocket_mat.clone()),
            Transform::from_xyz(px, py, 0.5),
        ));
    }

    // balls
    let ball_mesh = meshes.add(Circle::new(BALL_R));
    commands.spawn((
        Mesh2d(ball_mesh.clone()),
        MeshMaterial2d(m2.add(Color::srgb(0.97, 0.97, 0.95))),
        Transform::from_xyz(CUE_BALL.x, CUE_BALL.y, 1.0),
    ));
    let obj_colors = [
        Color::srgb(0.95, 0.80, 0.15),
        Color::srgb(0.15, 0.35, 0.85),
        Color::srgb(0.85, 0.15, 0.15),
        Color::srgb(0.55, 0.25, 0.75),
    ];
    for (p, c) in OBJECT_BALLS.iter().zip(obj_colors) {
        commands.spawn((
            Mesh2d(ball_mesh.clone()),
            MeshMaterial2d(m2.add(c)),
            Transform::from_xyz(p.x, p.y, 1.0),
        ));
    }

    // cue stick (a scaled unit rectangle, so it reads as a stick rather than a hairline)
    let unit_rect = meshes.add(Rectangle::new(1.0, 1.0));
    commands.spawn((
        Mesh2d(unit_rect.clone()),
        MeshMaterial2d(m2.add(Color::srgb(0.85, 0.68, 0.42))),
        Transform::from_xyz(0.0, 0.0, 2.0),
        StickBody,
    ));
    commands.spawn((
        Mesh2d(unit_rect.clone()),
        MeshMaterial2d(m2.add(Color::srgb(0.12, 0.12, 0.15))),
        Transform::from_xyz(0.0, 0.0, 2.1),
        StickTip,
    ));
    commands.spawn((
        Mesh2d(unit_rect),
        MeshMaterial2d(m2.add(Color::srgb(0.40, 0.26, 0.13))),
        Transform::from_xyz(0.0, 0.0, 2.0),
        StickButt,
    ));

    // world-space label for the elevation gauge (so the wedge is not mistaken for a guide)
    commands.spawn((
        Text2d::new(""),
        TextFont::from_font_size(34.0),
        TextColor(Color::srgb(1.0, 0.65, 0.25)),
        Transform::from_xyz(0.0, 0.0, 3.0),
        WedgeLabel,
    ));

    // world-space label on the cue gap: reads the drag-back as a distance, not a vibe
    commands.spawn((
        Text2d::new(""),
        TextFont::from_font_size(34.0),
        TextColor(Color::srgb(1.0, 1.0, 1.0)),
        Transform::from_xyz(0.0, 0.0, 3.0),
        PullLabel,
    ));

    // ---- 3D strike-marker widget, rendered to a texture --------------------
    let layer = RenderLayers::layer(1);
    let widget_image = images.add(Image::new_target_texture(
        640,
        640,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.03, 0.04, 0.05)),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 0.45,
            ..default()
        }),
        RenderTarget::Image(widget_image.clone().into()),
        Transform::default(),
        layer.clone(),
        WPart::Camera,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
        layer.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(4000.0, 3000.0))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.07, 0.22, 0.15),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(BALL_R).mesh().ico(6).unwrap())),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.96, 0.96, 0.94),
            perceptual_roughness: 0.3,
            ..default()
        })),
        Transform::from_xyz(0.0, BALL_R, 0.0),
        layer.clone(),
        WPart::Ball,
    ));

    // miscue-envelope ring, sitting on the ball face where the limit lives
    let ring_r = envelope_mm();
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(ring_r - 0.5, ring_r + 0.5))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.55, 0.15),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
        WPart::Ring,
    ));

    // face centre crosshair: the fixed reference the contact dot is read against
    let cross_mesh = meshes.add(Cylinder::new(0.9, 46.0));
    let cross_mat = m3.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.25, 0.28),
        unlit: true,
        ..default()
    });
    for bar in [0.0f32, 1.0] {
        commands.spawn((
            Mesh3d(cross_mesh.clone()),
            MeshMaterial3d(cross_mat.clone()),
            Transform::default(),
            layer.clone(),
            WPart::CrossBar,
            CrossBar(bar),
        ));
    }

    // tip-contact marker: white halo first so the black dot reads on the white ball too
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(9.0, 1.0))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
        WPart::MarkerHalo,
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(7.0, 1.2))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.05, 0.05, 0.06),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
        WPart::Marker,
        MarkerMat,
    ));

    // shaft + tip
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(8.0, STICK_LEN))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.55, 0.32),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
        WPart::Shaft,
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(6.6, 10.0))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.10, 0.12),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
        WPart::ShaftTip,
    ));

    // spin axis through the ball: what the authored spin actually is
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(1.6, 118.0))),
        MeshMaterial3d(m3.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.95, 0.45),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        layer.clone(),
        WPart::SpinBar,
    ));
    for sign in [1.0f32, -1.0] {
        commands.spawn((
            Mesh3d(meshes.add(Cone::new(4.2, 13.0))),
            MeshMaterial3d(m3.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.95, 0.45),
                unlit: true,
                ..default()
            })),
            Transform::default(),
            layer.clone(),
            WPart::SpinCone,
            SpinSign(sign),
        ));
    }

    // ---- HUD ---------------------------------------------------------------
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(12),
            width: px(700),
            padding: UiRect::all(px(10)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            border: px(1.0).all(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.88)),
        BorderColor::all(Color::srgb(0.35, 0.35, 0.45)),
        children![
            (
                Text::new(""),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.92, 0.92, 0.86)),
                ReadoutText,
            ),
            (
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextColor(Color::srgb(0.55, 0.95, 0.55)),
                StatusText,
            ),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(16),
            bottom: px(16),
            width: px(304),
            padding: UiRect::all(px(8)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            align_items: AlignItems::Center,
            border: px(1.0).all(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.88)),
        BorderColor::all(Color::srgb(0.35, 0.35, 0.45)),
        children![
            (
                Text::new("STRIKE MARKER - 3D cue ball, tip contact"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.85, 0.85, 0.8)),
            ),
            (
                ImageNode::new(widget_image.clone()),
                Node {
                    width: px(286),
                    height: px(286),
                    ..default()
                },
                WidgetCard,
            ),
            (
                Text::new("ring = miscue limit 0.514 R  |  green = spin axis  |  + = face centre"),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.7, 0.7, 0.65)),
            ),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(12),
            width: px(760),
            padding: UiRect::all(px(8)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.80)),
        children![
            (
                Text::new(
                    "AIM move the mouse (no button) | POWER press on the table, drag back, release to commit\n\
                     SPIN drag inside the strike marker card (or A/D = a, W/S = b, Shift = fine, C = centre)\n\
                     ELEVATION wheel or UP/DOWN (Shift = 1 deg) | PRESETS 1-9, Tab cycles | RESET 0 | QUIT Esc",
                ),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.8, 0.8, 0.75)),
            ),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(14),
                    ..default()
                },
                children![
                    guide_label("cue (projected)", Color::srgb(0.85, 0.68, 0.42)),
                    guide_label("aim line", Color::srgb(0.95, 0.95, 0.95)),
                    guide_label("ghost ball", Color::srgb(0.65, 0.8, 1.0)),
                    guide_label("object path", Color::srgb(0.95, 0.45, 0.9)),
                    guide_label("tangent (cue ball after impact)", Color::srgb(0.35, 0.95, 0.95)),
                    guide_label("elevation gauge", Color::srgb(1.0, 0.6, 0.2)),
                ],
            ),
        ],
    ));
}

fn guide_label(
    text: &'static str,
    color: Color,
) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont::from_font_size(11.5),
        TextColor(color),
    )
}

#[derive(Component)]
struct SpinSign(f32);

/// 0.0 = the bar along the +a axis, 1.0 = the bar along the +b axis.
#[derive(Component)]
struct CrossBar(f32);

// ---------------------------------------------------------------------------
// scripted screenshot driver

fn script_driver(
    mut commands: Commands,
    mut ui: ResMut<ShotUi>,
    mut script: ResMut<Script>,
    mut phase: ResMut<ScriptPhase>,
    mut flags: ResMut<CaptureFlags>,
    mut exit: MessageWriter<AppExit>,
) {
    match phase.0 {
        Phase::Settle => {
            if script.settle > 0 {
                script.settle -= 1;
                return;
            }
            let preset_idx = script.indices[script.pos];
            let name = presets()[preset_idx].file;
            let path = script.path_for(preset_idx);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()))
                .observe(|_: On<ScreenshotCaptured>, mut flags: ResMut<CaptureFlags>| {
                    flags.saved = true;
                });
            println!("[cue-ux] capture {name} -> {}", path.display());
            phase.0 = Phase::Capturing;
        }
        Phase::Capturing => {
            if flags.saved {
                flags.saved = false;
                script.pos += 1;
                if script.pos >= script.indices.len() {
                    println!("[cue-ux] done");
                    exit.write(AppExit::Success);
                } else {
                    let i = script.indices[script.pos];
                    let p = &presets()[i];
                    ui.decl = p.decl;
                    ui.pull_mm = p.pull_mm;
                    ui.preset_idx = i;
                    ui.label = p.label.to_string();
                    ui.committed = p.commit.then(|| p.decl.json());
                    ui.flash = if p.commit { 100.0 } else { 0.0 };
                    script.settle = script.frames;
                    phase.0 = Phase::Settle;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// interactive input

#[allow(clippy::too_many_arguments)]
fn interactive_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    window: Single<&Window>,
    cam: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
    card: Query<(&ComputedNode, &UiGlobalTransform), With<WidgetCard>>,
    mut ui: ResMut<ShotUi>,
    mut exit: MessageWriter<AppExit>,
) {
    let (camera, cam_tf) = *cam;
    let cursor = window.cursor_position();
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let cursor_world = cursor
        .and_then(|c| camera.viewport_to_world_2d(cam_tf, c).ok())
        .map(|w| Vec2::new(w.x, w.y));
    let over_card = cursor.map(|c| point_in_card(c, &card)).unwrap_or(false);

    // ---- elevation ---------------------------------------------------------
    let mut elev_delta = 0.0;
    let step = if shift { ELEV_STEP / 5.0 } else { ELEV_STEP };
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::BracketRight) {
        elev_delta += step;
    }
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::BracketLeft) {
        elev_delta -= step;
    }
    for ev in wheel.read() {
        if ev.unit == MouseScrollUnit::Line {
            elev_delta += ev.y * step / 2.0;
        }
    }
    if elev_delta != 0.0 {
        ui.decl.elevation = (ui.decl.elevation + elev_delta).clamp(0.0, MAX_ELEV);
        ui.label = "free play (edited)".to_string();
    }

    // ---- spin offsets ------------------------------------------------------
    let fine = if shift { 0.01 } else { 0.05 };
    let mut da = 0.0;
    let mut db = 0.0;
    if keys.just_pressed(KeyCode::KeyD) {
        da += fine;
    }
    if keys.just_pressed(KeyCode::KeyA) {
        da -= fine;
    }
    if keys.just_pressed(KeyCode::KeyW) {
        db += fine;
    }
    if keys.just_pressed(KeyCode::KeyS) {
        db -= fine;
    }
    if da != 0.0 || db != 0.0 {
        ui.decl.a += da;
        ui.decl.b += db;
        ui.label = "free play (edited)".to_string();
    }
    if keys.just_pressed(KeyCode::KeyC) {
        ui.decl.a = 0.0;
        ui.decl.b = 0.0;
    }
    if keys.just_pressed(KeyCode::Digit0) {
        ui.decl = Declaration::default();
        ui.pull_mm = 0.0;
        ui.committed = None;
        ui.label = "free play (edited)".to_string();
    }

    // ---- presets: 1-9 jump straight to a scripted state, Tab cycles all of them
    let preset_keys = [
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
    let all = presets();
    let mut jump: Option<usize> = None;
    for (i, k) in preset_keys.iter().enumerate() {
        if keys.just_pressed(*k) {
            jump = Some(i);
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        jump = Some((ui.preset_idx + 1) % all.len());
    }
    if let Some(i) = jump {
        let p = &all[i.min(all.len() - 1)];
        ui.decl = p.decl;
        ui.pull_mm = p.pull_mm;
        ui.preset_idx = i;
        ui.committed = p.commit.then(|| p.decl.json());
        ui.flash = if p.commit { 100.0 } else { 0.0 };
        ui.label = p.label.to_string();
    }

    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }

    // ---- mouse: aim, then drag-back power ---------------------------------
    if buttons.just_pressed(MouseButton::Left) {
        if over_card {
            ui.spin_drag = true;
        } else {
            ui.power_drag = true;
            ui.pull_mm = 0.0;
        }
    }

    if ui.spin_drag {
        if let (Some(c), Ok((node, xf))) = (cursor, card.single()) {
            let inv = node.inverse_scale_factor();
            let center = xf.translation * inv;
            let size = node.size * inv;
            if size.x > 1.0 {
                let local = (c - center) / size * 2.0; // [-1,1], y down
                let span = SPIN_SELECT_MAX * envelope_tip_radii();
                ui.decl.a = (local.x * span).clamp(-2.0 * span, 2.0 * span);
                ui.decl.b = (-local.y * span).clamp(-2.0 * span, 2.0 * span);
                ui.label = "free play (edited)".to_string();
            }
        }
        if buttons.just_released(MouseButton::Left) {
            ui.spin_drag = false;
        }
    }

    if ui.power_drag {
        if let Some(p) = cursor_world {
            let back = (CUE_BALL - p).dot(ui.decl.aim);
            ui.pull_mm = back.clamp(0.0, MAX_DRAG);
            ui.decl.speed = ShotUi::speed_from_pull(ui.pull_mm);
            ui.label = "free play (edited)".to_string();
        }
        if buttons.just_released(MouseButton::Left) {
            ui.power_drag = false;
            commit(&mut ui);
        }
    } else if let Some(p) = cursor_world {
        let d = p - CUE_BALL;
        if d.length() > 40.0 && !over_card {
            ui.decl.aim = d.normalize();
        }
    }

    if keys.just_pressed(KeyCode::Enter) {
        commit(&mut ui);
    }
}

fn commit(ui: &mut ShotUi) {
    if !ui.decl.valid() {
        return; // visibly rejected: nothing is committed
    }
    if ui.decl.speed <= 0.0 {
        return;
    }
    ui.committed = Some(ui.decl.json());
    ui.flash = 1.0;
}

fn point_in_card(cursor: Vec2, card: &Query<(&ComputedNode, &UiGlobalTransform), With<WidgetCard>>) -> bool {
    let Ok((node, xf)) = card.single() else {
        return false;
    };
    let inv = node.inverse_scale_factor();
    let center = xf.translation * inv;
    let half = node.size * inv * 0.5;
    (cursor - center).abs().cmple(half).all()
}

fn decay_flash(time: Res<Time>, mut ui: ResMut<ShotUi>) {
    if ui.flash > 0.0 {
        ui.flash = (ui.flash - time.delta_secs() * 1.5).max(0.0);
    }
}

// ---------------------------------------------------------------------------
// 3D widget sync

fn sync_widget(
    ui: Res<ShotUi>,
    mut parts: Query<(&mut Transform, &WPart, Option<&SpinSign>, Option<&CrossBar>)>,
    mut marker_mat: Query<&mut MeshMaterial3d<StandardMaterial>, With<MarkerMat>>,
    mut vis: Query<(&mut Visibility, &WPart)>,
    mut m3: ResMut<Assets<StandardMaterial>>,
) {
    let d = ui.decl;
    let off = d.offset();
    let axis = to_bevy(cue_axis(d.aim, d.elevation));
    let p_real = contact_point(d.aim, d.elevation, off);
    let ball_c = Vec3::new(0.0, BALL_R, 0.0);
    let p = to_bevy(p_real) + ball_c;
    let normal = to_bevy(p_real.normalize());
    let spin = to_bevy(spin_axis(d.aim, d.elevation, off));
    let side_b = to_bevy(cue_side(d.aim));
    let up_b = to_bevy(cue_up(d.aim, d.elevation));

    // widget camera: fixed three-quarter view, rotated with the aim so +a stays screen-right
    let side = cue_side(d.aim);
    let cam_real = Vec3::new(
        -d.aim.x * 255.0 + side.x * 22.0,
        -d.aim.y * 255.0 + side.y * 22.0,
        195.0,
    );
    let cam_pos = to_bevy(cam_real) + ball_c;

    // envelope ring on the ball face
    let rho = envelope_mm();
    let h = (BALL_R * BALL_R - rho * rho).max(0.0).sqrt();
    let ring_c = ball_c - axis * h;

    let pulled = (ui.pull_mm * 0.12).clamp(0.0, 45.0);
    let tip_end = p - axis * pulled;
    let tip_len = 10.0;
    let shaft_c = tip_end - axis * (shaft_len() * 0.5);
    let tip_piece_c = tip_end - axis * (tip_len * 0.5);
    let up_rot = Quat::from_rotation_arc(Vec3::Y, axis);

    for (mut tf, part, sign, bar) in &mut parts {
        match part {
            WPart::Camera => {
                *tf = Transform::from_translation(cam_pos).looking_at(ball_c, Vec3::Y);
            }
            WPart::Ring => {
                tf.translation = ring_c;
                tf.rotation = up_rot;
            }
            WPart::Marker => {
                tf.translation = p;
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, normal);
            }
            WPart::MarkerHalo => {
                tf.translation = p - normal * 0.35;
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, normal);
            }
            WPart::Shaft => {
                tf.translation = shaft_c;
                tf.rotation = up_rot;
            }
            WPart::ShaftTip => {
                tf.translation = tip_piece_c;
                tf.rotation = up_rot;
            }
            WPart::SpinBar => {
                tf.translation = ball_c;
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, spin);
            }
            WPart::SpinCone => {
                let s = sign.map(|s| s.0).unwrap_or(1.0);
                tf.translation = ball_c + spin * (59.0 * s);
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, spin * s);
            }
            WPart::CrossBar => {
                let b = bar.map(|c| c.0).unwrap_or(0.0);
                let dir = if b < 0.5 { side_b } else { up_b };
                tf.translation = ball_c - axis * (BALL_R + 0.4);
                tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
            }
            WPart::Ball => {}
        }
    }

    // material + visibility
    let ok_color = Color::srgb(0.05, 0.05, 0.06);
    let bad_color = Color::srgb(0.95, 0.15, 0.15);
    for mat in &mut marker_mat {
        if let Some(mut m) = m3.get_mut(&mat.0) {
            m.base_color = if d.valid() { ok_color } else { bad_color };
        }
    }
    let spin_visible = off.length() > 0.05 || d.elevation > 0.02;
    for (mut v, part) in &mut vis {
        if matches!(part, WPart::SpinBar | WPart::SpinCone) {
            *v = if spin_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Label the elevation gauge in world space so it reads as a gauge, not as a guide line.
fn sync_wedge_label(
    ui: Res<ShotUi>,
    mut wedge: Query<(&mut Text2d, &mut Transform), (With<WedgeLabel>, Without<PullLabel>)>,
    mut pull: Query<(&mut Text2d, &mut Transform), (With<PullLabel>, Without<WedgeLabel>)>,
) {
    let origin = CUE_BALL + Vec2::new(-30.0, -150.0);
    let txt = format!("elevation {:.0} deg", ui.decl.elevation_deg());
    for (mut t, mut tf) in &mut wedge {
        if t.0 != txt {
            t.0 = txt.clone();
        }
        tf.translation = Vec3::new(origin.x + 95.0, origin.y + 52.0, 3.0);
    }

    // the drag-back distance, printed at the cue gap so the pull reads as a length
    let txt = if ui.pull_mm >= 5.0 {
        format!("pull {:.0} mm", ui.pull_mm)
    } else {
        String::new()
    };
    for (mut t, mut tf) in &mut pull {
        if t.0 != txt {
            t.0 = txt.clone();
        }
        let cos_e = ui.decl.elevation.cos();
        let gap = (BALL_R * 0.97 + ui.pull_mm) * cos_e;
        let mid = CUE_BALL - ui.decl.aim * (gap * 0.5);
        let side = Vec2::new(-ui.decl.aim.y, ui.decl.aim.x);
        let at = mid + side * 60.0;
        tf.translation = Vec3::new(at.x, at.y, 3.0);
    }
}

fn shaft_len() -> f32 {
    STICK_LEN
}

// ---------------------------------------------------------------------------
// 2D aim guides, cue stick, elevation gauge

fn draw_guides(ui: Res<ShotUi>, mut gizmos: Gizmos) {
    let d = ui.decl;
    let cue = CUE_BALL;
    let guide = aim_guide(cue, d.aim, &OBJECT_BALLS);

    let c_aim = Srgba::new(0.95, 0.95, 0.95, 0.9);
    let c_ghost = Srgba::new(0.65, 0.8, 1.0, 0.85);
    let c_travel = Srgba::new(0.95, 0.45, 0.9, 0.95);
    let c_tangent = Srgba::new(0.35, 0.95, 0.95, 0.9);
    let c_gauge = Srgba::new(1.0, 0.6, 0.2, 1.0);
    let c_gauge_ref = Srgba::new(0.6, 0.6, 0.6, 0.9);

    // aim line + first-contact guide
    match guide {
        Guide::Ball {
            ghost,
            target,
            travel,
            tangent,
            cut_deg: _,
        } => {
            gizmos.line_2d(cue + d.aim * BALL_R, ghost, c_aim);
            gizmos.circle_2d(ghost, 2.0 * BALL_R, c_ghost);
            gizmos.line_2d(target, target + travel * 320.0, c_travel);
            gizmos.line_2d(ghost, ghost + tangent * 300.0, c_tangent);
        }
        Guide::Cushion { hit, reflect } => {
            gizmos.line_2d(cue + d.aim * BALL_R, hit, c_aim);
            gizmos.circle_2d(hit, 14.0, c_ghost);
            gizmos.line_2d(hit, hit + reflect * 420.0, c_tangent);
        }
    }

    // elevation gauge: a side-view wedge beside the cue ball (candidate answer A)
    let origin = cue + Vec2::new(-30.0, -150.0);
    let arm = 95.0;
    gizmos.line_2d(origin, origin + Vec2::new(arm, 0.0), c_gauge_ref);
    let dir = Vec2::new(d.elevation.cos(), d.elevation.sin());
    gizmos.line_2d(origin, origin + dir * arm, c_gauge);
    if d.elevation > 0.01 {
        gizmos.short_arc_2d_between(
            origin,
            origin + Vec2::X * (arm * 0.55),
            origin + dir * (arm * 0.55),
            c_gauge,
        );
    }

    // power pull-back indicator on the cloth
    let cos_e = d.elevation.cos();
    let gap = (BALL_R * 0.97 + ui.pull_mm) * cos_e;
    if ui.pull_mm > 1.0 {
        let a = cue - d.aim * (gap + 10.0);
        let b = a - d.aim * ui.pull_mm * cos_e;
        gizmos.line_2d(a, b, Srgba::new(1.0, 1.0, 1.0, 0.5));
    }
}

/// The top-down cue: a scaled rectangle so it reads as a stick. Its projected length
/// shrinks by cos(elevation) — the only honest cue elevation signal a top-down view has.
fn sync_stick(
    ui: Res<ShotUi>,
    mut body: Query<&mut Transform, (With<StickBody>, Without<StickTip>, Without<StickButt>)>,
    mut tip: Query<&mut Transform, (With<StickTip>, Without<StickButt>)>,
    mut butt: Query<&mut Transform, With<StickButt>>,
) {
    let d = ui.decl;
    let cos_e = d.elevation.cos();
    let gap = (BALL_R * 0.97 + ui.pull_mm) * cos_e;
    let tip_pos = CUE_BALL - d.aim * gap;
    let len = STICK_LEN * cos_e;
    let mid = tip_pos - d.aim * (len * 0.5);
    let rot = Quat::from_rotation_z(d.aim.y.atan2(d.aim.x));
    let set = |tf: &mut Transform, center: Vec2, length: f32, width: f32| {
        tf.translation = Vec3::new(center.x, center.y, tf.translation.z);
        tf.rotation = rot;
        tf.scale = Vec3::new(length, width, 1.0);
    };
    for mut tf in &mut body {
        set(&mut tf, mid, len, 16.0);
    }
    for mut tf in &mut tip {
        let c = tip_pos - d.aim * (9.0 * cos_e);
        set(&mut tf, c, 18.0 * cos_e, 12.0);
    }
    for mut tf in &mut butt {
        let c = tip_pos - d.aim * (len - 30.0 * cos_e);
        set(&mut tf, c, 60.0 * cos_e, 18.0);
    }
}

// ---------------------------------------------------------------------------
// HUD

fn update_hud(
    ui: Res<ShotUi>,
    mut readout: Query<&mut Text, (With<ReadoutText>, Without<StatusText>)>,
    mut status: Query<(&mut Text, &mut TextColor), With<StatusText>>,
) {
    let d = ui.decl;
    let env = envelope_mm();
    let off = d.offset();
    let fill = ((ui.pull_mm / MAX_DRAG).clamp(0.0, 1.0) * 20.0).round() as usize;
    let bar = format!("{}{}", "#".repeat(fill), "-".repeat(20 - fill));
    let guide_line = match aim_guide(CUE_BALL, d.aim, &OBJECT_BALLS) {
        Guide::Ball {
            target,
            cut_deg,
            travel,
            ..
        } => format!(
            "guide   first ball at ({:.0}, {:.0}) | cut {:.0} deg | object line {:.0} deg | tangent shown",
            target.x,
            target.y,
            cut_deg,
            travel.y.atan2(travel.x).to_degrees()
        ),
        Guide::Cushion { hit, .. } => format!(
            "guide   no ball on the line | cushion at ({:.0}, {:.0}) | reflection shown",
            hit.x, hit.y
        ),
    };
    let txt = format!(
        "CUE-UX PROTOTYPE | wayfinder #10 | throwaway, no physics\n\
         state   {}\n\
         aim     {:+6.1} deg   (x {:+.3}, y {:+.3})\n\
         power   {:>5.0} / {:.0} mm/s   drag-back {:>5.1} / {:.0} mm  [{}]\n\
         spin a  {:+5.2} tr  ({:+5.1} mm)  across the ball face\n\
         spin b  {:+5.2} tr  ({:+5.1} mm)  up the ball face (+ = follow side)\n\
         offset  {:5.1} mm   miscue envelope {:.1} mm (0.514 R, mu 0.6) = {:.2} tr\n\
         eleva   {:5.1} deg   {}\n\
         intent  {}\n\
         {}\n\
         decl    {}",
        ui.label,
        d.aim_deg(),
        d.aim.x,
        d.aim.y,
        d.speed,
        MAX_SPEED,
        ui.pull_mm,
        MAX_DRAG,
        bar,
        d.a,
        d.a * TIP_RADIUS,
        d.b,
        d.b * TIP_RADIUS,
        off.length(),
        env,
        envelope_tip_radii(),
        d.elevation_deg(),
        if d.elevation < 0.017 {
            "(level cue)"
        } else {
            "(butt raised)"
        },
        d.intent(),
        guide_line,
        d.json(),
    );
    for mut t in &mut readout {
        if t.0 != txt {
            t.0 = txt.clone();
        }
    }

    let (line, color) = if !d.valid() {
        (
            format!(
                "REJECTED - offset {:.1} mm is {:.1} mm past the miscue envelope ({:.2} tr limit): past the tip-friction cone (mu {:.1}), so the declaration cannot be committed.",
                off.length(),
                d.envelope_margin(),
                envelope_tip_radii(),
                MU_TIP
            ),
            Color::srgb(1.0, 0.35, 0.35),
        )
    } else if ui.flash > 0.0 {
        (
            format!(
                "COMMITTED (prototype: no physics, balls do not move) - drag back again to author the next shot."
            ),
            Color::srgb(0.55, 1.0, 0.55),
        )
    } else if ui.power_drag {
        (
            format!("AUTHORING - pulling the cue back, release to commit."),
            Color::srgb(1.0, 0.95, 0.6),
        )
    } else if let Some(last) = &ui.committed {
        (
            format!("last commit  {last}"),
            Color::srgb(0.7, 0.75, 0.7),
        )
    } else {
        (
            "AUTHORING - press on the table and drag back to set power, release (or Enter) to commit."
                .to_string(),
            Color::srgb(0.75, 0.85, 0.75),
        )
    };
    for (mut t, mut c) in &mut status {
        if t.0 != line {
            t.0 = line.clone();
        }
        c.0 = color;
    }
}
