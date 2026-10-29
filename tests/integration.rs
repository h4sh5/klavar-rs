//! End-to-end tests: parse samples, build klavar scores, emit PDFs.

use klavar_rs::{klavar::{self, Hand, Options}, midi, model::u_pos, musicxml, to_pdf};

fn sample(name: &str) -> Vec<u8> {
    let path = format!("{}/web/samples/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {}", path, e))
}

#[test]
fn u_pos_octave_layout() {
    // one unit per semitone-ish slot; E-F and B-C keep the extra gap
    assert_eq!(u_pos(60) - u_pos(48), 14);
    assert_eq!(u_pos(61) - u_pos(60), 1);
    assert_eq!(u_pos(64) - u_pos(63), 1); // E sits 1 unit from the D# line
    assert_eq!(u_pos(65) - u_pos(64), 2); // E|F boundary gap (no black key)
    assert_eq!(u_pos(72) - u_pos(71), 2); // B|C boundary gap
    assert_eq!(u_pos(61) - u_pos(49), 14); // consistent octaves
}

#[test]
fn midi_smoke() {
    let s = midi::score_from_midi(&sample("test.mid")).unwrap();
    assert_eq!(s.title, "MIDI Test");
    assert_eq!(s.tempo_bpm, Some(120.0));
    assert_eq!(s.parts.len(), 2);
    // chromatic run 60..67 quarter notes
    let p0 = &s.parts[0];
    assert_eq!(p0.notes.len(), 8);
    assert_eq!(p0.notes[0].start, 0);
    assert_eq!(p0.notes[1].start, 480);
    assert_eq!(p0.notes[7].pitch, 67);
    // whole notes in channel 2
    assert_eq!(s.parts[1].notes[0].end, 1920);
}

#[test]
fn musicxml_smoke() {
    let t = String::from_utf8(sample("twinkle.musicxml")).unwrap();
    let s = musicxml::parse_musicxml(&t).unwrap();
    assert_eq!(s.title, "Twinkle Twinkle");
    assert_eq!(s.composer, "Traditional");
    assert_eq!(s.parts.len(), 2);
    assert_eq!(s.parts[0].notes.len(), 14);
    assert_eq!(s.parts[0].name, "Rights");
}

#[test]
fn musicxml_time_signature() {
    let t = String::from_utf8(sample("waltz.musicxml")).unwrap();
    let s = musicxml::parse_musicxml(&t).unwrap();
    let ks = klavar::convert(&s, &Options::default());
    assert!(ks.measures.iter().all(|m| m.num == 3 && m.den == 4));
    // two notes held over bar lines -> exactly two continuation dots
    let dots: usize = ks.voices.iter().flat_map(|v| v.attacks.iter()).map(|a| a.dots.len()).sum();
    assert_eq!(dots, 2);
}

#[test]
fn hands_split_auto() {
    let t = String::from_utf8(sample("chromatic.musicxml")).unwrap();
    let s = musicxml::parse_musicxml(&t).unwrap();
    let ks = klavar::convert(&s, &Options::default());
    assert_eq!(ks.voices.len(), 2);
    let rh = ks.voices.iter().find(|v| v.hand == Hand::Right).unwrap();
    let lh = ks.voices.iter().find(|v| v.hand == Hand::Left).unwrap();
    assert!(rh.attacks.iter().all(|a| a.pitches.iter().all(|&p| p >= 60)));
    assert!(lh.attacks.iter().all(|a| a.pitches.iter().all(|&p| p < 60)));
}

#[test]
fn stop_signs_for_short_notes() {
    let s = midi::score_from_midi(&sample("test.mid")).unwrap();
    let ks = klavar::convert(&s, &Options::default());
    let rh = ks.voices.iter().find(|v| v.hand == Hand::Right).unwrap();
    // notes are 440 ticks with a 40-tick gap -> stop sign on every attack
    // except the final one, which rings to the end of the piece
    assert!(rh.attacks[..rh.attacks.len() - 1].iter().all(|a| a.stop_tick.is_some()));
    assert!(rh.attacks.last().unwrap().stop_tick.is_none());
}

#[test]
fn pdf_output_wellformed() {
    let opts = Options::default();
    let data = sample("twinkle.musicxml");
    let pdf = to_pdf(&data, &opts).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(pdf.windows(6).any(|w| w == b"%%EOF\n"));
    assert!(pdf.windows(4).any(|w| w == b"xref"));
    // xref offsets must be monotonically increasing and within file size
    let xref_at = pdf.windows(5).position(|w| w == b"xref\n").unwrap();
    let tail = &pdf[xref_at..];
    let mut last = 0usize;
    for line in tail.windows(10).step_by(20).take(50) {
        let s = String::from_utf8_lossy(line).to_string();
        if let Ok(off) = s.parse::<usize>() {
            if off == 0 { continue; }
            assert!(off > last && off < pdf.len(), "bad xref offset {} ({} | {})", off, last, pdf.len());
            last = off;
        }
    }
}

fn n_pages(pdf: &[u8]) -> usize {
    pdf.windows(b"/Type /Page ".len()).filter(|w| w == b"/Type /Page ").count()
}

#[test]
fn column_count_controls_layout() {
    let data = sample("twinkle.musicxml");
    let wide = klavar::Options { quarter_mm: 12.0, columns: 1, ..Default::default() };
    let mut pages = Vec::new();
    for cols in [1u8, 2, 3] {
        let pdf = to_pdf(&data, &klavar::Options { columns: cols, ..wide }).unwrap();
        pages.push(n_pages(&pdf));
    }
    assert!(pages[0] > 1, "baseline should be multi-page at 12mm/quarter: {}", pages[0]);
    assert!(pages[1] <= pages[0] && pages[2] <= pages[1], "more columns must not add pages: {:?}", pages);
    assert!(pages[2] < pages[0], "3 columns must save pages vs 1: {:?}", pages);
}

#[test]
fn musicxml_chords_and_ties() {
    let t = String::from_utf8(sample("twinkle.musicxml")).unwrap();
    let s = musicxml::parse_musicxml(&t).unwrap();
    // accompaniment has 4-note chords
    let ks = klavar::convert(&s, &Options { hand_mode: 0, ..Default::default() });
    let lh = ks.voices.iter().find(|v| v.hand == Hand::Left).unwrap();
    assert!(lh.attacks.iter().all(|a| a.pitches.len() == 4));
}
