//! The shell's HUD (`architecture.md` §10's `ui.rs`): the 3-line player read-out and the message
//! slot of `ux-cue.md` §8, the ball-in-hand prompt, the choice menus, and the development panel
//! behind `--debug-candidates`.
//!
//! §8 splits the read-out in two, and the split is normative: the player lines carry the turn and
//! the group, the declaration being authored, and one line of validity — speed in m/s, no `tr`/`mm`
//! unit explanations, no envelope arithmetic, no guide geometry, no declaration JSON. Those are
//! dev-panel material, and they live in [`Panel::Dev`], which only the flag turns on.

use bevy::prelude::*;
use pool_rules::state::Target;

use crate::input::{self, Cue};
use crate::playback::Playback;
use crate::session::{self, Awaiting, Game, Tone};

/// The window's logical width, the layout's reference (`main.rs`'s fixed capture size).
const WINDOW_W: f32 = 1920.0;
/// The left margin every panel shares.
const MARGIN: f32 = 16.0;
/// The player column's width.
const COLUMN_W: f32 = 760.0;

/// The dynamic panels: each carries its own line of text, updated every frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// §8's line 1: turn and group.
    Turn,
    /// §8's line 2: the declaration being authored.
    Declaration,
    /// §8's line 3: the one-line validity state.
    Validity,
    /// The last record, in a sentence: fouls, turns, rack and match results.
    Message,
    /// The ball-in-hand prompt (`rules.md` §6).
    Prompt,
    /// The choice menu (`rules-break.md` §3's trees).
    Menu,
    /// The development panel (§8's behind-a-flag read-out).
    Dev,
}

/// The static panels: their text is spawned once, and only their visibility moves.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Static {
    /// The guide legend — on whenever guides are drawn (§5/§10.8).
    Legend,
    /// The controls hint.
    Hints,
}

/// The palette the panels share.
const PANEL_BG: Color = Color::srgba(0.02, 0.02, 0.04, 0.88);
const PANEL_BORDER: Color = Color::srgb(0.35, 0.35, 0.45);
const INK: Color = Color::srgb(0.92, 0.92, 0.86);
const INK_DIM: Color = Color::srgb(0.72, 0.72, 0.68);
const INK_GOOD: Color = Color::srgb(0.55, 0.95, 0.55);
const INK_BAD: Color = Color::srgb(1.00, 0.35, 0.35);
const INK_WARN: Color = Color::srgb(1.00, 0.85, 0.45);

/// The HUD systems.
pub fn systems(app: &mut App) {
    app.add_systems(Startup, spawn_hud)
        .add_systems(Update, (update_panels, update_visibility));
}

