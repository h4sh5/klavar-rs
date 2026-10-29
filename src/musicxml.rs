//! Minimal clean-room MusicXML (score-partwise) reader.

use crate::model::{quantize, Note, Part, TimeSig, PPQ};

const STEP_TABLE: &[(&str, u8)] = &[
    ("C", 0),
    ("D", 2),
    ("E", 4),
    ("F", 5),
    ("G", 7),
    ("A", 9),
    ("B", 11),
];

struct PendingNote {
    note: Note,
    tie_start: bool,
    tie_stop: bool,
}

/// roxmltree does not process DTDs; MusicXML files usually carry one, so strip it.
fn strip_doctype(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(p) = rest.find("<!DOCTYPE") {
        // find the end of the doctype declaration (handles internal subset [...])
        let after = &rest[p..];
        let end = if let Some(sb) = after.find('[') {
            match after[sb..].find(']') {
                Some(cb) => {
                    let mut e = sb + cb + 1;
                    while e < after.len() && after.as_bytes()[e] != b'>' {
                        e += 1;
                    }
                    e + 1
                }
                None => after.len(),
            }
        } else {
            match after.find('>') {
                Some(e) => e + 1,
                None => after.len(),
            }
        };
        out.push_str(&rest[..p]);
        rest = &rest[(p + end).min(rest.len())..];
    }
    out.push_str(rest);
    out
}

pub fn parse_musicxml(text: &str) -> Result<crate::model::Score, String> {
    let text = strip_doctype(text);
    let doc = roxmltree::Document::parse(&text).map_err(|e| format!("XML parse error: {}", e))?;
    let root = doc.root_element();
    if root.tag_name().name() != "score-partwise" {
        return Err("Only MusicXML 'score-partwise' files are supported".into());
    }

    let mut score = crate::model::Score::default();

    for n in root.children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "work" => {
                if let Some(t) = n
                    .children()
                    .find(|c| c.is_element() && c.tag_name().name() == "work-title")
                    .and_then(|c| c.text())
                {
                    score.title = t.trim().to_string();
                }
            }
            "movement-title" => {
                if let Some(t) = n.text() {
                    score.title = t.trim().to_string();
                }
            }
            "identification" => {
                for c in n.children().filter(|c| c.is_element() && c.tag_name().name() == "creator")
                {
                    if c.attribute("type") == Some("composer") {
                        if let Some(t) = c.text() {
                            score.composer = t.trim().to_string();
                        }
                    }
                }
            }
            "part-list" => {
                // names applied later, keyed by part id
            }
            "part" => {
                score.parts.push(parse_part(&n, &mut score.time_sigs)?);
            }
            _ => {}
        }
    }

    // part names from part-list (second pass; re-parse list)
    if let Some(pl) = root
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "part-list")
    {
        let mut names: Vec<String> = Vec::new();
        for sp in pl
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "score-part")
        {
            let name = sp
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "part-name")
                .and_then(|c| c.text())
                .unwrap_or("")
                .trim()
                .to_string();
            names.push(name);
        }
        for (i, p) in score.parts.iter_mut().enumerate() {
            if let Some(nm) = names.get(i) {
                if !nm.is_empty() {
                    p.name = nm.clone();
                }
            }
        }
    }

    if score.parts.iter().all(|p| p.notes.is_empty()) {
        return Err("No notes found in MusicXML file".into());
    }
    Ok(score)
}

