//! Serve path: load the exported smoke policy and score candidates per decision.
//!
//! One binary, two builds (#9 §8's `OnnxPolicy` behind a feature and the `ScriptedPolicy`
//! fallback):
//!
//!   cargo build --release              -> scripted fallback (no ONNX Runtime linked)
//!   cargo build --release --features onnx -> ORT policy
//!
//! The binary-size delta and the per-decision latency distribution are measured between exactly
//! these two builds, running the same workload.
//!
//! Usage:
//!   serve --positions 300 --json results/serve-latency.json
//!   serve --model <policy.onnx> --golden <onnx-golden.json> --json results/serve-latency.json

use std::path::PathBuf;
use std::time::Instant;

use ai_spine_proto::encode::{encode_cands, encode_obs, ObsCtx};
use ai_spine_proto::gen::{self, Candidate, GenCfg};
use ai_spine_proto::planner::{self, PlannerCfg};
use ai_spine_proto::positions;
use serde_json::{json, Value};

#[cfg(feature = "onnx")]
fn logits_onnx(
    session: &mut ort::session::Session,
    obs: &[f32],
    cand: &[f32],
    k: usize,
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    use ort::value::Tensor;
    let obs_t = Tensor::from_array(([1usize, obs.len()], obs.to_vec()))?;
    let cand_t = Tensor::from_array(([1usize, k, 16usize], cand.to_vec()))?;
    let outputs = session.run(ort::inputs!["obs" => obs_t, "cand" => cand_t])?;
    let (_shape, data) = outputs["logits"].try_extract_tensor::<f32>()?;
    Ok(data.to_vec())
}

/// The scripted fallback: the analytic seed score is already a per-candidate logit.
fn logits_fallback(cands: &[Candidate]) -> Vec<f32> {
    cands.iter().map(|c| c.seed_score as f32).collect()
}

fn argmax_masked(logits: &[f32], mask: &[bool]) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (i, l) in logits.iter().enumerate() {
        if !mask.get(i).copied().unwrap_or(false) {
            continue;
        }
        if best.map(|(b, _)| *l > b).unwrap_or(true) {
            best = Some((*l, i));
        }
    }
    best.map(|(_, i)| i)
}

fn exe_size() -> u64 {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0)
}

fn sha256(path: &str) -> String {
    std::process::Command::new("shasum")
        .args(["-a", "256", path])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_default()
}