/// A one-entity panel: its own node, its own line of text.
fn text_panel(tag: Panel, top: f32, font: f32) -> impl Bundle {
    (
        tag,
        Node {
            position_type: PositionType::Absolute,
            left: px(MARGIN),
            top: px(top),
            width: px(COLUMN_W),
            padding: px(8).all(),
            border: px(1).all(),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        Text::new(""),
        TextFont::from_font_size(font),
        TextColor(INK),
    )
}

/// A static panel: a bordered box whose text is fixed at spawn.
fn static_panel(tag: Static, text: &'static str, top: f32, color: Color) -> impl Bundle {
    (
        tag,
        Node {
            position_type: PositionType::Absolute,
            left: px(MARGIN),
            bottom: px(top),
            width: px(COLUMN_W),
            padding: px(6).all(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.70)),
        Text::new(text),
        TextFont::from_font_size(13.0),
        TextColor(color),
    )
}

/// Spawn the HUD: §8's three player lines, the message slot, the prompt, the menu, the legend, the
/// controls, and the dev panel (hidden unless `--debug-candidates` asks for it).
fn spawn_hud(mut commands: Commands) {
    commands.spawn(text_panel(Panel::Turn, MARGIN, 18.0));
    commands.spawn(text_panel(Panel::Declaration, MARGIN + 46.0, 15.0));
    commands.spawn(text_panel(Panel::Validity, MARGIN + 92.0, 15.0));
    commands.spawn(text_panel(Panel::Message, MARGIN + 138.0, 15.0));
    commands.spawn(text_panel(Panel::Prompt, MARGIN + 184.0, 15.0));
    commands.spawn(text_panel(Panel::Menu, MARGIN + 184.0, 15.0));
    // The dev panel sits on the right, clear of the strike card.
    commands.spawn((
        Panel::Dev,
        Node {
            position_type: PositionType::Absolute,
            right: px(MARGIN),
            top: px(MARGIN),
            width: px(WINDOW_W * 0.42),
            padding: px(8).all(),
            border: px(1).all(),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        Text::new(""),
        TextFont::from_font_size(12.5),
        TextColor(INK_DIM),
        Visibility::Hidden,
    ));

    commands.spawn(static_panel(
        Static::Hints,
        "CONTROLS  aim the mouse · press on the cloth and drag back, release commits (Enter commits, Escape abandons)\n\
         SPIN  drag inside the card, or A/D = a, W/S = b, C = centre · ELEVATION  wheel or UP/DOWN (Shift = fine)\n\
         CALL  Tab cycles the suggested call · T toggles the tangent line · 1-9 pick a menu option",
        72.0,
        INK_DIM,
    ));
    commands.spawn((
        Static::Legend,
        Node {
            position_type: PositionType::Absolute,
            left: px(MARGIN),
            bottom: px(MARGIN),
            padding: px(6).all(),
            flex_direction: FlexDirection::Row,
            column_gap: px(14),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.70)),
        children![
            legend_entry("aim line", Color::srgb(0.95, 0.95, 0.95)),
            legend_entry("ghost ball", Color::srgb(0.65, 0.80, 1.00)),
            legend_entry("object path", Color::srgb(0.95, 0.45, 0.90)),
            legend_entry("tangent (T)", Color::srgb(0.35, 0.95, 0.95)),
            legend_entry("elevation gauge", Color::srgb(1.00, 0.60, 0.20)),
            legend_entry("dimension bracket = pull", Color::srgb(0.95, 0.95, 0.95)),
        ],
    ));
}

/// One legend entry: the colour is the key, so the text carries it (`ux-cue.md` §5: the legend stays
/// on whenever guides are drawn).
fn legend_entry(text: &'static str, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont::from_font_size(13.0),
        TextColor(color),
    )
}

/// §8's line 1: whose turn it is, and the group that seat is on (`rules.md` §4.4).
fn turn_line(game: &Game) -> String {
    let view = game.view();
    let race = format!(
        "rack {} (race {}–{}, first to {})",
        view.rack_index + 1,
        view.race[0],
        view.race[1],
        view.race_target
    );
    match game.awaiting() {
        Awaiting::Shot {
            shooter,
            target,
            is_break,
        } => {
            let group = if is_break {
                "the break".to_string()
            } else {
                target_text(target, shooter, game)
            };
            format!("{} to shoot — {group} — {race}", session::seat_name(shooter))
        }
        Awaiting::Placement { shooter, domain } => format!(
            "{} has ball in hand {} — {race}",
            session::seat_name(shooter),
            session::domain_name(domain)
        ),
        Awaiting::Choice { chooser, .. } => {
            format!("{} chooses — {race}", session::seat_name(chooser))
        }
        Awaiting::RackOver { winner: _ } => format!("rack over — {race}"),
        Awaiting::MatchOver { winner } => {
            format!("{} wins the match — {race}", session::seat_name(winner))
        }
    }
}

/// The shooter's target, in the HUD's words (`rules.md` §2/§4.4/§5).
fn target_text(target: Target, shooter: pool_rules::Player, game: &Game) -> String {
    match target {
        Target::Open => "open table".to_string(),
        Target::OnTheEight => "on the 8".to_string(),
        Target::Group => game
            .view()
            .assignment
            .map_or_else(|| "its group".to_string(), |groups| {
                session::group_name(groups[shooter.index()]).to_string()
            }),
    }
}

/// §8's line 2: the declaration being authored, at the player's level of detail.
fn declaration_line(game: &Game, cue: &Cue) -> String {
    let view = game.view();
    let speed_scale = format!("{:.1} m/s", f64::from(input::speed_from_pull(cue.pull_mm)) / 1000.0);
    let authored = format!(
        "call {}  |  aim {:.0}°  |  {}  |  spin (a {:+.2}, b {:+.2})  |  elevation {:.0}°",
        cue.call_text(),
        f64::from(cue.aim[1]).atan2(f64::from(cue.aim[0])).to_degrees(),
        speed_scale,
        cue.spin[0],
        cue.spin[1],
        cue.elevation_rad.to_degrees(),
    );
    if let Some(shot) = view.shot_count.checked_sub(1) {
        format!("{authored}  |  shot {shot} of rack {}", view.rack_index + 1)
    } else {
        authored
    }
}

