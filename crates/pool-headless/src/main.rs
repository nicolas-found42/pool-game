//! `pool-headless` (`architecture.md` §7): the one binary that runs the game without Bevy.
//!
//! `replay` runs a whole recorded match through the shared `Session` loop and reports its final state;
//! `strike` computes one shot from a rack seed and a strike declaration — the physics debugging hook.
//! `pipe`, §7's third mode, carries `pool-ai`'s encoding and lands with that crate (M4): the
//! subcommand is absent rather than stubbed, so nothing here pretends to serve a policy.
//!
//! Both modes read files with `std::fs` (`architecture.md` §1: the crates do no I/O, the binaries do).
//! `config/profiles/<id>.json` is resolved from the current directory, so the tool runs from the
//! repository root.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use pool_match::{Entry, InputLog, MatchConfig, Request, Session};
use pool_rules::record::Adjudication;
use pool_rules::{Player, Spin, Vec2};
use pool_sim::rack;
use pool_sim::state_hash::state_hash_hex;
use pool_sim::strike::StrikeDecl;
use pool_sim::table::Table;
use pool_sim::{Profile, Sim};
use serde::Deserialize;

/// The CLI's error: a message for the user, printed once and followed by a non-zero exit.
type Result<T, E = String> = std::result::Result<T, E>;

#[derive(Parser)]
#[command(
    name = "pool-headless",
    version,
    about = "Run the game without Bevy: replay a recorded match, or compute one shot",
    long_about = "Run the game without Bevy (architecture.md §7).\n\n\
                  `replay` runs a whole input log through the shared Session loop, printing every \
                  adjudication and the match's final state; a log the rules layer refuses exits \
                  non-zero and names the entry.\n\n\
                  `strike` computes one shot from a rack seed and a strike file (the declaration's \
                  physics half: aim, speed, spin, elevation).\n\n\
                  Profile records are read from config/profiles/<id>.json, so run from the repository \
                  root."
)]
struct Cli {
    /// The mode to run.
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Run a whole match through `Session` and report its final state.
    Replay {
        /// The recorded input log (`docs/spec/input-log.schema.json`).
        input_log: PathBuf,
        /// Write one adjudication record per line.
        #[arg(long, value_name = "records.jsonl")]
        emit: Option<PathBuf>,
        /// Write the facts stream, one fact per line.
        #[arg(long, value_name = "events.jsonl")]
        emit_events: Option<PathBuf>,
    },
    /// Compute one shot from a rack seed and a strike declaration.
    Strike {
        /// The rack's seed (`rules-break.md` §2.6). The cue ball starts at its rack position
        /// unless `--placement` moves it.
        #[arg(long)]
        rack_seed: u64,
        /// The cue ball's placement, `x,y` in mm, table frame.
        #[arg(long, value_parser = parse_point, value_name = "x,y", allow_hyphen_values = true)]
        placement: Option<[f64; 2]>,
        /// The strike declaration: `aim`, `speed`, `spin`, `elevation`.
        #[arg(long, value_name = "strike.json")]
        strike_file: PathBuf,
        /// Write the facts stream, one fact per line.
        #[arg(long, value_name = "events.jsonl")]
        emit_events: Option<PathBuf>,
    },
}

/// The strike file (`--strike-file`): the declaration's physics half, in the input log's spelling.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrikeFile {
    /// Unit horizontal aim direction in the table frame.
    aim: Vec2,
    /// Cue-ball launch speed (mm/s).
    speed: f64,
    /// Tip offset, as fractions of the miscue envelope (`physics.md` §4).
    spin: Spin,
    /// Cue elevation (rad).
    elevation: f64,
}

fn main() {
    let result = match Cli::parse().mode {
        Mode::Replay {
            input_log,
            emit,
            emit_events,
        } => replay(&input_log, emit.as_deref(), emit_events.as_deref()),
        Mode::Strike {
            rack_seed,
            placement,
            strike_file,
            emit_events,
        } => strike(rack_seed, placement, &strike_file, emit_events.as_deref()),
    };
    if let Err(message) = result {
        eprintln!("pool-headless: {message}");
        std::process::exit(1);
    }
}

