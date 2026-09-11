//! Raster rendering: a tiny RGB canvas plus a dependency-free PNG writer
//! (stored deflate blocks), and the top-down table/trajectory drawing the
//! visual artefacts need.

use crate::consts::*;
use crate::font;
use crate::sim::BallState;
use crate::table::Table;
use crate::vec::{v3, V3};

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>, // palette indices
}

/// Palette-indexed colour: `Rgb` is the palette slot. Frames ship as indexed
/// PNGs so a 560x360 render stays a few tens of KB instead of ~600 KB.
#[derive(Clone, Copy)]
pub struct Rgb(pub u8);

pub const PALETTE: [[u8; 3]; 32] = [
    [28, 28, 34],    // 0 background
    [22, 84, 48],    // 1 cloth
    [122, 80, 42],   // 2 cushion
    [74, 46, 24],    // 3 rail
    [8, 8, 8],       // 4 pocket
    [245, 245, 245], // 5 white
    [16, 16, 16],    // 6 black
    [255, 230, 60],  // 7 cue path
    [206, 206, 206], // 8 path
    [255, 60, 60],   // 9 highlight
    [184, 184, 190], // 10 dim text
    [240, 208, 40],  // 11 ball 1/9
    [30, 70, 200],   // 12 ball 2/10
    [210, 40, 40],   // 13 ball 3/11
    [120, 60, 170],  // 14 ball 4/12
    [235, 130, 30],  // 15 ball 5/13
    [30, 140, 70],   // 16 ball 6/14
    [150, 40, 45],   // 17 ball 7/15
    [20, 20, 20],    // 18 ball 8
    [90, 90, 96],    // 19 pocketed marker
    [40, 40, 46],    // 20 spare
    [60, 60, 66],    // 21
    [140, 140, 146], // 22
    [255, 255, 255], // 23
    [80, 120, 90],   // 24
    [200, 160, 60],  // 25
    [0, 0, 0],       // 26
    [0, 0, 0],       // 27
    [0, 0, 0],       // 28
    [0, 0, 0],       // 29
    [0, 0, 0],       // 30
    [0, 0, 0],       // 31
];

pub const CLOTH: Rgb = Rgb(1);
pub const CUSHION: Rgb = Rgb(2);
pub const RAIL: Rgb = Rgb(3);
pub const POCKET: Rgb = Rgb(4);
pub const WHITE: Rgb = Rgb(5);
pub const BLACK: Rgb = Rgb(6);
pub const CUE_PATH: Rgb = Rgb(7);
pub const PATH: Rgb = Rgb(8);
pub const HILITE: Rgb = Rgb(9);
pub const DIMMED: Rgb = Rgb(10);
pub const POCKETED: Rgb = Rgb(19);