/// §8's line 3: one line of validity, and nothing else.
fn validity_line(game: &Game, cue: &Cue, playback: &Playback) -> (String, Color) {
    if let Some(refusal) = &cue.refusal {
        return (format!("rejected — {refusal}"), INK_BAD);
    }
    if !cue.legal() {
        return (
            format!(
                "rejected — offset {:.1} mm is {:.1} mm past the miscue envelope; the declaration cannot be committed",
                cue.offset_mm(),
                cue.overage_mm()
            ),
            INK_BAD,
        );
    }
    match game.awaiting() {
        Awaiting::Placement { .. } => (
            "point at the table and click to place the cue ball".to_string(),
            INK_WARN,
        ),
        Awaiting::Choice { .. } => ("press the number of an option".to_string(), INK_WARN),
        Awaiting::MatchOver { .. } | Awaiting::RackOver { .. } => {
            (game.line().text.clone(), ink_of(game.line().tone))
        }
        Awaiting::Shot { .. } => {
            if !playback.at_rest() {
                ("the shot is running".to_string(), INK_DIM)
            } else if cue.phase == input::Phase::PowerDrag {
                (
                    format!(
                        "pulling back {:.0} mm — release to commit, Escape abandons",
                        cue.pull_mm
                    ),
                    INK_WARN,
                )
            } else if input::speed_from_pull(cue.pull_mm) > 0.0 {
                (
                    "legal — release or Enter to commit, Escape abandons a drag".to_string(),
                    INK_GOOD,
                )
            } else {
                (
                    "press on the cloth and drag back to set the power".to_string(),
                    INK_DIM,
                )
            }
        }
    }
}

/// The colour a tone paints.
fn ink_of(tone: Tone) -> Color {
    match tone {
        Tone::Neutral => INK_DIM,
        Tone::Good => INK_GOOD,
        Tone::Bad => INK_BAD,
    }
}

/// The ball-in-hand prompt (`rules.md` §6): the domain, and why the proposal is refused if it is.
fn prompt_text(game: &Game, cue: &Cue) -> String {
    let Awaiting::Placement { shooter, domain } = game.awaiting() else {
        return String::new();
    };
    let fault = match cue.placement.as_ref().and_then(|proposal| proposal.fault.as_ref()) {
        None => "click to place the cue ball".to_string(),
        Some(fault) => format!("cannot place here: {fault:?}"),
    };
    format!(
        "BALL IN HAND — {} places {}: {fault}",
        session::seat_name(shooter),
        session::domain_name(domain)
    )
}

/// The choice menu (`rules-break.md` §3's trees): every presented option, numbered.
fn menu_text(offers: &[pool_rules::Offer]) -> String {
    let mut lines = Vec::new();
    let mut index = 1;
    for offer in offers {
        lines.push(format!("CHOOSE — {}", offer.tree.name().replace('_', " ")));
        for option in &offer.options {
            lines.push(format!("  [{index}] {}", option.description));
            index += 1;
        }
    }
    lines.join("\n")
}

