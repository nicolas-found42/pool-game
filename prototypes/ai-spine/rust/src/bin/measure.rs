//! Measurement harness for wayfinder #16. Every number in `docs/spec/ai-constants.md` that is
//! labelled *measured* comes out of this binary; the JSON it writes is the raw record.
//!
//! Usage:
//!   measure gen      --out-dir <dir>
//!   measure decision --out-dir <dir>
//!   measure eval     --out-dir <dir>
//!   measure dump     --out-dir <dir>
//!   measure all      --out-dir <dir>

use std::path::PathBuf;

use ai_spine_proto::encode::{encode_cand, encode_cands, encode_obs, ObsCtx, CAND_DIM, OBS_DIM};
use ai_spine_proto::gen::{self, GenCfg, Kind};
use ai_spine_proto::geom::{pockets, Ob, V2};
use ai_spine_proto::planner::{self, PlannerCfg};
use ai_spine_proto::positions;
use ai_spine_proto::rng::SplitMix64;
use ai_spine_proto::toy_sim;
use serde_json::{json, Value};

fn machine_info() -> Value {
    let sh = |cmd: &str, args: &[&str]| -> String {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let cpu = sh("sysctl", &["-n", "machdep.cpu.brand_string"]);
    let ncpu = sh("sysctl", &["-n", "hw.ncpu"]);
    let mem = sh("sysctl", &["-n", "hw.memsize"]);
    let os = sh("sw_vers", &["-productVersion"]);
    json!({
        "cpu": cpu,
        "ncpu": ncpu.parse::<u64>().unwrap_or(0),
        "mem_bytes": mem.parse::<u64>().unwrap_or(0),
        "os": format!("macOS {}", os),
        "arch": std::env::consts::ARCH,
        "profile": "release",
    })
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn stats_ms(v: &[f64]) -> Value {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = if s.is_empty() {
        0.0
    } else {
        s.iter().sum::<f64>() / s.len() as f64
    };
    json!({
        "n": s.len(),
        "mean": mean,
        "p50": pct(&s, 50.0),
        "p90": pct(&s, 90.0),
        "p99": pct(&s, 99.0),
        "max": s.last().copied().unwrap_or(0.0),
    })
}

fn write_json(dir: &PathBuf, name: &str, v: &Value) {
    std::fs::create_dir_all(dir).expect("create out dir");
    let p = dir.join(name);
    std::fs::write(&p, serde_json::to_string_pretty(v).expect("serialize")).expect("write");
    println!("wrote {}", p.display());
}

fn out_dir(args: &[String]) -> PathBuf {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--out-dir" {
            if let Some(v) = it.next() {
                return PathBuf::from(v);
            }
        }
    }
    PathBuf::from("results")
}

// ---------------------------------------------------------------- generation throughput