fn parse_part(node: &roxmltree::Node, sigs: &mut Vec<TimeSig>) -> Result<Part, String> {
    let mut notes: Vec<Note> = Vec::new();
    let mut pendings: Vec<PendingNote> = Vec::new();
    let mut divisions: f64 = 1.0; // units per quarter
    let mut time = (4u8, 4u8);
    let mut measure_start: i64 = 0; // in ticks (480 ppq)

    for measure in node
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "measure")
    {
        let mut cursor: f64 = 0.0; // in divisions, relative to measure start
        let mut last_note_start: Option<usize> = None; // index into pendings, for <chord/>

        for el in measure.children().filter(|c| c.is_element()) {
            match el.tag_name().name() {
                "attributes" => {
                    for a in el.children().filter(|c| c.is_element()) {
                        match a.tag_name().name() {
                            "divisions" => {
                                if let Some(v) = a.text().and_then(|t| t.trim().parse::<f64>().ok())
                                {
                                    if v > 0.0 {
                                        divisions = v;
                                    }
                                }
                            }
                            "time" => {
                                let mut beats: Option<u8> = None;
                                let mut beat_type: Option<u8> = None;
                                for t in a.children().filter(|c| c.is_element()) {
                                    match t.tag_name().name() {
                                        "beats" => beats = t.text().and_then(|v| v.trim().parse().ok()),
                                        "beat-type" => {
                                            beat_type = t.text().and_then(|v| v.trim().parse().ok())
                                        }
                                        _ => {}
                                    }
                                }
                                if let (Some(b), Some(bt)) = (beats, beat_type) {
                                    if time != (b, bt) || sigs.is_empty() {
                                        sigs.push(TimeSig { tick: measure_start, num: b, den: bt });
                                    }
                                    time = (b, bt);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "note" => {
                    let mut dur_div: f64 = 0.0;
                    let mut is_chord = false;
                    let mut is_rest = false;
                    let mut midi: Option<u8> = None;
                    let mut tie_start = false;
                    let mut tie_stop = false;

                    for c in el.children().filter(|c| c.is_element()) {
                        match c.tag_name().name() {
                            "chord" => is_chord = true,
                            "rest" => is_rest = true,
                            "duration" => {
                                dur_div = c.text().and_then(|t| t.trim().parse().ok()).unwrap_or(0.0)
                            }
                            "pitch" => midi = parse_pitch(&c),
                            "cue" => {}
                            "tie" => {
                                match c.attribute("type") {
                                    Some("start") => tie_start = true,
                                    Some("stop") => tie_stop = true,
                                    _ => {}
                                }
                            }
                            "notations" => {
                                for t in c.children().filter(|x| x.is_element() && x.tag_name().name() == "tie")
                                {
                                    match t.attribute("type") {
                                        Some("start") => tie_start = true,
                                        Some("stop") => tie_stop = true,
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    let dur_ticks = (dur_div * PPQ as f64 / divisions).round() as i64;
                    if is_chord {
                        // same onset as previous note in this measure
                        if let Some(idx) = last_note_start {
                            let start = pendings[idx].note.start;
                            if !is_rest {
                                if let Some(p) = midi {
                                    pendings.push(PendingNote {
                                        note: Note {
                                            start,
                                            end: start + dur_ticks.max(1),
                                            pitch: p,
                                        },
                                        tie_start,
                                        tie_stop,
                                    });
                                }
                            }
                        }
                    } else {
                        let start = measure_start + (cursor * PPQ as f64 / divisions).round() as i64;
                        cursor += dur_div;
                        if !is_rest {
                            if let Some(p) = midi {
                                last_note_start = Some(pendings.len());
                                pendings.push(PendingNote {
                                    note: Note {
                                        start,
                                        end: start + dur_ticks.max(1),
                                        pitch: p,
                                    },
                                    tie_start,
                                    tie_stop,
                                });
                            }
                        }
                    }
                }
                "backup" => {
                    let d = el
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "duration")
                        .and_then(|c| c.text().and_then(|t| t.trim().parse::<f64>().ok()))
                        .unwrap_or(0.0);
                    cursor -= d;
                }
                "forward" => {
                    let d = el
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "duration")
                        .and_then(|c| c.text().and_then(|t| t.trim().parse::<f64>().ok()))
                        .unwrap_or(0.0);
                    cursor += d;
                }
                _ => {}
            }
        }
        // advance measure start using the current time signature
        let unit = tick_unit(time.1);
        measure_start += measure_len_ticks(time.0, time.1, unit);
    }

    // merge ties
    merge_ties(pendings, &mut notes);
    notes.sort_by_key(|n| (n.start, n.pitch));
    notes.iter_mut().for_each(|n| {
        n.start = quantize(n.start);
        n.end = quantize(n.end).max(quantize(n.start) + 1);
    });

    Ok(Part {
        name: node.attribute("id").unwrap_or("Part").to_string(),
        notes,
    })
}

fn merge_ties(pendings: Vec<PendingNote>, out: &mut Vec<Note>) {
    let mut open: Vec<usize> = Vec::new(); // indices in `out` with active tie
    for pn in pendings {
        let mut merged = false;
        if pn.tie_stop {
            if let Some(&oi) = open.iter().find(|&&oi| {
                out[oi].pitch == pn.note.pitch && out[oi].end == pn.note.start
            }) {
                out[oi].end = out[oi].end.max(pn.note.end);
                if pn.tie_start {
                    // keep tie open (continue)
                } else {
                    open.retain(|&x| x != oi);
                }
                merged = true;
            }
        }
        if !merged {
            out.push(pn.note);
            if pn.tie_start {
                open.push(out.len() - 1);
            }
        }
    }
}

fn parse_pitch(node: &roxmltree::Node) -> Option<u8> {
    let mut step = None;
    let mut alter: f64 = 0.0;
    let mut octave: Option<i32> = None;
    for c in node.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "step" => step = c.text().map(|t| t.trim().to_uppercase()),
            "alter" => alter = c.text().and_then(|t| t.trim().parse().ok()).unwrap_or(0.0),
            "octave" => octave = c.text().and_then(|t| t.trim().parse().ok()),
            _ => {}
        }
    }
    let step = step?;
    let base = STEP_TABLE.iter().find(|(s, _)| *s == step)?.1;
    let oct = octave?;
    let v = (oct + 1) * 12 + base as i32 + alter.round() as i32;
    if (0..=127).contains(&v) {
        Some(v as u8)
    } else {
        None
    }
}

/// ticks for one beat of the given beat-type
pub fn tick_unit(den: u8) -> i64 {
    let den = den.max(1);
    (PPQ * 4 + (den as i64) / 2) / (den as i64)
}

/// measure length in ticks
pub fn measure_len_ticks(num: u8, _den: u8, unit: i64) -> i64 {
    (num.max(1) as i64) * unit
}
