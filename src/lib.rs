//! klavar-rs: clean-room Klavar (Klavarscript) notation converter.
//!
//! Input: MusicXML or Standard MIDI File. Output: a klavar notation PDF.

pub mod klavar;
pub mod midi;
pub mod model;
pub mod musicxml;
pub mod pdf;
pub mod render;

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
) -> Result<Vec<u8>, JsValue> {
    let opts = klavar::Options {
        quarter_mm: if quarter_mm.is_finite() { quarter_mm.clamp(3.0, 20.0) } else { 7.5 },
        hand_mode,
        show_bar_numbers: flags & 1 != 0,
        show_count_numbers: flags & 2 != 0,
        show_subdivisions: flags & 4 != 0,
        columns: columns.min(3),
    };
    to_pdf(data, &opts).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn wasm_filename(data: &[u8]) -> String {
    pdf_filename(data)
}