fn measure_gen(dir: &PathBuf) {
    const N_BALLS: [(usize, &str); 3] = [(3, "3-ball"), (6, "6-ball"), (15, "full-rack")];
    const DEPTHS: [u8; 4] = [0, 1, 2, 3];
    const REPS: usize = 12;

    let mut rows: Vec<Value> = Vec::new();
    let mut shortlist_us: Vec<f64> = Vec::new();

    for (n, nlabel) in N_BALLS {
        for depth in DEPTHS {
            let cfg = GenCfg {
                max_rails: depth,
                banks: depth > 0,
                kicks: depth > 0,
                combos: true,
                safeties: true,
                max_cands: 32,
                min_cut_cos: 0.10,
            };
            let mut times: Vec<f64> = Vec::new();
            let mut constructed = 0usize;
            let mut kept = 0usize;
            let mut dedup = 0usize;
            let mut by_kind = [0usize; 7];
            let mut bank_rails = [0usize; 4];
            let mut kick_rails = [0usize; 4];
            let mut total = 0.0f64;
            for rep in 0..REPS {
                let pos = positions::drill(1000 + rep as u64, n, 8);
                let t = std::time::Instant::now();
                let out = gen::generate(pos.cue, &pos.objs, &cfg);
                let dt = t.elapsed().as_secs_f64();
                times.push(dt * 1000.0);
                total += dt;
                constructed += out.stats.constructed;
                kept += out.stats.kept;
                dedup += out.stats.after_dedup;
                for (k, c) in out.stats.by_kind.iter().enumerate() {
                    by_kind[k] += c;
                }
                for d in 0..4 {
                    bank_rails[d] += out.stats.bank_by_rails[d];
                    kick_rails[d] += out.stats.kick_by_rails[d];
                }
                if depth == 3 && n == 15 && rep < 40 {
                    let mut cs = out.candidates.clone();
                    let t2 = std::time::Instant::now();
                    gen::shortlist(&mut cs, 6);
                    shortlist_us.push(t2.elapsed().as_secs_f64() * 1e6);
                }
            }
            let mk = |k: Kind| by_kind[kind_slot(k)] as f64 / total;
            rows.push(json!({
                "balls": n,
                "balls_label": nlabel,
                "rail_depth": depth,
                "reps": REPS,
                "ms": stats_ms(&times),
                "constructed_total": constructed,
                "kept_total": kept,
                "after_dedup_total": dedup,
                "constructed_per_s": constructed as f64 / total,
                "after_dedup_per_s": dedup as f64 / total,
                "kept_per_s": kept as f64 / total,
                "per_class_per_s": {
                    "direct": mk(Kind::Direct),
                    "bank": mk(Kind::Bank),
                    "kick": mk(Kind::Kick),
                    "combo": mk(Kind::Combo),
                    "safety_escape": mk(Kind::SafetyEscape),
                    "safety_rollup": mk(Kind::SafetyRollUp),
                    "safety_twoway": mk(Kind::SafetyTwoWay),
                },
                "rail_depth_histogram": {
                    "bank_1": bank_rails[1], "bank_2": bank_rails[2], "bank_3": bank_rails[3],
                    "kick_1": kick_rails[1], "kick_2": kick_rails[2], "kick_3": kick_rails[3],
                },
                "counts_total": {
                    "direct": by_kind[0], "bank": by_kind[1], "kick": by_kind[2], "combo": by_kind[3],
                    "safety_escape": by_kind[4], "safety_rollup": by_kind[5], "safety_twoway": by_kind[6],
                },
            }));
        }
    }
    let v = json!({
        "section": "candidate generation throughput",
        "machine": machine_info(),
        "note": "One generation = one full decision's candidate set for the position. 'rail_depth' 0 = direct+combo+safety only; the rail chains are enumerated for banks and kicks at depth >= 1. Counts are pre-dedup 'kept' (post line-of-sight) and 'after_dedup' (post contact-point dedup).",
        "rows": rows,
        "shortlist_sort6_us": stats_ms(&shortlist_us),
    });
    println!("== candidate generation throughput ==");
    for r in v["rows"].as_array().unwrap() {
        println!(
            "{} depth={} ms={:.3} constructed/s={:.0} after_dedup/s={:.0} kept={}",
            r["balls_label"].as_str().unwrap(),
            r["rail_depth"],
            r["ms"]["mean"].as_f64().unwrap(),
            r["constructed_per_s"].as_f64().unwrap(),
            r["after_dedup_per_s"].as_f64().unwrap(),
            r["kept_total"],
        );
    }
    println!(
        "shortlist(sort+K=6) mean {:.1} us",
        v["shortlist_sort6_us"]["mean"].as_f64().unwrap()
    );
    write_json(dir, "gen-throughput.json", &v);
}

fn kind_slot(k: Kind) -> usize {
    match k {
        Kind::Direct => 0,
        Kind::Bank => 1,
        Kind::Kick => 2,
        Kind::Combo => 3,
        Kind::SafetyEscape => 4,
        Kind::SafetyRollUp => 5,
        Kind::SafetyTwoWay => 6,
    }
}

// ---------------------------------------------------------------- decision cost