/// The development panel (§8): every quantity the player lines leave out.
fn dev_text(game: &Game, cue: &Cue, playback: &Playback) -> String {
    let declaration = cue.declaration();
    let json = serde_json::to_string(&declaration).unwrap_or_else(|_| "{}".to_string());
    let view = game.view();
    let envelope = input::envelope_mm();
    let guide = match cue.guide {
        input::Guide::Ball {
            ball,
            target,
            ghost,
            travel,
            tangent,
            cut_deg,
        } => format!(
            "ball {ball} at ({:+.1},{:+.1}) | ghost ({:+.1},{:+.1}) | cut {cut_deg:.1}° | object line {:+.0}° | tangent {:+.0}°{}",
            target.x, target.y, ghost.x, ghost.y,
            f64::from(travel.y).atan2(f64::from(travel.x)).to_degrees(),
            f64::from(tangent.y).atan2(f64::from(tangent.x)).to_degrees(),
            if cue.tangent { " (on)" } else { " (off)" },
        ),
        input::Guide::Cushion { hit, reflect } => format!(
            "no ball on the line | cushion ({:+.1},{:+.1}) | reflection {:+.0}°",
            hit.x,
            hit.y,
            f64::from(reflect.y).atan2(f64::from(reflect.x)).to_degrees(),
        ),
    };
    let presentation = playback.t_rest_s().map_or_else(
        || "at rest".to_string(),
        |rest| format!("t {:.2} / {rest:.2} s", playback.t_s()),
    );
    format!(
        "DEV — --debug-candidates\n\
         decl      {json}\n\
         a, b      {:+.3}, {:+.3} envelope fractions (1.0 = the limit; a > 0 right, b > 0 above centre)\n\
         offset    {:.2} mm = {:.3} of rho {:.2} mm ({:.3} R, mu {:.1}) = {:.2} tip radii\n\
         limit     |offset_mm| <= rho + 1e-3 ; margin {:+.3} mm\n\
         guide     {guide}\n\
         power     pull {:.1} / {:.0} mm -> {:.0} mm/s (v = 7000 (pull/350)^2); the {:.0} mm/s soft band ends at {:.0} mm of pull\n\
         state     {} | {} | {presentation}\n\
         difficulty {} (inert: both seats are human in this slice)\n\
         profile   {} | match seed {} | shot {} of rack {}",
        cue.spin[0],
        cue.spin[1],
        cue.offset_mm(),
        cue.offset_mm() / envelope,
        envelope,
        envelope / pool_sim::constants::BALL_RADIUS_MM,
        pool_sim::constants::TIP_FRICTION_MU,
        cue.offset_mm() / 6.35,
        cue.offset_mm() - envelope,
        cue.pull_mm,
        input::MAX_PULL_MM,
        input::speed_from_pull(cue.pull_mm),
        input::SOFT_BAND_MM_S,
        input::pull_for_speed(input::SOFT_BAND_MM_S),
        view.state.kind(),
        game.line().text,
        session::difficulty_name(game.config().difficulty.level),
        game.config().profile.id,
        game.config().match_seed,
        view.shot_count,
        view.rack_index + 1,
    )
}

/// The one update for every dynamic panel.
fn update_panels(
    game: Res<Game>,
    cue: Res<Cue>,
    playback: Res<Playback>,
    mut panels: Query<(&Panel, &mut Text, &mut TextColor)>,
) {
    let (validity, validity_ink) = validity_line(&game, &cue, &playback);
    let message = game.line().text.clone();
    let message_ink = ink_of(game.line().tone);
    let menu = match game.awaiting() {
        Awaiting::Choice { offers, .. } => menu_text(&offers),
        _ => String::new(),
    };
    for (panel, mut text, mut color) in &mut panels {
        let (wanted, ink) = match panel {
            Panel::Turn => (turn_line(&game), INK),
            Panel::Declaration => (declaration_line(&game, &cue), INK),
            Panel::Validity => (validity.clone(), validity_ink),
            Panel::Message => (message.clone(), message_ink),
            Panel::Prompt => (prompt_text(&game, &cue), INK_WARN),
            Panel::Menu => (menu.clone(), INK_WARN),
            Panel::Dev => (dev_text(&game, &cue, &playback), INK_DIM),
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
        color.0 = ink;
    }
}

/// Visibility: the prompt and the menu follow the state, the legend follows the guides (§5: the
/// legend stays on whenever guides are drawn), and the dev panel follows the flag.
fn update_visibility(
    game: Res<Game>,
    cue: Res<Cue>,
    playback: Res<Playback>,
    args: Res<crate::Args>,
    mut panels: Query<(&Panel, &mut Visibility)>,
    mut statics: Query<(&Static, &mut Visibility)>,
) {
    let awaiting = game.awaiting();
    for (panel, mut visibility) in &mut panels {
        let shown = match panel {
            Panel::Prompt => matches!(awaiting, Awaiting::Placement { .. }),
            Panel::Menu => matches!(awaiting, Awaiting::Choice { .. }),
            Panel::Dev => args.debug_candidates,
            _ => true,
        };
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (tag, mut visibility) in &mut statics {
        let shown = match tag {
            Static::Legend => input::guides_drawn(&game, &cue, &playback),
            Static::Hints => true,
        };
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