fn stat(v: &[f64]) -> Value {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let g = |p: f64| -> f64 {
        if s.is_empty() {
            0.0
        } else {
            let i = ((p / 100.0) * (s.len() as f64 - 1.0)).round() as usize;
            s[i.min(s.len() - 1)]
        }
    };
    json!({
        "n": s.len(),
        "mean": if s.is_empty() { 0.0 } else { s.iter().sum::<f64>() / s.len() as f64 },
        "min": s.first().copied().unwrap_or(0.0),
        "p50": g(50.0), "p90": g(90.0), "p99": g(99.0), "max": s.last().copied().unwrap_or(0.0),
        "note": "min is the least-contended sample: this box was shared with other agent workloads, so min/p50 are the useful figures and p99/max are upper bounds",
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut model: Option<String> = None;
    let mut golden: Option<String> = None;
    let mut json_out: Option<String> = None;
    let mut n_positions = 300usize;
    let mut n_balls = 3usize;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--model" => model = it.next().cloned(),
            "--golden" => golden = it.next().cloned(),
            "--json" => json_out = it.next().cloned(),
            "--positions" => n_positions = it.next().and_then(|v| v.parse().ok()).unwrap_or(300),
            "--balls" => n_balls = it.next().and_then(|v| v.parse().ok()).unwrap_or(3),
            _ => {}
        }
    }

    let feature_on = cfg!(feature = "onnx");
    println!(
        "serve: onnx feature {} | binary {} bytes",
        if feature_on { "ON" } else { "OFF" },
        exe_size()
    );

    #[cfg(feature = "onnx")]
    let mut session: Option<ort::session::Session> = None;
    #[allow(unused_mut)]
    let mut load_ms = 0.0f64;
    #[cfg(feature = "onnx")]
    {
        match model.as_deref() {
            Some(p) => {
                let t = Instant::now();
                let built = ort::session::Session::builder()
                    .map_err(|e| e.to_string())
                    .and_then(|b| b.with_intra_threads(1).map_err(|e| e.to_string()));
                match built {
                    Ok(mut builder) => match builder.commit_from_file(p).map_err(|e| e.to_string()) {
                        Ok(s) => {
                            load_ms = t.elapsed().as_secs_f64() * 1000.0;
                            session = Some(s);
                            println!("loaded {p} in {load_ms:.1} ms");
                        }
                        Err(e) => {
                            eprintln!("FAILED to load {p}: {e} — scripted fallback");
                        }
                    },
                    Err(e) => {
                        eprintln!("FAILED to configure the ORT session: {e} — scripted fallback");
                    }
                }
            }
            None => println!("no --model: scripted fallback"),
        }
    }
    #[cfg(not(feature = "onnx"))]
    {
        if model.is_some() {
            eprintln!("--model ignored: built without the `onnx` feature (scripted fallback)");
        }
    }

    // Golden-vector check: does ORT in this process reproduce the Python logits?
    #[allow(unused_mut)]
    let mut golden_result: Option<Value> = None;
    if let Some(g) = golden.as_deref() {
        let txt = std::fs::read_to_string(g).expect("read golden json");
        let v: Value = serde_json::from_str(&txt).expect("parse golden json");
        let obs: Vec<f32> = v["obs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        let k = v["cand"].as_array().unwrap().len();
        let mut cand: Vec<f32> = Vec::with_capacity(k * 16);
        for row in v["cand"].as_array().unwrap() {
            for x in row.as_array().unwrap() {
                cand.push(x.as_f64().unwrap() as f32);
            }
        }
        let expected: Vec<f32> = v["expected_logits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        #[cfg(feature = "onnx")]
        if let Some(s) = session.as_mut() {
            match logits_onnx(s, &obs, &cand, k) {
                Ok(got) => {
                    let max_err = got
                        .iter()
                        .zip(expected.iter())
                        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
                        .fold(0.0f64, f64::max);
                    golden_result = Some(json!({
                        "k": k,
                        "max_abs_err_ort_vs_python": max_err,
                        "logits_ort": got,
                        "logits_python": expected,
                        "pass": max_err < 1e-5,
                    }));
                    println!("golden check: k={k} max_abs_err={max_err:.3e}");
                }
                Err(e) => eprintln!("golden run failed: {e}"),
            }
        }
        #[cfg(not(feature = "onnx"))]
        {
            let _ = (obs, cand, expected, k);
            println!("golden check skipped: no onnx feature");
        }
    }

    // Per-decision workload: generate -> mask -> encode -> policy -> select.
    let gen_cfg = GenCfg::default();
    let plan_cfg = PlannerCfg::default();
    let mut total: Vec<f64> = Vec::new();
    let mut gen_t: Vec<f64> = Vec::new();
    let mut enc_t: Vec<f64> = Vec::new();
    let mut inf_t: Vec<f64> = Vec::new();
    let mut sel_t: Vec<f64> = Vec::new();
    let mut ks: Vec<f64> = Vec::new();
    let mut policy_calls = 0usize;
    let mut picked = 0usize;

    for i in 0..n_positions {
        let pos = positions::drill(70000 + i as u64, n_balls, 8);
        let t0 = Instant::now();
        let g = gen::generate(pos.cue, &pos.objs, &gen_cfg);
        gen_t.push(t0.elapsed().as_secs_f64() * 1000.0);
        let m = planner::mask(&g.candidates, &pos.objs);
        let t1 = Instant::now();
        let obs = encode_obs(
            pos.cue,
            &pos.objs,
            &ObsCtx {
                shot_index: 0,
                max_shots: 8,
                n_initial: n_balls,
            },
            &g.candidates,
            &m,
        );
        let cand = encode_cands(&g.candidates);
        enc_t.push(t1.elapsed().as_secs_f64() * 1000.0);

        let k = g.candidates.len();
        if k == 0 {
            continue;
        }
        ks.push(k as f64);
        let t2 = Instant::now();
        #[cfg(feature = "onnx")]
        let logits = match session.as_mut() {
            Some(s) => match logits_onnx(s, &obs, &cand, k) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("inference failed: {e} — scripted fallback for this position");
                    logits_fallback(&g.candidates)
                }
            },
            None => logits_fallback(&g.candidates),
        };
        #[cfg(not(feature = "onnx"))]
        let logits = {
            let _ = (&obs, &cand); // encoded either way: encoding cost is part of the workload
            logits_fallback(&g.candidates)
        };
        inf_t.push(t2.elapsed().as_secs_f64() * 1000.0);
        policy_calls += 1;
        let _ = &plan_cfg;

        let t3 = Instant::now();
        if argmax_masked(&logits, &m).is_some() {
            picked += 1;
        }
        sel_t.push(t3.elapsed().as_secs_f64() * 1000.0);
        total.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let model_info = model.as_deref().map(|p| {
        json!({
            "path": p,
            "size_bytes": std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            "sha256": sha256(p),
            "load_ms": load_ms,
        })
    });

    let out = json!({
        "section": "serve path: per-decision latency and binary size",
        "machine": machine_info(),
        "onnx_feature": feature_on,
        "binary_size_bytes": exe_size(),
        "binary_path": std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        "model": model_info,
        "golden": golden_result,
        "workload": {
            "positions": policy_calls,
            "balls": n_balls,
            "candidates_per_decision": stat(&ks),
            "note": "generate -> mask -> encode -> policy forward -> masked argmax. The sim-verified shortlist and the micro are NOT in this loop; they are measured by the `measure decision` harness.",
        },
        "latency_ms": {
            "total": stat(&total),
            "generate": stat(&gen_t),
            "encode": stat(&enc_t),
            "inference": stat(&inf_t),
            "select": stat(&sel_t),
        },
    });

    println!(
        "decisions {} | total ms min {:.3} p50 {:.3} p99 {:.3} max {:.3} | infer min {:.4} p50 {:.3} p99 {:.3} | candidates/decision mean {:.1}",
        policy_calls,
        out["latency_ms"]["total"]["min"].as_f64().unwrap(),
        out["latency_ms"]["total"]["p50"].as_f64().unwrap(),
        out["latency_ms"]["total"]["p99"].as_f64().unwrap(),
        out["latency_ms"]["total"]["max"].as_f64().unwrap(),
        out["latency_ms"]["inference"]["min"].as_f64().unwrap(),
        out["latency_ms"]["inference"]["p50"].as_f64().unwrap(),
        out["latency_ms"]["inference"]["p99"].as_f64().unwrap(),
        out["workload"]["candidates_per_decision"]["mean"].as_f64().unwrap(),
    );
    println!("picked a candidate in {picked}/{policy_calls} decisions");

    if let Some(p) = json_out {
        let pb = PathBuf::from(&p);
        if let Some(parent) = pb.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&pb, serde_json::to_string_pretty(&out).unwrap()).unwrap();
        println!("wrote {p}");
    }
}

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
    json!({
        "cpu": sh("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "ncpu": sh("sysctl", &["-n", "hw.ncpu"]).parse::<u64>().unwrap_or(0),
        "mem_bytes": sh("sysctl", &["-n", "hw.memsize"]).parse::<u64>().unwrap_or(0),
        "os": format!("macOS {}", sh("sw_vers", &["-productVersion"])),
        "arch": std::env::consts::ARCH,
    })
}
