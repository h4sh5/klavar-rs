//! Minimal clean-room Standard MIDI File (SMF) reader.
//! Supports format 0/1/2 with metric (ticks-per-quarter) time division.

use crate::model::{quantize, rescale, Note, TimeSig};

#[derive(Debug, Clone)]
pub enum Ev {
    NoteOn { ch: u8, key: u8, vel: u8 },
    NoteOff { ch: u8, key: u8 },
    Tempo { uspq: u32 },
    TimeSig { num: u8, den_pow: u8 },
    TrackName(String),
}

#[derive(Debug, Clone)]
pub struct Track {
    pub events: Vec<(i64, Ev)>, // absolute ticks
}

pub struct MidiFile {
    pub ppq: u16,
    pub tracks: Vec<Track>,
}

fn vlq(d: &[u8], i: &mut usize) -> Result<u32, String> {
    let mut v: u32 = 0;
    for _ in 0..4 {
        if *i >= d.len() {
            return Err("truncated MIDI (VLQ)".into());
        }
        let b = d[*i];
        *i += 1;
        v = (v << 7) | (b & 0x7f) as u32;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err("bad VLQ".into())
}

fn be16(d: &[u8], p: usize) -> Result<u16, String> {
    if p + 2 > d.len() {
        return Err("truncated".into());
    }
    Ok(((d[p] as u16) << 8) | d[p + 1] as u16)
}

fn be32(d: &[u8], p: usize) -> Result<u32, String> {
    if p + 4 > d.len() {
        return Err("truncated".into());
    }
    Ok(((d[p] as u32) << 24) | ((d[p + 1] as u32) << 16) | ((d[p + 2] as u32) << 8) | d[p + 3] as u32)
}

pub fn parse_midi(d: &[u8]) -> Result<MidiFile, String> {
    if d.len() < 14 || &d[0..4] != b"MThd" {
        return Err("Not a MIDI file (missing MThd header)".into());
    }
    let _hlen = be32(d, 4)?;
    let _format = be16(d, 8)?;
    let ntracks = be16(d, 10)? as usize;
    let division = be16(d, 12)?;
    if division & 0x8000 != 0 {
        return Err("SMPTE time division is not supported".into());
    }
    let ppq = division.max(1);
    let mut tracks = Vec::new();
    let mut p = 14usize;
    for t in 0..ntracks {
        if p + 8 > d.len() || &d[p..p + 4] != b"MTrk" {
            return Err(format!("missing MTrk header for track {}", t));
        }
        let len = be32(d, p + 4)? as usize;
        p += 8;
        let end = (p + len).min(d.len());
        let mut evs = Vec::new();
        let mut tick: i64 = 0;
        let mut i = p;
        let mut running: u8 = 0;
        while i < end {
            let delta = vlq(d, &mut i)? as i64;
            tick += delta;
            if i >= end {
                break;
            }
            let mut status = d[i];
            if status < 0x80 {
                // running status
                status = running;
            } else {
                i += 1;
            }
            if status == 0xFF {
                // meta
                if i >= end {
                    break;
                }
                let mt = d[i];
                i += 1;
                let mlen = vlq(d, &mut i)? as usize;
                let mdata_end = (i + mlen).min(end);
                let data = &d[i..mdata_end];
                i = mdata_end;
                match mt {
                    0x51 if data.len() >= 3 => {
                        let uspq = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32;
                        evs.push((tick, Ev::Tempo { uspq }));
                    }
                    0x58 if !data.is_empty() => {
                        evs.push((tick, Ev::TimeSig { num: data[0], den_pow: data[1] }));
                    }
                    0x03 | 0x01 => {
                        let s = String::from_utf8_lossy(data).to_string();
                        if !s.is_empty() {
                            evs.push((tick, Ev::TrackName(s)));
                        }
                    }
                    0x2F => { /* end of track */ }
                    _ => {}
                }
                continue;
            }
            if status == 0xF0 || status == 0xF7 {
                let slen = vlq(d, &mut i)? as usize;
                i = (i + slen).min(end);
                running = 0; // sysex cancels running status
                continue;
            }
            let kind = status & 0xF0;
            let ch = status & 0x0F;
            match kind {
                0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => {
                    if i + 2 > end {
                        break;
                    }
                    let d1 = d[i];
                    let d2 = d[i + 1];
                    i += 2;
                    running = status;
                    match kind {
                        0x90 if d2 > 0 => evs.push((tick, Ev::NoteOn { ch, key: d1, vel: d2 })),
                        0x90 => evs.push((tick, Ev::NoteOff { ch, key: d1 })),
                        0x80 => evs.push((tick, Ev::NoteOff { ch, key: d1 })),
                        _ => {}
                    }
                }
                0xC0 | 0xD0 => {
                    if i >= end {
                        break;
                    }
                    i += 1;
                    running = status;
                }
                _ => break,
            }
        }
        p = end;
        tracks.push(Track { events: evs });
    }
    Ok(MidiFile { ppq, tracks })
}

/// A channel extracted from a MIDI file.
pub struct Channel {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<Note>,
}

/// Everything extracted from a MIDI file, rescaled to PPQ=480.
pub struct MidiContent {
    pub channels: Vec<Channel>,
    pub tempo: Option<f64>,
    pub time_sigs: Vec<TimeSig>,
}

/// Merge all tracks into per-MIDI-channel note lists, rescaled to PPQ=480.
pub fn content(m: &MidiFile) -> MidiContent {
    let mut out: Vec<Channel> = Vec::new();
    let mut tempo: Option<f64> = None;
    let mut time_sigs: Vec<TimeSig> = Vec::new();
    let mut names: std::collections::HashMap<u8, String> = Default::default();

    for tr in &m.tracks {
        let mut open: std::collections::HashMap<(u8, u8), i64> = Default::default();
        let mut last_ch_notes: Vec<(u8, Vec<Note>)> = Vec::new();
        let track_ppq = m.ppq.max(1) as i64;
        let mut track_end: i64 = 0;
        for (t, ev) in &tr.events {
            let t480 = rescale(*t, track_ppq);
            track_end = track_end.max(t480);
            match ev {
                Ev::NoteOn { ch, key, .. } => {
                    open.insert((*ch, *key), t480);
                }
                Ev::NoteOff { ch, key } => {
                    if let Some(s) = open.remove(&(*ch, *key)) {
                        let (start, end) = (quantize(s), quantize(t480).max(quantize(s) + 1));
                        add_note(&mut last_ch_notes, *ch, Note { start, end, pitch: *key });
                    }
                }
                Ev::Tempo { uspq } => {
                    if tempo.is_none() && *uspq > 0 {
                        tempo = Some(60_000_000.0 / *uspq as f64);
                    }
                }
                Ev::TimeSig { num, den_pow } => {
                    time_sigs.push(TimeSig { tick: t480, num: *num, den: 1u8 << den_pow });
                }
                Ev::TrackName(s) => {
                    // attribute track name to the first channel used by this track;
                    // simpler: remember globally per track (applied below)
                    let _ = s;
                }
            }
        }
        // close dangling notes at track end
        for ((ch, key), s) in open {
            let (start, end) = (quantize(s), track_end.max(quantize(s) + 1));
            add_note(&mut last_ch_notes, ch, Note { start, end, pitch: key });
        }
        // find track name (scan again)
        let tname = tr
            .events
            .iter()
            .rev()
            .find_map(|(_, e)| match e {
                Ev::TrackName(s) if !s.trim().is_empty() => Some(s.clone()),
                _ => None,
            });
        for (ch, notes) in last_ch_notes {
            if notes.is_empty() {
                continue;
            }
            if let Some(n) = &tname {
                names.entry(ch).or_insert_with(|| n.clone());
            }
            match out.iter_mut().find(|c| c.channel == ch) {
                Some(c) => c.notes.extend(notes),
                None => out.push(Channel {
                    name: tname.clone().unwrap_or_else(|| format!("Channel {}", ch + 1)),
                    channel: ch,
                    notes,
                }),
            }
        }
    }
    let _ = names;
    out.sort_by_key(|c| c.channel);
    MidiContent {
        channels: out,
        tempo,
        time_sigs,
    }
}

fn add_note(list: &mut Vec<(u8, Vec<Note>)>, ch: u8, n: Note) {
    for (c, v) in list.iter_mut() {
        if *c == ch {
            v.push(n);
            return;
        }
    }
    list.push((ch, vec![n]));
}

/// Parse MIDI bytes into a Score (parts = MIDI channels with notes).
pub fn score_from_midi(d: &[u8]) -> Result<crate::model::Score, String> {
    let mf = parse_midi(d)?;
    let c = content(&mf);
    let mut score = crate::model::Score {
        tempo_bpm: c.tempo,
        time_sigs: c.time_sigs,
        ..Default::default()
    };
    // Common convention: track 0 is a carrier track whose name is the piece title.
    if mf.tracks.len() >= 2 {
        if let Some(t) = mf.tracks[0].events.iter().find_map(|(_, e)| match e {
            Ev::TrackName(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        }) {
            score.title = t;
        }
    }
    for ch in c.channels {
        let name = if ch.name.is_empty() {
            format!("Channel {}", ch.channel + 1)
        } else {
            ch.name
        };
        score.parts.push(crate::model::Part { name, notes: ch.notes });
    }
    Ok(score)
}