impl Canvas {
    pub fn new(w: usize, h: usize, bg: Rgb) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![bg.0; w * h],
        }
    }
    pub fn put(&mut self, x: i64, y: i64, c: Rgb) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        self.px[(y as usize) * self.w + x as usize] = c.0;
    }
    pub fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, c: Rgb) {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let mut y = y0.round() as i64;
        while (y as f64) <= y1 {
            let mut x = x0.round() as i64;
            while (x as f64) <= x1 {
                self.put(x, y, c);
                x += 1;
            }
            y += 1;
        }
    }
    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, c: Rgb) {
        let r2 = r * r;
        let mut y = (cy - r).floor() as i64;
        while (y as f64) <= cy + r {
            let mut x = (cx - r).floor() as i64;
            while (x as f64) <= cx + r {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                if dx * dx + dy * dy <= r2 {
                    self.put(x, y, c);
                }
                x += 1;
            }
            y += 1;
        }
    }
    pub fn ring(&mut self, cx: f64, cy: f64, r: f64, w: f64, c: Rgb) {
        let n = ((2.0 * std::f64::consts::PI * r) as i64).max(24);
        for i in 0..n {
            let t = i as f64 / n as f64 * 2.0 * std::f64::consts::PI;
            let x = cx + r * t.cos();
            let y = cy + r * t.sin();
            self.disc(x, y, w, c);
        }
    }
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64), c: Rgb, thick: f64) {
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let len = (dx * dx + dy * dy).sqrt();
        let n = (len * 2.0).ceil().max(1.0) as i64;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let x = a.0 + dx * t;
            let y = a.1 + dy * t;
            if thick <= 1.0 {
                self.put(x.round() as i64, y.round() as i64, c);
            } else {
                self.disc(x, y, thick * 0.5, c);
            }
        }
    }
    pub fn text(&mut self, s: &str, x: usize, y: usize, scale: usize, c: Rgb) {
        font::draw(s, x, y, scale, |px, py| {
            self.put(px as i64, py as i64, c);
        });
    }

    /// Dependency-free indexed PNG: stored (uncompressed) deflate blocks.
    pub fn write_png(&self, path: &str) -> std::io::Result<()> {
        let mut raw: Vec<u8> = Vec::with_capacity(self.h * (1 + self.w));
        let mut prev_row: Vec<u8> = vec![0u8; self.w];
        for y in 0..self.h {
            let row = &self.px[y * self.w..(y + 1) * self.w];
            // Up filter (2) then Sub (1) alternatives: pick Sub for rows with
            // long horizontal runs of one colour -- enough to shrink the
            // stored-deflate stream on a table render.
            let mut diff = Vec::with_capacity(self.w);
            for (i, v) in row.iter().enumerate() {
                let left = if i == 0 { 0 } else { row[i - 1] };
                diff.push(v.wrapping_sub(left));
            }
            let cost_sub: usize = diff.iter().filter(|d| **d != 0).count();
            let cost_raw: usize = row.iter().enumerate().filter(|(i, v)| **v != prev_row[*i]).count();
            if cost_sub <= cost_raw {
                raw.push(1u8);
                raw.extend_from_slice(&diff);
            } else {
                raw.push(0u8);
                raw.extend_from_slice(row);
            }
            prev_row.copy_from_slice(row);
        }
        let mut z: Vec<u8> = Vec::with_capacity(raw.len() + raw.len() / 60000 * 5 + 16);
        z.push(0x78);
        z.push(0x01);
        let mut i = 0usize;
        while i < raw.len() {
            let n = (raw.len() - i).min(65535);
            let last = i + n >= raw.len();
            z.push(if last { 1 } else { 0 });
            z.push((n & 0xff) as u8);
            z.push(((n >> 8) & 0xff) as u8);
            z.push((!n & 0xff) as u8);
            z.push(((!n >> 8) & 0xff) as u8);
            z.extend_from_slice(&raw[i..i + n]);
            i += n;
        }
        let a = adler32(&raw);
        z.push((a >> 24) as u8);
        z.push((a >> 16) as u8);
        z.push((a >> 8) as u8);
        z.push(a as u8);

        let mut out: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut ihdr: Vec<u8> = Vec::new();
        ihdr.extend_from_slice(&(self.w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 3, 0, 0, 0]); // 8-bit, indexed colour
        chunk(&mut out, b"IHDR", &ihdr);
        let mut plte: Vec<u8> = Vec::with_capacity(PALETTE.len() * 3);
        for c in PALETTE.iter() {
            plte.extend_from_slice(c);
        }
        chunk(&mut out, b"PLTE", &plte);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        std::fs::write(path, out)
    }
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &d in data {
        a = (a + d as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &d in data {
        c = table[((c ^ d as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Top-down view mapping: millimetres to pixels.
pub struct View {
    pub scale: f64,
    pub x0: f64,
    pub y0: f64,
}

impl View {
    pub fn new(canvas_w: usize, canvas_h: usize, top: f64, side: f64) -> View {
        let sx = (canvas_w as f64 - 2.0 * side) / TABLE_LEN;
        let sy = (canvas_h as f64 - top - side) / TABLE_WID;
        let scale = sx.min(sy);
        let x0 = (canvas_w as f64 - TABLE_LEN * scale) / 2.0;
        let y0 = top + ((canvas_h as f64 - top - side) - TABLE_WID * scale) / 2.0;
        View { scale, x0, y0 }
    }
    pub fn px(&self, p: V3) -> (f64, f64) {
        (
            self.x0 + (p.x + HALF_LEN) * self.scale,
            self.y0 + (HALF_WID - p.y) * self.scale,
        )
    }
    pub fn mm_per_px(&self) -> f64 {
        1.0 / self.scale
    }
}

fn ball_colour(id: u8) -> Rgb {
    match id {
        0 => WHITE,
        1 | 9 => Rgb(11),
        2 | 10 => Rgb(12),
        3 | 11 => Rgb(13),
        4 | 12 => Rgb(14),
        5 | 13 => Rgb(15),
        6 | 14 => Rgb(16),
        7 | 15 => Rgb(17),
        _ => Rgb(18),
    }
}

fn ball_colour_dark(id: u8) -> bool {
    matches!(id, 4 | 8 | 12 | 2 | 10)
}

pub fn draw_table(c: &mut Canvas, table: &Table, view: &View) {
    // cloth
    let tl = view.px(v3(-HALF_LEN, HALF_WID, 0.0));
    let br = view.px(v3(HALF_LEN, -HALF_WID, 0.0));
    c.rect(tl.0 - 1.0, tl.1 - 1.0, br.0 + 1.0, br.1 + 1.0, CLOTH);
    // rail frame
    let m = 14.0;
    c.rect(tl.0 - m, tl.1 - m, br.0 + m, tl.1, RAIL);
    c.rect(tl.0 - m, br.1, br.0 + m, br.1 + m, RAIL);
    c.rect(tl.0 - m, tl.1 - m, tl.0, br.1 + m, RAIL);
    c.rect(br.0, tl.1 - m, br.0 + m, br.1 + m, RAIL);
    // cushion segments (nose line) drawn just inside the frame
    for w in &table.walls {
        if w.rail.is_none() {
            continue;
        }
        let a = view.px(w.p);
        let b = view.px(w.p + w.t * w.len);
        c.line(a, b, CUSHION, 3.0);
    }
    // jaws
    for w in &table.walls {
        if w.rail.is_some() {
            continue;
        }
        let a = view.px(w.p);
        let b = view.px(w.p + w.t * w.len.min(120.0));
        c.line(a, b, CUSHION, 3.0);
    }
    // pocket mouths
    for pk in &table.pockets {
        let p = view.px(pk.mouth_center);
        let r = if pk.corner {
            MOUTH_CORNER * 0.5 * view.scale * 0.75
        } else {
            MOUTH_SIDE * 0.5 * view.scale * 0.75
        };
        c.disc(p.0, p.1, r, POCKET);
    }
}

pub fn draw_ball(c: &mut Canvas, view: &View, id: u8, p: V3, highlight: Option<Rgb>) {
    let (x, y) = view.px(p);
    let r = R * view.scale;
    let col = ball_colour(id);
    c.disc(x, y, r, col);
    if id >= 9 {
        // stripe: white band through the middle
        c.disc(x, y, r * 0.45, WHITE);
        c.disc(x, y, r * 0.28, col);
    }
    c.ring(x, y, r, 1.2, BLACK);
    if let Some(h) = highlight {
        c.ring(x, y, r + 3.0, 2.0, h);
    }
    if id != 0 && r > 8.0 {
        let label = format!("{}", id);
        let w = font::text_width(&label, 1);
        c.text(
            &label,
            (x as i64 - (w as i64) / 2).max(0) as usize,
            (y as i64 - 3).max(0) as usize,
            1,
            if ball_colour_dark(id) { WHITE } else { BLACK },
        );
    }
}

/// Trajectory overlay: polyline of each sampled position.
pub fn draw_path(c: &mut Canvas, view: &View, pts: &[V3], col: Rgb, thick: f64) {
    for w in pts.windows(2) {
        c.line(view.px(w[0]), view.px(w[1]), col, thick);
    }
}

pub fn sample_path(states: &[(f64, Vec<BallState>)], ball: usize) -> Vec<V3> {
    states.iter().map(|(_, s)| s[ball].p).collect()
}
