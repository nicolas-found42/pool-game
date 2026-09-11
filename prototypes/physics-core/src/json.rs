//! Minimal JSON reader: enough to read the corpus and the digitized curves
//! without pulling a dependency into a throwaway.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(k),
            _ => None,
        }
    }
    pub fn idx(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(v) => v.get(i),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Arr(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_obj(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Obj(m) => Some(m),
            _ => None,
        }
    }
    pub fn f(&self, k: &str) -> f64 {
        self.get(k).and_then(|v| v.as_f64()).unwrap_or(f64::NAN)
    }
    pub fn s(&self, k: &str) -> String {
        self.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }
    pub fn arr(&self, k: &str) -> Vec<Json> {
        self.get(k)
            .and_then(|v| v.as_arr())
            .cloned()
            .unwrap_or_default()
    }
    pub fn b(&self, k: &str) -> bool {
        matches!(self.get(k), Some(Json::Bool(true)))
    }
}

pub fn parse(src: &str) -> Result<Json, String> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let v = parse_value(bytes, &mut i)?;
    Ok(v)
}

fn skip_ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && (b[*i] as char).is_whitespace() {
        *i += 1;
    }
}

fn parse_value(b: &[u8], i: &mut usize) -> Result<Json, String> {
    skip_ws(b, i);
    if *i >= b.len() {
        return Err("eof".into());
    }
    match b[*i] {
        b'{' => parse_obj(b, i),
        b'[' => parse_arr(b, i),
        b'"' => Ok(Json::Str(parse_str(b, i)?)),
        b't' => {
            *i += 4;
            Ok(Json::Bool(true))
        }
        b'f' => {
            *i += 5;
            Ok(Json::Bool(false))
        }
        b'n' => {
            *i += 4;
            Ok(Json::Null)
        }
        _ => parse_num(b, i),
    }
}

fn parse_obj(b: &[u8], i: &mut usize) -> Result<Json, String> {
    *i += 1;
    let mut m = BTreeMap::new();
    loop {
        skip_ws(b, i);
        if *i < b.len() && b[*i] == b'}' {
            *i += 1;
            return Ok(Json::Obj(m));
        }
        let k = parse_str(b, i)?;
        skip_ws(b, i);
        if *i < b.len() && b[*i] == b':' {
            *i += 1;
        }
        let v = parse_value(b, i)?;
        m.insert(k, v);
        skip_ws(b, i);
        if *i < b.len() && b[*i] == b',' {
            *i += 1;
        }
    }
}

fn parse_arr(b: &[u8], i: &mut usize) -> Result<Json, String> {
    *i += 1;
    let mut v = Vec::new();
    loop {
        skip_ws(b, i);
        if *i < b.len() && b[*i] == b']' {
            *i += 1;
            return Ok(Json::Arr(v));
        }
        v.push(parse_value(b, i)?);
        skip_ws(b, i);
        if *i < b.len() && b[*i] == b',' {
            *i += 1;
        }
    }
}

fn parse_str(b: &[u8], i: &mut usize) -> Result<String, String> {
    skip_ws(b, i);
    if *i >= b.len() || b[*i] != b'"' {
        return Err(format!("expected string at {}", *i));
    }
    *i += 1;
    let mut out = String::new();
    while *i < b.len() {
        let c = b[*i];
        *i += 1;
        match c {
            b'"' => return Ok(out),
            b'\\' => {
                if *i >= b.len() {
                    break;
                }
                let e = b[*i];
                *i += 1;
                match e {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'u' => {
                        if *i + 4 <= b.len() {
                            let hex = std::str::from_utf8(&b[*i..*i + 4]).unwrap_or("0000");
                            let cp = u32::from_str_radix(hex, 16).unwrap_or(0);
                            *i += 4;
                            out.push(char::from_u32(cp).unwrap_or('?'));
                        }
                    }
                    other => out.push(other as char),
                }
            }
            other => out.push(other as char),
        }
    }
    Err("unterminated string".into())
}

fn parse_num(b: &[u8], i: &mut usize) -> Result<Json, String> {
    let start = *i;
    while *i < b.len() {
        let c = b[*i];
        if c.is_ascii_digit() || c == b'-' || c == b'+' || c == b'.' || c == b'e' || c == b'E' {
            *i += 1;
        } else {
            break;
        }
    }
    let s = std::str::from_utf8(&b[start..*i]).map_err(|e| e.to_string())?;
    s.parse::<f64>()
        .map(Json::Num)
        .map_err(|_| format!("bad number {s}"))
}
