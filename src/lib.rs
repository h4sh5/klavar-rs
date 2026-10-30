//! klavar-rs: clean-room Klavar (Klavarscript) notation converter.
//!
//! Input: MusicXML or Standard MIDI File. Output: a klavar notation PDF.

pub mod klavar;
pub mod midi;
pub mod model;
pub mod musicxml;
pub mod pdf;
pub mod render;
pub mod svg;

use pdf::PdfBuilder;

#[derive(Debug)]
pub enum Error {
    Parse(String),
    Empty(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(s) => write!(f, "Parse error: {}", s),
            Error::Empty(s) => write!(f, "{}", s),
        }
    }
}

/// Guess format from magic bytes and parse into a Score.
pub fn parse_input(data: &[u8]) -> Result<model::Score, Error> {
    if data.starts_with(b"MThd") {
        midi::score_from_midi(data).map_err(Error::Parse)
    } else {
        // tolerate a UTF-8 BOM
        let text = strip_bom(data);
        let text = String::from_utf8_lossy(&text);
        musicxml::parse_musicxml(&text).map_err(Error::Parse)
    }
}

fn strip_bom(data: &[u8]) -> Vec<u8> {
    if data.starts_with(&[0xef, 0xbb, 0xbf]) {
        data[3..].to_vec()
    } else {
        data.to_vec()
    }
}

/// Full pipeline: input bytes + options -> PDF bytes.
pub fn to_pdf(data: &[u8], opts: &klavar::Options) -> Result<Vec<u8>, Error> {
    let score = parse_input(data)?;
    if score.parts.iter().all(|p| p.notes.is_empty()) {
        return Err(Error::Empty("No playable notes found in input".into()));
    }
    let ks = klavar::convert(&score, opts);
    let mut pdf = PdfBuilder::new(595.28, 841.89);
    render::render(&ks, &mut pdf);
    Ok(pdf.build())
}

/// Full pipeline: input bytes + options -> klavar notation as an SVG string
/// (note heads tagged with "tick:pitch" for interactive highlighting).
pub fn to_svg(data: &[u8], opts: &klavar::Options) -> Result<String, Error> {
    let score = parse_input(data)?;
    if score.parts.iter().all(|p| p.notes.is_empty()) {
        return Err(Error::Empty("No playable notes found in input".into()));
    }
    let ks = klavar::convert(&score, opts);
    let mut p = svg::SvgPainter::new(595.28, 841.89);
    render::render(&ks, &mut p);
    Ok(p.finish())
}

/// Playback timeline as JSON: `{"bpm":..,"ppq":480,"endTick":..,
/// "notes":[[start,end,pitch],..]}` sorted by start tick. Times are in
/// ticks; matches the note-head tags emitted in the SVG.
pub fn timeline_json(data: &[u8]) -> Result<String, Error> {
    let score = parse_input(data)?;
    let mut notes: Vec<model::Note> = Vec::new();
    for p in &score.parts {
        for n in klavar::normalize(&p.notes) {
            if !notes.iter().any(|m| m.start == n.start && m.pitch == n.pitch) {
                notes.push(n);
            }
        }
    }
    if notes.is_empty() {
        return Err(Error::Empty("No playable notes found in input".into()));
    }
    notes.sort_by_key(|n| (n.start, n.pitch));
    let bpm = score.tempo_bpm.unwrap_or(120.0).clamp(20.0, 400.0);
    let mut s = String::with_capacity(64 + notes.len() * 24);
    s.push_str(&format!(
        "{{\"bpm\":{:.4},\"ppq\":{},\"endTick\":{},\"notes\":[",
        bpm,
        model::PPQ,
        score.last_tick()
    ));
    for (i, n) in notes.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("[{},{},{}]", n.start, n.end, n.pitch));
    }
    s.push_str("]}");
    Ok(s)
}

/// Name a downloadable PDF from the score title.
pub fn pdf_filename(data: &[u8]) -> String {
    let base = parse_input(data)
        .map(|s| s.title)
        .unwrap_or_default();
    let mut safe: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    safe = safe.trim_matches('_').to_string();
    if safe.is_empty() {
        safe = "klavarscore".into();
    }
    safe.truncate(60);
    format!("{}.pdf", safe)
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn wasm_convert(
    data: &[u8],
    quarter_mm: f64,
    hand_mode: u8,
    flags: u8,
    columns: u8,
    note_size: f64,
) -> Result<Vec<u8>, JsValue> {
    let opts = klavar::Options {
        quarter_mm: if quarter_mm.is_finite() { quarter_mm.clamp(3.0, 20.0) } else { 7.5 },
        hand_mode,
        show_bar_numbers: flags & 1 != 0,
        show_count_numbers: flags & 2 != 0,
        show_subdivisions: flags & 4 != 0,
        columns: columns.min(3),
        note_size: if note_size.is_finite() { note_size } else { 1.0 },
    };
    to_pdf(data, &opts).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn opts_from(quarter_mm: f64, hand_mode: u8, flags: u8, columns: u8, note_size: f64) -> klavar::Options {
    klavar::Options {
        quarter_mm: if quarter_mm.is_finite() { quarter_mm.clamp(3.0, 20.0) } else { 7.5 },
        hand_mode,
        show_bar_numbers: flags & 1 != 0,
        show_count_numbers: flags & 2 != 0,
        show_subdivisions: flags & 4 != 0,
        columns: columns.min(3),
        note_size: if note_size.is_finite() { note_size } else { 1.0 },
    }
}

#[wasm_bindgen]
pub fn wasm_filename(data: &[u8]) -> String {
    pdf_filename(data)
}

/// Same as wasm_convert but returns the klavar score as an SVG string.
#[wasm_bindgen]
pub fn wasm_svg(
    data: &[u8],
    quarter_mm: f64,
    hand_mode: u8,
    flags: u8,
    columns: u8,
    note_size: f64,
) -> Result<String, JsValue> {
    to_svg(data, &opts_from(quarter_mm, hand_mode, flags, columns, note_size))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Playback timeline JSON for the interactive view (see `timeline_json`).
#[wasm_bindgen]
pub fn wasm_timeline(data: &[u8]) -> Result<String, JsValue> {
    timeline_json(data).map_err(|e| JsValue::from_str(&e.to_string()))
}
