//! Klavar conversion pipeline: from a parsed Score to laid-out klavar measures.

use crate::musicxml::{measure_len_ticks, tick_unit};
use crate::model::{is_black_key, quantize, Note, Score, TimeSig, GRID, PPQ};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Right,
    Left,
}

/// One chord/attack on one part at one point in time.
#[derive(Debug, Clone)]
pub struct Attack {
    pub tick: i64,
    pub pitches: Vec<u8>,
    /// tick at which a stop sign must be drawn, if any
    pub stop_tick: Option<i64>,
    /// barlines that this (held) note crosses -> continuation dots
    pub dots: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct Measure {
    pub start: i64,
    pub len: i64,
    pub num: u8,
    pub den: u8,
    pub number: usize, // 1-based
    pub implicit: bool,
}

/// A part ready for rendering, with hand and stem index assigned.
#[derive(Debug, Clone)]
pub struct Voice {
    pub name: String,
    pub hand: Hand,
    /// 0 = normal stem length, 1 = longer stem, ... (voices sharing a hand)
    pub stem_index: usize,
    pub attacks: Vec<Attack>,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// length of a quarter note in mm (klavar default: 7.5)
    pub quarter_mm: f64,
    /// 0 = auto (per part; single part splits at middle C),
    /// 1 = all right hand, 2 = all left hand,
    /// 3 = split at middle C, 4 = per part, no middle-C split
    pub hand_mode: u8,
    pub show_bar_numbers: bool,
    pub show_count_numbers: bool,
    pub show_subdivisions: bool,
    /// strips per page: 0 = automatic, 1..=3 = fixed (staff is compressed
    /// horizontally as needed so that the chosen number of columns fit)
    pub columns: u8,
    /// Note size scale: line pitch, radii, stems, overall width (clamped 0.1..=3).
    pub note_size: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            quarter_mm: 7.5,
            hand_mode: 0,
            show_bar_numbers: true,
            show_count_numbers: false,
            show_subdivisions: true,
            columns: 0,
            note_size: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KlavarScore {
    pub title: String,
    pub composer: String,
    pub tempo_bpm: Option<f64>,
    pub measures: Vec<Measure>,
    pub voices: Vec<Voice>,
    pub opts: Options,
    pub midi_min: u8,
    pub midi_max: u8,
    /// first tick of each time-signature change (for printing the signature)
    pub time_sig_ticks: Vec<i64>,
}

const TOL: i64 = GRID; // 1/60 beat tolerance

/// Normalize raw part notes: quantize, dedupe, drop empties.
fn normalize(notes: &[Note]) -> Vec<Note> {
    let mut v: Vec<Note> = notes
        .iter()
        .map(|n| {
            let s = quantize(n.start);
            let e = quantize(n.end).max(s + GRID);
            Note {
                start: s,
                end: e,
                pitch: n.pitch.min(127),
            }
        })
        .collect();
    v.sort_by_key(|n| (n.start, n.pitch));
    // drop exact duplicates (same start & pitch), keep longest
    v.dedup_by(|a, b| {
        if a.start == b.start && a.pitch == b.pitch {
            if a.end > b.end {
                b.end = a.end;
            }
            return true;
        }
        false
    });
    v
}

/// Build klavar attacks for one part, given the barlines it must relate to.
pub fn part_attacks(notes: &[Note], barlines: &[i64]) -> Vec<Attack> {
    let notes = normalize(notes);
    if notes.is_empty() {
        return Vec::new();
    }
    // group by start tick
    let mut groups: Vec<(i64, Vec<u8>, i64)> = Vec::new(); // (tick, pitches, max end)
    for n in &notes {
        match groups.last_mut() {
            Some((t, p, e)) if *t == n.start => {
                if !p.contains(&n.pitch) {
                    p.push(n.pitch);
                }
                *e = (*e).max(n.end);
            }
            _ => groups.push((n.start, vec![n.pitch], n.end)),
        }
    }

    let mut out = Vec::new();
    for (tick, mut pitches, end) in groups.into_iter() {
        pitches.sort();
        let next = notes
            .iter()
            .skip_while(|n| n.start <= tick)
            .map(|n| n.start)
            .next();
        let sounding_end = match next {
            Some(nt) => end.min(nt),
            None => end,
        };
        let mut stop_tick = None;
        if let Some(nt) = next {
            if end + TOL < nt {
                stop_tick = Some(end.max(tick + GRID));
            }
        }
        let stop_at = stop_tick.unwrap_or(sounding_end);
        // continuation dots: barlines strictly between start and release,
        // but not one exactly at the release point (the stop sign is drawn there)
        let dots: Vec<i64> = barlines
            .iter()
            .copied()
            .filter(|b| *b > tick + TOL && *b < stop_at - TOL)
            .collect();
        out.push(Attack {
            tick,
            pitches,
            stop_tick,
            dots,
        });
    }
    out
}

/// Build the full measure list covering the score, honouring time-sig changes.
pub fn build_measures(time_sigs: &[TimeSig], end_tick: i64) -> Vec<Measure> {
    let mut sigs: Vec<TimeSig> = Vec::new();
    if time_sigs.is_empty() || time_sigs[0].tick > 0 {
        sigs.push(TimeSig {
            tick: 0,
            num: 4,
            den: 4,
        });
    }
    for s in time_sigs {
        let s = TimeSig {
            tick: quantize(s.tick),
            num: s.num.max(1),
            den: s.den.max(1),
        };
        match sigs.last_mut() {
            Some(l) if l.tick == s.tick => *l = s,
            _ => sigs.push(s),
        }
    }
    let mut out = Vec::new();
    let mut tick = 0i64;
    let mut si = 0usize;
    let mut number = 1usize;
    while tick <= end_tick {
        while si + 1 < sigs.len() && sigs[si + 1].tick <= tick {
            si += 1;
        }
        let sig = sigs[si];
        let unit = tick_unit(sig.den);
        let len = measure_len_ticks(sig.num, sig.den, unit).max(1);
        // if the next time signature falls inside this measure, cut it short
        let len = if si + 1 < sigs.len() {
            len.min((sigs[si + 1].tick - tick).max(GRID))
        } else {
            len
        };
        out.push(Measure {
            start: tick,
            len,
            num: sig.num,
            den: sig.den,
            number,
            implicit: false,
        });
        number += 1;
        tick += len;
    }
    if out.is_empty() {
        out.push(Measure {
            start: 0,
            len: PPQ * 4,
            num: 4,
            den: 4,
            number: 1,
            implicit: false,
        });
    }
    out
}

/// Assign hands to parts according to the chosen mode; build voices with attacks.
pub fn build_voices(score: &Score, measures: &[Measure], opts: &Options) -> Vec<Voice> {
    let mut barlines: Vec<i64> = measures.iter().map(|m| m.start).collect();
    if let Some(last) = measures.last() {
        barlines.push(last.start + last.len);
    }

    // split parts into hand-groups
    enum Plan {
        PerPart,
        SplitMiddleC,
    }
    let plan = match opts.hand_mode {
        1 | 2 => Plan::PerPart,
        3 => Plan::SplitMiddleC,
        _ => {
            // auto: per part; single part -> split at middle C
            let usable = score
                .parts
                .iter()
                .filter(|p| !p.notes.is_empty())
                .count();
            if usable <= 1 {
                Plan::SplitMiddleC
            } else {
                Plan::PerPart
            }
        }
    };

    // produce (name, notes) groups
    let mut groups: Vec<(String, Vec<Note>)> = Vec::new();
    match plan {
        Plan::PerPart => {
            for p in &score.parts {
                if !p.notes.is_empty() {
                    groups.push((p.name.clone(), p.notes.clone()));
                }
            }
        }
        Plan::SplitMiddleC => {
            let all: Vec<Note> = score
                .parts
                .iter()
                .flat_map(|p| p.notes.iter().copied())
                .collect();
            let rh: Vec<Note> = all.iter().copied().filter(|n| n.pitch >= 60).collect();
            let lh: Vec<Note> = all.iter().copied().filter(|n| n.pitch < 60).collect();
            let has_rh = !rh.is_empty();
            let has_lh = !lh.is_empty();
            if has_rh {
                groups.push(("Right hand".into(), rh));
            }
            if has_lh {
                groups.push(("Left hand".into(), lh));
            }
            if !has_rh && !has_lh {
                groups.push(("Right hand".into(), Vec::new()));
            }
        }
    }

    // order parts: descending average pitch (melody on the right)
    if opts.hand_mode == 0 || opts.hand_mode == 4 {
        let avg = |v: &(String, Vec<Note>)| -> f64 {
            if v.1.is_empty() {
                0.0
            } else {
                v.1.iter().map(|n| n.pitch as f64).sum::<f64>() / v.1.len() as f64
            }
        };
        groups.sort_by(|a, b| avg(b).partial_cmp(&avg(a)).unwrap_or(std::cmp::Ordering::Equal));
    }

    // default nicer names for two groups
    if groups.len() == 2 {
        let generic = |s: &String| {
            s.starts_with("Channel") || s.starts_with("Part") || s.starts_with("P") || s.is_empty()
        };
        if generic(&groups[0].0) {
            groups[0].0 = "Right hand".into();
        }
        if generic(&groups[1].0) {
            groups[1].0 = "Left hand".into();
        }
    }

    let mut voices = Vec::new();
    let mut n_right = 0usize;
    let mut n_left = 0usize;
    for (i, (name, notes)) in groups.into_iter().enumerate() {
        let hand = match opts.hand_mode {
            1 => Hand::Right,
            2 => Hand::Left,
            _ => {
                if i == 0 {
                    Hand::Right
                } else if i == 1 {
                    Hand::Left
                } else if i % 2 == 0 {
                    Hand::Right
                } else {
                    Hand::Left
                }
            }
        };
        let stem_index = match hand {
            Hand::Right => {
                let k = n_right;
                n_right += 1;
                k
            }
            Hand::Left => {
                let k = n_left;
                n_left += 1;
                k
            }
        };
        voices.push(Voice {
            name,
            hand,
            stem_index,
            attacks: part_attacks(&notes, &barlines),
        });
    }
    voices
}

pub fn convert(score: &Score, opts: &Options) -> KlavarScore {
    let end = score.last_tick();
    let mut measures = build_measures(&score.time_sigs, end);
    // drop fully-empty trailing measures
    while measures.len() > 1 && measures.last().map(|m| m.start >= end).unwrap_or(false) {
        measures.pop();
    }
    let mut time_sig_ticks: Vec<i64> = Vec::new();
    {
        let mut last: Option<(u8, u8)> = None;
        let mut si = 0usize;
        let mut sigs = score.time_sigs.clone();
        if sigs.is_empty() || sigs[0].tick > 0 {
            sigs.insert(0, TimeSig { tick: 0, num: 4, den: 4 });
        }
        for m in &measures {
            while si + 1 < sigs.len() && sigs[si + 1].tick <= m.start {
                si += 1;
            }
            let cur = (sigs[si].num, sigs[si].den);
            if last != Some(cur) {
                time_sig_ticks.push(m.start);
                last = Some(cur);
            }
        }
    }
    let voices = build_voices(score, &measures, opts);
    let (mn, mx) = {
        let mut mn = 127u8;
        let mut mx = 0u8;
        for v in &voices {
            for a in &v.attacks {
                for &p in &a.pitches {
                    mn = mn.min(p);
                    mx = mx.max(p);
                }
            }
        }
        if mn > mx {
            (60, 72)
        } else {
            (mn, mx)
        }
    };
    KlavarScore {
        title: score.title.clone(),
        composer: score.composer.clone(),
        tempo_bpm: score.tempo_bpm,
        measures,
        voices,
        opts: opts.clone(),
        midi_min: mn,
        midi_max: mx,
        time_sig_ticks,
    }
}

/// convenience: is a pitch a black key (re-export for renderer)
pub fn black(p: u8) -> bool {
    is_black_key(p)
}
