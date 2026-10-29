//! Common data model for the klavar converter.
//!
//! All times are expressed in ticks with PPQ = 480 (pulses per quarter note).

pub const PPQ: i64 = 480;
/// Quantization grid: 8 ticks = 1/60 of a quarter note. Preserves duple and
/// triple subdivisions (480/3=160, /6=80, /12=40, /5=96 are all multiples of 8).
pub const GRID: i64 = 8;

pub fn quantize(t: i64) -> i64 {
    ((t + GRID / 2) / GRID) * GRID
}

/// Convert ticks to another PPQ (rounded).
pub fn rescale(t: i64, from_ppq: i64) -> i64 {
    // keep precision: t * 480 / from_ppq with rounding
    (t * PPQ + from_ppq / 2).max(0) / from_ppq + {
        // correct rounding for negatives not needed (t >= 0)
        0
    }
}

/// A single sounded note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    pub start: i64,
    pub end: i64,
    pub pitch: u8, // MIDI note number
}

/// One instrumental part (MIDI channel / MusicXML part).
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeSig {
    pub tick: i64,
    pub num: u8,
    pub den: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Score {
    pub title: String,
    pub composer: String,
    pub parts: Vec<Part>,
    pub time_sigs: Vec<TimeSig>,
    /// tempo in BPM at tick 0, if known
    pub tempo_bpm: Option<f64>,
}

impl Score {
    pub fn last_tick(&self) -> i64 {
        self.parts
            .iter()
            .flat_map(|p| p.notes.iter())
            .map(|n| n.end.max(n.start))
            .max()
            .unwrap_or(PPQ)
    }
}

/// True if the MIDI pitch is a black key.
pub fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Horizontal klavar staff position, in units of the smallest step `u`.
/// Per octave: C=0, C#=1, D=2, D#=3, E=4, F=6, F#=7, G=8, G#=9, A=10, A#=11,
/// B=12 — 14 units per octave. Staff lines sit on the five black keys
/// (units 1, 3, 7, 9, 11); white-key noteheads sit between lines. The empty
/// units 5 and 13 are the E|F and B|C key boundaries, where no black key
/// exists. Matches the official reference engraving: D# notehead centres on
/// its line, C/F white-key heads sit 1 unit left of the C#/F# lines.
pub fn u_pos(midi: u8) -> i32 {
    let oct = (midi as i32).div_euclid(12) - 1; // C4 (midi 60) -> octave 4
    let table = [0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12];
    oct * 14 + table[midi as usize % 12]
}