fn measure_decision(dir: &PathBuf) {
    const CAPS: [usize; 7] = [0, 4, 8, 16, 24, 32, 64];
    let mut rows: Vec<Value> = Vec::new();

    // Raw sim unit cost, so the cap can be read against the real sim later.
    let mut sim_us: Vec<f64> = Vec::new();
    let mut sim_events: Vec<f64> = Vec::new();
    {
        let pos = positions::drill(7, 6, 8);
        let c = gen::generate(pos.cue, &pos.objs, &GenCfg::default());
        for (i, cand) in c.candidates.iter().enumerate().take(40) {
            let t = std::time::Instant::now();
            let s = toy_sim::strike(pos.cue, &pos.objs, cand.aim, cand.speed, Some(cand.ball), 4000);
            let dt = t.elapsed().as_secs_f64() * 1e6;
            sim_us.push(dt);
            sim_events.push(s.events as f64);
            let _ = i;
        }
    }

    for n in [3usize, 6] {
        for cap in CAPS {
            for micro in [true, false] {
                if !micro && cap != 64 {
                    continue;
                }
                let cfg = PlannerCfg {
                    work_cap: cap,
                    shortlist_k: 6,
                    micro,
                    ..PlannerCfg::default()
                };
                let mut times: Vec<f64> = Vec::new();
                let mut gen_t: Vec<f64> = Vec::new();
                let mut ver_t: Vec<f64> = Vec::new();
                let mut mic_t: Vec<f64> = Vec::new();
                let mut work: Vec<f64> = Vec::new();
                let mut pot = 0usize;
                let mut scratch = 0usize;
                let mut exec_pot = 0usize;
                let mut exec_scratch = 0usize;
                let mut cands: Vec<f64> = Vec::new();
                let reps = if n == 3 { 60 } else { 40 };
                for rep in 0..reps {
                    let pos = positions::drill(5000 + rep as u64, n, 8);
                    let d = planner::decide(pos.cue, &pos.objs, &cfg);
                    // Execute the committed declaration noise-free: the quality the cap bought.
                    if let Some(c) = d.chosen.as_ref() {
                        let out = toy_sim::strike(
                            pos.cue,
                            &pos.objs,
                            c.aim,
                            c.speed,
                            if c.kind.is_safety() { None } else { Some(c.ball) },
                            4000,
                        );
                        if out.target_potted {
                            exec_pot += 1;
                        }
                        if out.scratch {
                            exec_scratch += 1;
                        }
                    }
                    times.push(d.total_s * 1000.0);
                    gen_t.push(d.gen_s * 1000.0);
                    ver_t.push(d.verify_s * 1000.0);
                    mic_t.push(d.micro_s * 1000.0);
                    work.push(d.sim_evals as f64);
                    cands.push(d.candidates as f64);
                    if d.verified_pot {
                        pot += 1;
                    }
                    if d.scratch {
                        scratch += 1;
                    }
                }
                let p99 = stats_ms(&times)["p99"].as_f64().unwrap();
                rows.push(json!({
                    "balls": n,
                    "work_cap": cap,
                    "micro": micro,
                    "reps": reps,
                    "total_ms": stats_ms(&times),
                    "generate_ms": stats_ms(&gen_t),
                    "verify_ms": stats_ms(&ver_t),
                    "micro_ms": stats_ms(&mic_t),
                    "sim_evals_used": stats_ms(&work),
                    "candidates": stats_ms(&cands),
                    "verified_pot_rate": pot as f64 / reps as f64,
                    "executed_pot_rate": exec_pot as f64 / reps as f64,
                    "executed_scratch_rate": exec_scratch as f64 / reps as f64,
                    "scratch_rate": scratch as f64 / reps as f64,
                    "slo_p99_under_1000ms": p99 < 1000.0,
                    "decisions_per_s_mean": 1000.0 / stats_ms(&times)["mean"].as_f64().unwrap(),
                }));
            }
        }
    }

    let v = json!({
        "section": "per-decision cost (generate -> shortlist -> sim verify -> micro)",
        "machine": machine_info(),
        "note": "Wall clock on the prototype's toy sim. The real sim (#8) will be slower per shot; the cap is a work-unit count, so the SLO conversion needs the real sim's measured per-shot cost.",
        "toy_sim_cost_us": stats_ms(&sim_us),
        "toy_sim_events_per_shot": stats_ms(&sim_events),
        "rows": rows,
    });
    println!("== per-decision cost ==");
    println!(
        "toy sim: mean {:.1} us/shot, mean events {:.1}",
        v["toy_sim_cost_us"]["mean"].as_f64().unwrap(),
        v["toy_sim_events_per_shot"]["mean"].as_f64().unwrap()
    );
    for r in v["rows"].as_array().unwrap() {
        println!(
            "{} balls cap={:2} micro={:5} mean={:8.3} p99={:8.3} ms work={:.1} executed_pot={:.3} scratch={:.3}",
            r["balls"],
            r["work_cap"],
            r["micro"],
            r["total_ms"]["mean"].as_f64().unwrap(),
            r["total_ms"]["p99"].as_f64().unwrap(),
            r["sim_evals_used"]["mean"].as_f64().unwrap(),
            r["executed_pot_rate"].as_f64().unwrap(),
            r["executed_scratch_rate"].as_f64().unwrap(),
        );
    }
    write_json(dir, "decision-cost.json", &v);
}

// ---------------------------------------------------------------- eval calibration

