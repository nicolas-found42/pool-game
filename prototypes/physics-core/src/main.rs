//! physics-core: throwaway headless prototype of the pool physics core
//! (wayfinder ticket #8). Subcommands are the measurement gates named in
//! README.md.

mod consts;
mod diff;
mod diag;
mod facts;
mod font;
mod json;
mod ladder;
mod render;
mod rows;
mod shots;
mod sim;
mod strike;
mod table;
mod vec;
mod zhang;

use std::fs;
use std::path::PathBuf;

fn arg(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: physics-core <shot|break|rows|diag|render|ladder|zhang> [options]");
        std::process::exit(2);
    }
    let cmd = args[1].as_str();
    let out = arg(&args, "--out").unwrap_or_else(|| "out".to_string());
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).expect("create out dir");
    match cmd {
        "shot" => {
            let name = arg(&args, "--preset").unwrap_or_else(|| "break".to_string());
            diag::run_preset(&name, &out);
        }
        "break" => diag::break_report(&out),
        "diag" => diag::full(&out),
        "selftest" => diag::selftest(),
        "rest" => diag::dump_rest(),
        "cushion" => {
            let v: f64 = arg(&args, "--v").map(|s| s.parse().unwrap()).unwrap_or(1500.0);
            diag::dbg_cushion(v);
        }
        "probe" => {
            let aim: f64 = arg(&args, "--aim").map(|s| s.parse().unwrap()).unwrap_or(-2.0);
            let sp: f64 = arg(&args, "--speed").map(|s| s.parse().unwrap()).unwrap_or(1400.0);
            let a: f64 = arg(&args, "--a").map(|s| s.parse().unwrap()).unwrap_or(0.0);
            let b: f64 = arg(&args, "--b").map(|s| s.parse().unwrap()).unwrap_or(0.0);
            let e: f64 = arg(&args, "--elev").map(|s| s.parse().unwrap()).unwrap_or(0.0);
            diag::probe(aim, sp, a, b, e);
        }
        "diff" => diff::run_diff(&out),
        "tolerance" => diag::pin_tolerance(&out),
        "ke" => diag::ke_trace(&[0.22, 0.231, 0.232, 0.24, 0.25, 0.26, 0.3, 0.4, 0.6, 1.0, 2.0]),
        "spin" => {
            let w: f64 = arg(&args, "--w").map(|s| s.parse().unwrap()).unwrap_or(-110.8);
            diag::dbg_spin(w);
        }
        "drawtest" => {
            let v: f64 = arg(&args, "--v").map(|s| s.parse().unwrap()).unwrap_or(4000.0);
            let w: f64 = arg(&args, "--w").map(|s| s.parse().unwrap()).unwrap_or(-110.8);
            let d: f64 = arg(&args, "--drag").map(|s| s.parse().unwrap()).unwrap_or(304.8);
            diag::dbg_draw(v, w, d);
        }
        "debug" => {
            let g: u64 = arg(&args, "--groups").map(|s| s.parse().unwrap()).unwrap_or(80);
            diag::debug_break(g);
        }
        "rows" => {
            let spec = arg(&args, "--spec")
                .unwrap_or_else(|| "docs/spec/physics-break.json".to_string());
            let budget: usize = arg(&args, "--budget")
                .map(|s| s.parse().unwrap())
                .unwrap_or(400);
            let only: Vec<String> = arg(&args, "--only")
                .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                .unwrap_or_default();
            rows::run_all(&spec, &out, budget, &only);
        }
        "ladder" => {
            let stage = arg(&args, "--stage").unwrap_or_else(|| "all".to_string());
            let curves = arg(&args, "--curves").unwrap_or_else(|| "data/curves".to_string());
            ladder::run(&stage, &curves, &out);
        }
        "render" => {
            let name = arg(&args, "--preset").unwrap_or_else(|| "all".to_string());
            diag::render_all(&name, &out);
        }
        "zhang" => {
            let strikes = arg(&args, "--strikes").unwrap_or_default();
            let limit: usize = arg(&args, "--limit")
                .map(|s| s.parse().unwrap())
                .unwrap_or(40);
            zhang::replay(&strikes, &out, limit);
        }
        other => {
            eprintln!("unknown subcommand {other}");
            std::process::exit(2);
        }
    }
}