/// `replay <input-log.json> [--emit records.jsonl] [--emit-events events.jsonl]`.
///
/// The log is the free-choice sequence and the run is the whole match: the loop ends only with the
/// race decided (`architecture.md` §7's "runs a whole match"), so a log that stops mid-rack is
/// refused rather than reported as a finished match.
fn replay(input_log: &Path, emit: Option<&Path>, emit_events: Option<&Path>) -> Result<()> {
    let log: InputLog = read_log(input_log)?;
    let profile = load_profile(&log.profile)?;
    let mut session = Session::new(MatchConfig::from_log(&log, profile));
    let mut records: Vec<Adjudication> = Vec::with_capacity(log.entries.len());
    let mut events = String::new();
    let mut shots = 0_u32;

    for (index, entry) in log.entries.iter().enumerate() {
        let record = session
            .request(Request::from_entry(entry))
            .map_err(|error| format!("entries[{index}]: {error}"))?;
        println!(
            "entries[{index}] {}: {:?} {}",
            entry_kind(entry),
            record.verdict,
            record.legality
        );
        if matches!(entry, Entry::Declaration(_)) {
            shots += 1;
            if emit_events.is_some()
                && let Some(shot) = session.last_shot()
            {
                for fact in shot.events() {
                    events.push_str(
                        &serde_json::to_string(fact)
                            .map_err(|error| format!("a fact does not serialize: {error}"))?,
                    );
                    events.push('\n');
                }
            }
        }
        records.push(record);
    }

    let winner = session.winner().ok_or_else(|| {
        format!(
            "the log ends with the match unfinished ({})",
            session.state().state.kind()
        )
    })?;
    let view = session.state();
    println!(
        "match: {} wins the race {}-{} (target {})",
        seat(winner),
        view.race[0],
        view.race[1],
        view.race_target
    );
    println!("racks: {}, shots: {shots}", view.rack_index + 1);
    println!("entries: {}", log.entries.len());
    println!("final state hash: {}", session.state_hash());

    if let Some(path) = emit {
        let mut text = String::new();
        for record in &records {
            text.push_str(
                &serde_json::to_string(record)
                    .map_err(|error| format!("a record does not serialize: {error}"))?,
            );
            text.push('\n');
        }
        write_file(path, &text)?;
    }
    if let Some(path) = emit_events {
        write_file(path, &events)?;
    }
    Ok(())
}

/// `strike --rack-seed <u64> [--placement x,y] --strike-file <strike.json> [--emit-events ...]`.
fn strike(
    rack_seed: u64,
    placement: Option<[f64; 2]>,
    strike_file: &Path,
    emit_events: Option<&Path>,
) -> Result<()> {
    let declaration: StrikeFile = serde_json::from_str(
        &std::fs::read_to_string(strike_file)
            .map_err(|error| format!("cannot read {}: {error}", strike_file.display()))?,
    )
    .map_err(|error| {
        format!(
            "{} is not a strike declaration: {error}",
            strike_file.display()
        )
    })?;

    let mut sim = Sim::new(
        Table::with_profile(Profile::default_profile()),
        rack::generate(rack_seed),
    );
    if let Some(pos_mm) = placement {
        sim.place_cue(pos_mm)
            .map_err(|error| format!("the placement was rejected: {error:?}"))?;
    }
    let shot = sim
        .strike(StrikeDecl {
            aim: [declaration.aim.x, declaration.aim.y],
            speed_mm_s: declaration.speed,
            spin: [declaration.spin.a, declaration.spin.b],
            elevation_rad: declaration.elevation,
        })
        .map_err(|error| format!("the strike was rejected: {error:?}"))?;

    let outcome = shot.outcome();
    println!(
        "shot: rest at {:.6} s, {} events, {} groups, max penetration {:.6} mm",
        outcome.t_rest_s, outcome.events, outcome.groups, outcome.max_penetration_mm
    );
    println!("pocketed: {:?}", shot.rest().pocketed);
    println!("off table: {:?}", shot.rest().off_table);
    println!("rest state hash: {}", state_hash_hex(&shot.rest().states));

    if let Some(path) = emit_events {
        let mut text = String::new();
        for fact in shot.events() {
            text.push_str(
                &serde_json::to_string(fact)
                    .map_err(|error| format!("a fact does not serialize: {error}"))?,
            );
            text.push('\n');
        }
        write_file(path, &text)?;
    }
    Ok(())
}

/// Read a log and validate it against its schema's bounds (`docs/spec/input-log.schema.json`).
fn read_log(path: &Path) -> Result<InputLog> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    InputLog::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// The profile record the log's header names (`architecture.md` §12), read from the current
/// directory.
fn load_profile(id: &str) -> Result<Profile> {
    let path = PathBuf::from("config/profiles").join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run the tool from the repository root)",
            path.display()
        )
    })?;
    let profile: Profile = serde_json::from_str(&text)
        .map_err(|error| format!("{} does not parse: {error}", path.display()))?;
    if profile.id != id {
        return Err(format!(
            "{} holds profile {:?}, but the log names {id:?}",
            path.display(),
            profile.id
        ));
    }
    Ok(profile)
}

fn write_file(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// The entry's kind, as the log tags it.
fn entry_kind(entry: &Entry) -> &'static str {
    match entry {
        Entry::Placement { .. } => "placement",
        Entry::SpotRequest => "spot_request",
        Entry::Declaration(_) => "declaration",
        Entry::Option { .. } => "option",
        Entry::Stalemate => "stalemate",
    }
}

/// A seat's name, as the log and the report write it.
fn seat(player: Player) -> &'static str {
    match player {
        Player::P1 => "p1",
        Player::P2 => "p2",
    }
}

/// `x,y` in millimetres.
fn parse_point(text: &str) -> std::result::Result<[f64; 2], String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("{text:?} is not `x,y`"))?;
    let parse = |value: &str| {
        value
            .trim()
            .parse::<f64>()
            .map_err(|error| format!("{value:?} is not a number: {error}"))
    };
    Ok([parse(x)?, parse(y)?])
}