fn measure_eval(dir: &PathBuf) {
    let base = PlannerCfg {
        work_cap: 24,
        shortlist_k: 6,
        micro: true,
        ..PlannerCfg::default()
    };

    // 1. Easy-pot suite: the anchor's rate at zero noise — the calibration for the >= 95% gate.
    let easy_n = 40;
    let mut easy_made = 0usize;
    let mut easy_scratch = 0usize;
    let mut easy_rows: Vec<Value> = Vec::new();
    let mut committed: Vec<(V2, Vec<Ob>, gen::Candidate)> = Vec::new();
    for seed in 0..easy_n {
        let pos = positions::easy_pot(9000 + seed as u64);
        let d = planner::decide(pos.cue, &pos.objs, &base);
        if let Some(c) = d.chosen.clone() {
            if d.verified_pot {
                easy_made += 1;
            }
            if d.scratch {
                easy_scratch += 1;
            }
            let kind = c.kind.as_str();
            committed.push((pos.cue, pos.objs.clone(), c));
            easy_rows.push(json!({
                "seed": 9000 + seed,
                "candidates": d.candidates,
                "sim_evals": d.sim_evals,
                "chosen_kind": kind,
                "pot": d.verified_pot,
                "scratch": d.scratch,
                "score": d.score,
                "total_ms": d.total_s * 1000.0,
            }));
        }
    }

    // 2. Class probes: forced execution of a sampled candidate per class, micro-refined.
    let mut class_rows: Vec<Value> = Vec::new();
    for cls in [
        Kind::Direct,
        Kind::Bank,
        Kind::Kick,
        Kind::Combo,
        Kind::SafetyEscape,
    ] {
        let mut tried = 0usize;
        let mut made = 0usize;
        let mut legal = 0usize;
        let mut scratch = 0usize;
        let mut leave_sum = 0.0f64;
        let mut cut_sum = 0.0f64;
        for seed in 0..16u64 {
            let pos = positions::drill(20000 + seed, 6, 8);
            // Untruncated: the class probe asks what the family can do, not what survives the
            // shortlist cut for one decision.
            let g = gen::generate(
                pos.cue,
                &pos.objs,
                &GenCfg {
                    max_cands: 4096,
                    ..GenCfg::default()
                },
            );
            let mut picks: Vec<&gen::Candidate> =
                g.candidates.iter().filter(|c| c.kind == cls).collect();
            picks.sort_by(|a, b| {
                b.makeability
                    .partial_cmp(&a.makeability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for c in picks.into_iter().take(3) {
                let r = planner::refine_one(pos.cue, &pos.objs, c, &base, 15);
                tried += 1;
                cut_sum += c.cut_cos;
                if r.pot {
                    made += 1;
                }
                if r.scratch {
                    scratch += 1;
                }
                // Legality for the toy: the strike made a first contact with a live ball.
                let out = toy_sim::strike(pos.cue, &pos.objs, r.cand.aim, r.cand.speed, None, 4000);
                if out.first_contact.is_some() {
                    legal += 1;
                }
                leave_sum += r.leave;
            }
        }
        class_rows.push(json!({
            "class": cls.as_str(),
            "tried": tried,
            "made": made,
            "success_rate": if tried > 0 { made as f64 / tried as f64 } else { 0.0 },
            "pot_rate": if tried > 0 { made as f64 / tried as f64 } else { 0.0 },
            "first_contact_rate": if tried > 0 { legal as f64 / tried as f64 } else { 0.0 },
            "scratch_rate": if tried > 0 { scratch as f64 / tried as f64 } else { 0.0 },
            "mean_cut_cos": if tried > 0 { cut_sum / tried as f64 } else { 0.0 },
            "mean_leave": if tried > 0 { leave_sum / tried as f64 } else { 0.0 },
        }));
    }

    // 3. Difficulty sigma sweep on the committed easy-pot strikes.
    let mut sigma_rows: Vec<Value> = Vec::new();
    for &s_aim in &[0.0f64, 0.0005, 0.001, 0.002, 0.004, 0.008, 0.016] {
        let cfg = PlannerCfg {
            noise_aim_sigma: s_aim,
            noise_speed_sigma: s_aim * 2.5,
            ..base
        };
        let mut made = 0usize;
        let mut runs = 0usize;
        let mut rng = SplitMix64::new(4242);
        for (cue, objs, c) in committed.iter() {
            for _ in 0..10 {
                let (aim, speed) = planner::perturb(c, &cfg, &mut rng);
                let out = toy_sim::strike(*cue, objs, aim, speed, Some(c.ball), 4000);
                runs += 1;
                if out.target_potted {
                    made += 1;
                }
            }
        }
        sigma_rows.push(json!({
            "sigma_aim_rad": s_aim,
            "sigma_aim_deg": s_aim * 180.0 / std::f64::consts::PI,
            "sigma_speed_rel": s_aim * 2.5,
            "trials": runs,
            "make_rate": if runs > 0 { made as f64 / runs as f64 } else { 0.0 },
        }));
    }

    // 4. Clearance drill: decide -> strike -> repeat, the toy env's rack analogue.
    let mut drill_rows: Vec<Value> = Vec::new();
    let mut rack_times: Vec<f64> = Vec::new();
    let mut total_decisions = 0usize;
    let mut total_decisions_s = 0.0f64;
    let mut cleared = 0usize;
    const DRILL_SEEDS: u64 = 30;
    for seed in 0..DRILL_SEEDS {
        let mut rng = SplitMix64::new(77 + seed);
        let mut pos = positions::drill(31000 + seed, 3, 8);
        let mut shots = 0usize;
        let t0 = std::time::Instant::now();
        let mut rng_decide = SplitMix64::new(31337 + seed);
        let _ = &mut rng;
        while shots < pos.max_shots && pos.live() > 0 {
            let d = planner::decide(pos.cue, &pos.objs, &base);
            total_decisions += 1;
            total_decisions_s += d.total_s;
            let Some(c) = d.chosen else { break };
            let (aim, speed) = planner::perturb(&c, &base, &mut rng_decide);
            let out = toy_sim::strike(pos.cue, &pos.objs, aim, speed, Some(c.ball), 4000);
            shots += 1;
            // Advance the position to the resulting rest state.
            pos.cue = out.rest[0].p;
            let mut idx = 1usize;
            for ob in pos.objs.iter_mut() {
                if ob.alive {
                    ob.p = out.rest[idx].p;
                    if !out.rest[idx].alive || out.potted_objs.contains(&ob.id) {
                        ob.alive = false;
                    }
                    idx += 1;
                }
            }
            if out.scratch {
                break;
            }
        }
        let dt = t0.elapsed().as_secs_f64();
        rack_times.push(dt);
        if pos.live() == 0 {
            cleared += 1;
        }
        drill_rows.push(json!({"seed": 31000 + seed, "shots": shots, "cleared": pos.live() == 0, "wall_s": dt}));
    }

    // 4b. Selection mix: what the planner actually chooses, and what the shortlist offered.
    //     This is the R/C evidence — if deeper families are never chosen, the cap is generous.
    let mut chosen_mix: Vec<Value> = Vec::new();
    let mut offer_mix: Vec<Value> = Vec::new();
    let mut n_sel = 0usize;
    {
        let mut chosen_counts = [0usize; 7];
        let mut offer_counts = [0usize; 7];
        for seed in 0..60u64 {
            let n = if seed % 2 == 0 { 3 } else { 6 };
            let pos = positions::drill(61000 + seed, n, 8);
            let g = gen::generate(pos.cue, &pos.objs, &GenCfg { max_cands: 4096, ..GenCfg::default() });
            // What the full generated set offers.
            for c in g.candidates.iter() {
                offer_counts[kind_slot(c.kind)] += 1;
            }
            let d = planner::decide(pos.cue, &pos.objs, &base);
            if let Some(c) = d.chosen.as_ref() {
                chosen_counts[kind_slot(c.kind)] += 1;
                n_sel += 1;
            }
        }
        for k in [
            Kind::Direct,
            Kind::Bank,
            Kind::Kick,
            Kind::Combo,
            Kind::SafetyEscape,
            Kind::SafetyRollUp,
            Kind::SafetyTwoWay,
        ] {
            chosen_mix.push(json!({
                "kind": k.as_str(),
                "chosen": chosen_counts[kind_slot(k)],
                "chosen_share": chosen_counts[kind_slot(k)] as f64 / n_sel.max(1) as f64,
            }));
            offer_mix.push(json!({
                "kind": k.as_str(),
                "offered_total": offer_counts[kind_slot(k)],
                "offered_per_position": offer_counts[kind_slot(k)] as f64 / 60.0,
            }));
        }
    }

    // 5. Determinism: identical inputs must give bit-identical decisions and rest states.
    let mut det_ok = true;
    let mut det_hashes: Vec<String> = Vec::new();
    for seed in 0..8u64 {
        let pos = positions::drill(41000 + seed, 3, 8);
        let a = planner::decide(pos.cue, &pos.objs, &base);
        let b = planner::decide(pos.cue, &pos.objs, &base);
        let ha = a.chosen.as_ref().map(|c| {
            toy_sim::state_hash(
                &toy_sim::strike(pos.cue, &pos.objs, c.aim, c.speed, Some(c.ball), 4000).rest,
            )
        });
        let hb = b.chosen.as_ref().map(|c| {
            toy_sim::state_hash(
                &toy_sim::strike(pos.cue, &pos.objs, c.aim, c.speed, Some(c.ball), 4000).rest,
            )
        });
        let same_bits = match (&a.chosen, &b.chosen) {
            (Some(x), Some(y)) => {
                x.aim.x.to_bits() == y.aim.x.to_bits()
                    && x.aim.y.to_bits() == y.aim.y.to_bits()
                    && x.speed.to_bits() == y.speed.to_bits()
                    && x.ball == y.ball
            }
            _ => false,
        };
        if !same_bits || ha != hb {
            det_ok = false;
        }
        det_hashes.push(format!("{:016x}/{:016x}", ha.unwrap_or(0), hb.unwrap_or(0)));
    }

    let v = json!({
        "section": "eval-bar calibration (toy env)",
        "machine": machine_info(),
        "note": "No rules layer and no real sim exist yet, so 'zero illegal declarations' is measured only as 'a first contact happened and the declaration named a live object ball'. A rack win rate against the scripted anchor needs an opponent and a rules engine and is NOT measurable here; the clearance drill stands in as the analogue.",
        "easy_pot_suite": {
            "positions": easy_n,
            "made": easy_made,
            "make_rate": easy_made as f64 / easy_n as f64,
            "scratch_rate": easy_scratch as f64 / easy_n as f64,
            "rows": easy_rows,
        },
        "class_probes": class_rows,
        "selection_mix": {"positions": 60, "chosen": chosen_mix, "offered": offer_mix},
        "sigma_sweep": sigma_rows,
        "clearance_drill": {
            "positions": DRILL_SEEDS,
            "cleared": cleared,
            "clearance_rate": cleared as f64 / DRILL_SEEDS as f64,
            "mean_wall_s_per_rack": rack_times.iter().sum::<f64>() / rack_times.len() as f64,
            "max_wall_s_per_rack": rack_times.iter().cloned().fold(0.0, f64::max),
            "decisions_total": total_decisions,
            "decisions_per_s": total_decisions as f64 / total_decisions_s,
            "rows": drill_rows,
        },
        "determinism": {
            "identical_decisions_and_hashes": det_ok,
            "rest_hashes": det_hashes,
        },
    });
    println!("== eval calibration ==");
    println!(
        "easy-pot: {}/{} = {:.3}",
        easy_made,
        easy_n,
        easy_made as f64 / easy_n as f64
    );
    for r in class_rows.iter() {
        println!(
            "class {:14} tried={:3} pot={:.3} first_contact={:.3} scratch={:.3} mean_cut={:.2} leave={:.3}",
            r["class"].as_str().unwrap(),
            r["tried"],
            r["pot_rate"].as_f64().unwrap(),
            r["first_contact_rate"].as_f64().unwrap(),
            r["scratch_rate"].as_f64().unwrap(),
            r["mean_cut_cos"].as_f64().unwrap(),
            r["mean_leave"].as_f64().unwrap()
        );
    }
    for r in sigma_rows.iter() {
        println!(
            "sigma_aim={:7.4} rad ({:5.3} deg) make_rate={:.3}",
            r["sigma_aim_rad"].as_f64().unwrap(),
            r["sigma_aim_deg"].as_f64().unwrap(),
            r["make_rate"].as_f64().unwrap()
        );
    }
    println!(
        "clearance: {}/{} = {:.3}, decisions/s={:.1}, mean rack {:.2}s",
        cleared,
        DRILL_SEEDS,
        cleared as f64 / DRILL_SEEDS as f64,
        total_decisions as f64 / total_decisions_s,
        rack_times.iter().sum::<f64>() / rack_times.len() as f64
    );
    println!("determinism ok: {det_ok}");
    write_json(dir, "eval-calibration.json", &v);
}

// ---------------------------------------------------------------- candidate dumps

fn candidate_json(c: &gen::Candidate) -> Value {
    json!({
        "kind": c.kind.as_str(),
        "ball": c.ball,
        "pocket": if c.pocket == usize::MAX { Value::Null } else { json!(c.pocket) },
        "aim": [c.aim.x, c.aim.y],
        "speed_mm_s": c.speed,
        "contact_point": [c.contact.x, c.contact.y],
        "rails": c.rails,
        "intermediates": c.inter,
        "cut_cos": c.cut_cos,
        "clearance_mm": c.clearance,
        "makeability": c.makeability,
        "seed_score": c.seed_score,
        "d_cue_contact": c.d_cue_contact,
        "d_obj_pocket": c.d_obj_pocket,
        "chain": c.chain[..c.chain_len as usize].to_vec(),
    })
}

fn measure_dump(dir: &PathBuf) {
    let cfg = PlannerCfg {
        work_cap: 24,
        shortlist_k: 8,
        micro: true,
        ..PlannerCfg::default()
    };
    let mut positions_json: Vec<Value> = Vec::new();
    for (tag, seed, n) in [
        ("drill-3ball", 5150u64, 3usize),
        ("drill-6ball", 5151, 6),
        ("easy-pot", 5152, 1),
    ] {
        let pos = if tag == "easy-pot" {
            positions::easy_pot(seed)
        } else {
            positions::drill(seed, n, 8)
        };
        let g = gen::generate(pos.cue, &pos.objs, &cfg.gen);
        let d = planner::decide(pos.cue, &pos.objs, &cfg);
        let mask = planner::mask(&g.candidates, &pos.objs);
        let ps = pockets();
        let mut top: Vec<&gen::Candidate> = g.candidates.iter().collect();
        top.sort_by(|a, b| {
            b.seed_score
                .partial_cmp(&a.seed_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top_rows: Vec<Value> = top
            .iter()
            .take(24)
            .map(|c| {
                let mut v = candidate_json(c);
                v["legal"] = json!(mask
                    .iter()
                    .zip(g.candidates.iter())
                    .position(|(m, x)| *m && std::ptr::eq(x, *c))
                    .is_some());
                v
            })
            .collect();
        let mut rows: Vec<Value> = Vec::new();
        for (i, r) in d.rows.iter().enumerate() {
            let full = &g.candidates[r.idx];
            let mut cj = candidate_json(full);
            cj["verified_pot"] = json!(r.pot);
            cj["verified_scratch"] = json!(r.scratch);
            cj["verified_leave"] = json!(r.leave);
            cj["score"] = json!(r.score);
            cj["rank"] = json!(i);
            cj["legal"] = json!(mask.get(r.idx).copied().unwrap_or(false));
            rows.push(cj);
        }
        positions_json.push(json!({
            "tag": tag,
            "seed": seed,
            "cue": [pos.cue.x, pos.cue.y],
            "balls": pos.objs.iter().map(|o| json!({"id": o.id, "p": [o.p.x, o.p.y], "alive": o.alive})).collect::<Vec<_>>(),
            "generated": g.candidates.len(),
            "generated_after_dedup": g.stats.after_dedup,
            "constructed": g.stats.constructed,
            "rows": rows,
            "top_unverified": top_rows,
            "chosen": d.chosen.as_ref().map(candidate_json),
            "refined": d.refined.as_ref().map(|r| json!({
                "kind": r.kind.as_str(), "aim": [r.aim.x, r.aim.y], "speed_mm_s": r.speed,
                "verified_pot": r.pot, "verified_scratch": r.scratch, "verified_leave": r.leave, "score": r.score,
            })),
            "decision": {
                "score": d.score, "sim_evals": d.sim_evals, "candidates": d.candidates,
                "gen_ms": d.gen_s * 1000.0, "verify_ms": d.verify_s * 1000.0, "micro_ms": d.micro_s * 1000.0,
                "total_ms": d.total_s * 1000.0, "verified_pot": d.verified_pot, "scratch": d.scratch,
            },
        }));
        let _ = ps;
    }
    let v = json!({
        "section": "candidate-set debug dump",
        "machine": machine_info(),
        "note": "Per-candidate scores for a handful of positions (#9 §8's debug view). 'score' is the planner's decision score after sim verification; 'seed_score'/'makeability' are the analytic pre-sim estimates. Rows are the verified shortlist in rank order; the full generated set is counted but not listed.",
        "encoding": {
            "obs_dim": OBS_DIM,
            "cand_dim": CAND_DIM,
            "cand_feature_order": [
                "kind_pot", "kind_safety", "kind_bank", "kind_kick", "kind_combo",
                "cut_cos", "cut_angle_over_pi", "d_cue_contact_over_L", "d_obj_pocket_over_L",
                "clearance_over_4R", "rails_over_3", "intermediates",
                "makeability", "leave", "contact_x_over_hl", "contact_y_over_hw"
            ],
        },
        "encoding_sample": encoding_sample(),
        "positions": positions_json,
    });
    println!("== candidate dump ==");
    for p in v["positions"].as_array().unwrap() {
        println!(
            "{}: generated={} rows={} chosen={} score={:.3}",
            p["tag"].as_str().unwrap(),
            p["generated"],
            p["rows"].as_array().unwrap().len(),
            p["chosen"]["kind"].as_str().unwrap_or("-"),
            p["decision"]["score"].as_f64().unwrap()
        );
    }
    write_json(dir, "candidate-dump.json", &v);
}

/// A real encoded sample, so the observation/candidate layout can be eyeballed across languages.
fn encoding_sample() -> Value {
    let pos = positions::drill(5150, 3, 8);
    let g = gen::generate(pos.cue, &pos.objs, &GenCfg::default());
    let mask = planner::mask(&g.candidates, &pos.objs);
    let m = planner::mask(&g.candidates, &pos.objs);
    let obs = encode_obs(
        pos.cue,
        &pos.objs,
        &ObsCtx {
            shot_index: 0,
            max_shots: 8,
            n_initial: 3,
        },
        &g.candidates,
        &m,
    );
    let ps = pockets();
    let first = g.candidates.first().map(|c| encode_cand(c, &ps));
    json!({
        "obs_dim": OBS_DIM,
        "cand_dim": CAND_DIM,
        "obs_len": obs.len(),
        "obs": obs,
        "cand_tensor_len": encode_cands(&g.candidates).len(),
        "k_live": mask.iter().filter(|x| **x).count(),
        "first_cand_row": first.map(|f| f.to_vec()),
    })
}

// ---------------------------------------------------------------- observation cross-check

/// Cross-language check of the 64-float observation layout: rebuild the observation in Rust from
/// the positions the Python env recorded, and compare the indices that do not depend on the
/// candidate set (53..56 are candidate-derived and are expected to differ here).
fn measure_fixture(dir: &PathBuf, args: &[String]) {
    let mut path = dir.join("obs-fixture.json").display().to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--fixture" {
            if let Some(v) = it.next() {
                path = v.clone();
            }
        }
    }
    let Ok(txt) = std::fs::read_to_string(&path) else {
        println!("obs fixture: {path} not found — skipped (pass --fixture <path>)");
        return;
    };
    let v: Value = serde_json::from_str(&txt).expect("parse obs fixture");
    let cue = V2::new(
        v["cue"][0].as_f64().unwrap(),
        v["cue"][1].as_f64().unwrap(),
    );
    let mut objs: Vec<Ob> = Vec::new();
    for (i, slot) in v["balls"].as_array().unwrap().iter().enumerate() {
        if i == 0 {
            continue; // slot 0 is the cue ball
        }
        let present = slot[2].as_f64().unwrap_or(0.0) > 0.5;
        if present {
            objs.push(Ob {
                id: i,
                p: V2::new(slot[0].as_f64().unwrap(), slot[1].as_f64().unwrap()),
                alive: true,
            });
        }
    }
    let max_shots = v["max_shots"].as_u64().unwrap_or(8) as usize;
    let shot_index = v["shot_index"].as_u64().unwrap_or(0) as usize;
    let n_initial = objs.len();
    let obs_rust = encode_obs(
        cue,
        &objs,
        &ObsCtx {
            shot_index,
            max_shots,
            n_initial,
        },
        &[],
        &[],
    );
    let obs_py: Vec<f64> = v["obs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    let compared: Vec<usize> = (0..3)
        .chain(48..53)
        .chain(std::iter::once(57))
        .chain(58..64)
        .collect();
    let mut max_diff = 0.0f64;
    let mut worst = 0usize;
    let mut mismatches: Vec<Value> = Vec::new();
    for &i in compared.iter() {
        let d = (f64::from(obs_rust[i]) - obs_py[i]).abs();
        if d > max_diff {
            max_diff = d;
            worst = i;
        }
        if d > 1e-6 {
            mismatches.push(json!({"index": i, "rust": obs_rust[i], "python": obs_py[i]}));
        }
    }
    let v_out = json!({
        "section": "observation layout cross-check (Rust encoder vs Python env)",
        "machine": machine_info(),
        "fixture": path,
        "balls_present": n_initial,
        "compared_indices": compared,
        "skipped_indices": [53, 54, 55, 56],
        "skipped_reason": "candidate-derived: this check feeds no candidate set, so those four differ by construction",
        "max_abs_diff": max_diff,
        "worst_index": worst,
        "mismatches": mismatches,
        "pass": max_diff <= 1e-6,
        "obs_rust": obs_rust,
        "obs_python": obs_py,
    });
    println!(
        "obs fixture: {} indices compared, max abs diff {:.3e} (index {}), pass={}",
        compared.len(),
        max_diff,
        worst,
        v_out["pass"]
    );
    if !mismatches.is_empty() {
        println!("mismatches: {}", serde_json::to_string(&mismatches).unwrap());
    }
    write_json(dir, "obs-fixture-check.json", &v_out);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).cloned().unwrap_or_else(|| "all".to_string());
    let dir = out_dir(&args);
    match cmd.as_str() {
        "gen" => measure_gen(&dir),
        "decision" => measure_decision(&dir),
        "eval" => measure_eval(&dir),
        "dump" => measure_dump(&dir),
        "fixture" => measure_fixture(&dir, &args),
        "all" => {
            measure_gen(&dir);
            measure_decision(&dir);
            measure_eval(&dir);
            measure_dump(&dir);
            measure_fixture(&dir, &args);
        }
        other => eprintln!("unknown subcommand {other}"),
    }
}
