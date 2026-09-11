//! Observation and candidate encoding — the fixed layout the Python env and the Rust side share.
//!
//! Layout contract (agreed with the Python half of the prototype; see `docs/spec/ai-constants.md`):
//! `obs` is 64 floats — 16 canonical slots × (x, y, present) at indices `3*i..3*i+3` (slot 0 = cue,
//! slots 1..=15 = balls 1..=15) plus 16 context scalars at 48..64. `cand` is one row of 16 floats
//! per candidate. Everything is normalized to the spec frame; nothing is quantized.

use crate::gen::Candidate;
use crate::geom::{nearest_pocket_dist, pockets, Ob, Pocket, V2, HL, HW, L};

pub const OBS_DIM: usize = 64;
pub const CAND_DIM: usize = 16;
pub const K_MAX: usize = 32;

pub struct ObsCtx {
    pub shot_index: usize,
    pub max_shots: usize,
    pub n_initial: usize,
}

pub fn encode_obs(cue: V2, objs: &[Ob], ctx: &ObsCtx, cands: &[Candidate], mask: &[bool]) -> Vec<f32> {
    let mut o = vec![0.0f32; OBS_DIM];
    // Slot 0: cue ball (always present in this prototype).
    o[0] = (cue.x / HL) as f32;
    o[1] = (cue.y / HW) as f32;
    o[2] = 1.0;
    let mut live: Vec<&Ob> = Vec::new();
    for ob in objs {
        if !ob.alive {
            continue;
        }
        live.push(ob);
        if ob.id >= 1 && ob.id <= 15 {
            let i = ob.id;
            o[3 * i] = (ob.p.x / HL) as f32;
            o[3 * i + 1] = (ob.p.y / HW) as f32;
            o[3 * i + 2] = 1.0;
        }
    }
    let ps = pockets();
    o[48] = if ctx.max_shots == 0 {
        0.0
    } else {
        (ctx.shot_index as f64 / ctx.max_shots as f64) as f32
    };
    o[49] = if ctx.n_initial == 0 {
        0.0
    } else {
        (live.len() as f64 / ctx.n_initial as f64) as f32
    };
    o[50] = 1.0;
    o[51] = if live.is_empty() {
        0.0
    } else {
        (live
            .iter()
            .map(|ob| nearest_pocket_dist(ob.p, &ps))
            .sum::<f64>()
            / live.len() as f64
            / (L / 2.0)) as f32
    };
    // Cut feasibility proxy: per live ball the best pocket line, then the worst of those — how
    // badly the worst ball is placed for a direct pot.
    let mut worst_best_cut = 1.0f64;
    let mut any = false;
    for ob in live.iter() {
        let Some(to_ob) = (ob.p - cue).unit() else {
            continue;
        };
        let mut best_here = -1.0f64;
        for p in ps.iter() {
            let Some(to_pk) = (p.p - ob.p).unit() else {
                continue;
            };
            let c = to_ob.dot(to_pk);
            if c > best_here {
                best_here = c;
            }
        }
        if best_here > -1.0 {
            any = true;
            if best_here < worst_best_cut {
                worst_best_cut = best_here;
            }
        }
    }
    o[52] = if any { worst_best_cut as f32 } else { -1.0 };
    let legal = mask.iter().filter(|m| **m).count();
    o[53] = if cands
        .iter()
        .zip(mask.iter())
        .any(|(c, m)| *m && c.kind == crate::gen::Kind::Direct)
    {
        1.0
    } else {
        0.0
    };
    o[54] = (legal as f64 / K_MAX as f64).min(1.0) as f32;
    o[55] = if cands.is_empty() {
        0.0
    } else {
        (cands.iter().map(|c| c.makeability).sum::<f64>() / cands.len() as f64) as f32
    };
    o[56] = cands
        .iter()
        .map(|c| c.makeability)
        .fold(0.0f64, f64::max) as f32;
    o[57] = if ctx.max_shots > 0 && ctx.shot_index * 3 >= ctx.max_shots * 2 {
        1.0
    } else {
        0.0
    };
    // 58..61 reserved (ball-in-hand domain, group state, on-8, score differential) — constants in
    // the prototype. 62: case = open table. 63: fouls so far / 3.
    o[62] = 1.0;
    o
}

pub fn encode_cand(c: &Candidate, ps: &[Pocket]) -> [f32; CAND_DIM] {
    let mut f = [0.0f32; CAND_DIM];
    use crate::gen::Kind::*;
    f[0] = matches!(c.kind, Direct | Bank | Kick | Combo) as i32 as f32;
    f[1] = c.kind.is_safety() as i32 as f32;
    f[2] = (c.kind == Bank) as i32 as f32;
    f[3] = (c.kind == Kick) as i32 as f32;
    f[4] = (c.kind == Combo) as i32 as f32;
    f[5] = c.cut_cos as f32;
    f[6] = (c.cut_cos.clamp(-1.0, 1.0).acos() / std::f64::consts::PI) as f32;
    f[7] = (c.d_cue_contact / L) as f32;
    f[8] = (c.d_obj_pocket / L) as f32;
    f[9] = (c.clearance / (4.0 * crate::geom::R)).clamp(0.0, 1.0) as f32;
    f[10] = (f64::from(c.rails) / 3.0) as f32;
    f[11] = f64::from(c.inter) as f32;
    f[12] = c.makeability as f32;
    f[13] = c.leave as f32;
    f[14] = (c.contact.x / HL) as f32;
    f[15] = (c.contact.y / HW) as f32;
    let _ = ps;
    f
}

/// Flattened candidate tensor `[K, 16]` for the policy.
pub fn encode_cands(cands: &[Candidate]) -> Vec<f32> {
    let ps = pockets();
    let mut out = Vec::with_capacity(cands.len() * CAND_DIM);
    for c in cands {
        out.extend_from_slice(&encode_cand(c, &ps));
    }
    out
}
