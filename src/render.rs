//! Render a KlavarScore to PDF.
//!
//! Geometry model (per the published klavar notation description):
//! - horizontal axis = pitch in staff units `u`: 14 u per octave (E-F and
//!   B-C gaps are 2u wide); black-key lines are solid verticals, white keys
//!   are the 1u-wide gaps; the two lines beside middle C (C#4/D#4) are dotted;
//! - vertical axis = time, read top to bottom; one quarter note = `quarter_mm`;
//! - horizontal stems at attack time: right = right hand, left = left hand;
//!   black noteheads rest on top of the stem, white noteheads below it;
//! - solid bar lines, dashed count lines, 'v' stop signs, small continuation
//!   dots where a note is held across a bar line.

use crate::klavar::{black, Hand, KlavarScore};
use crate::model::u_pos;
use crate::pdf::PdfBuilder;

const PAGE_W: f64 = 595.28; // A4
const PAGE_H: f64 = 841.89;
const MARGIN: f64 = 34.0;
const LABEL_W: f64 = 42.0;
const GUTTER: f64 = 26.0;

pub struct Geo {
    pub u: f64,
    pub r: f64,
    pub stem: f64,
    pub qn: f64, // pt per quarter note
}

impl Geo {
    pub fn stem_len(&self, idx: usize) -> f64 {
        self.stem * (1.0 + 0.45 * idx.min(3) as f64)
    }
}

pub fn geo(k: &KlavarScore) -> Geo {
    let u: f64 = 3.4;
    Geo {
        u,
        r: 0.9 * u,
        stem: 7.5 * u,
        qn: k.opts.quarter_mm.clamp(2.0, 30.0) * 72.0 / 25.4,
    }
}

struct StripPos {
    page: usize,
    x0: f64,
    x1: f64,
    label_w: f64,
    y_top: f64,
    y_bot: f64,
    tick_start: i64, // first tick of the strip's first measure
    tick_end: i64,   // end tick of the strip's last measure
    y_first: f64,    // y of tick_start
}

impl StripPos {
    fn contains(&self, tick: i64) -> bool {
        tick >= self.tick_start && tick < self.tick_end
    }
    fn tick_y(&self, g: &Geo, tick: i64) -> f64 {
        self.y_first + (tick - self.tick_start) as f64 / 480.0 * g.qn
    }
}

struct Placed {
    index: usize,
    strip: usize,
    y0: f64,
}

enum Annot {
    Stop { page: usize, x: f64, y: f64 },
    Dot { page: usize, x: f64, y: f64 },
}

/// x of a note head / staff line relative to the strip's x0.
fn note_x_rel(k: &KlavarScore, g: &Geo, pitch: u8) -> f64 {
    let min_u = u_pos(k.midi_min) - 2;
    (u_pos(pitch) - min_u) as f64 * g.u
}

pub fn render(k: &KlavarScore, pdf: &mut PdfBuilder) {
    let mut g = geo(k);

    let min_u = u_pos(k.midi_min) - 2;
    let max_u = u_pos(k.midi_max) + 2;
    let usable = PAGE_W - 2.0 * MARGIN;

    // natural (uncompressed) strip width decides automatic column count
    let nat_staff = ((max_u - min_u) as f64 * g.u).max(60.0);
    let nat_stem_max = g.stem_len(k.voices.iter().map(|v| v.stem_index).max().unwrap_or(0));
    let nat_strip = nat_staff + 2.0 * (nat_stem_max + g.r + 6.0);
    let auto_cols = (((usable + GUTTER) / (nat_strip + GUTTER)).floor() as usize).clamp(1, 3);
    let cols = if k.opts.columns == 0 {
        auto_cols
    } else {
        (k.opts.columns as usize).clamp(1, 3)
    };

    // column pitch; compress the staff horizontally when fixed columns are
    // requested but the strips are too wide to fit side by side
    let col_pitch = (usable + GUTTER) / cols as f64;
    let label_w = if cols > 1 { 24.0 } else { LABEL_W };
    let col_avail = col_pitch - GUTTER - label_w;
    let f = if cols > 1 {
        (col_avail / nat_strip).min(1.0)
    } else {
        1.0
    };
    if f < 1.0 {
        g.u *= f;
        g.stem *= f;
        g.r = (g.r * f).max(1.4);
    }

    let stem_max = g.stem_len(k.voices.iter().map(|v| v.stem_index).max().unwrap_or(0));
    let staff_px = ((max_u - min_u) as f64 * g.u).max(40.0);
    let strip_w_full = staff_px + 2.0 * (stem_max + g.r + 6.0);

    let first_top = MARGIN + 66.0; // room for the title block on page 1
    let page_top = MARGIN + 10.0;
    let page_bottom = PAGE_H - MARGIN - 12.0;

    // ---- pack measures into strips ----
    // With a fixed column count, strip height is capped near the average so
    // that the columns on a page fill evenly instead of one full-height strip
    // running ahead of the others; the cap is grown until the page count
    // stays at the minimum achievable with full-height strips.
    let hs: Vec<f64> = k.measures.iter().map(|m| m.len as f64 / 480.0 * g.qn).collect();
    let total_h: f64 = hs.iter().sum();
    let first_h = page_bottom - first_top;
    let rest_h = page_bottom - page_top;
    let max_measure = hs.iter().cloned().fold(0.0f64, f64::max);
    let pages_min = {
        let mut pages = 1usize;
        let mut room = first_h;
        for &h in &hs {
            if h > room {
                pages += 1;
                room = rest_h;
            }
            room -= h;
        }
        pages
    };
    let page_goal = pages_min.div_ceil(cols).max(1);
    let simulate = |cap: f64| -> usize {
        let mut ns = 1usize;
        let mut used = 0.0;
        for &h in &hs {
            if used > 0.0 && used + h > cap {
                ns += 1;
                used = 0.0;
            }
            used += h;
        }
        ns
    };
    let mut cap = (total_h / ((page_goal * cols) as f64)).max(max_measure);
    for _ in 0..12 {
        if simulate(cap).div_ceil(cols) <= page_goal {
            break;
        }
        cap *= 1.15;
    }
    let cap = cap.min(rest_h.max(max_measure));

    let mut placed: Vec<Placed> = Vec::new();
    let mut strip_bounds: Vec<(f64, f64, usize, usize)> = Vec::new(); // (top,bottom,first_measure,last_measure)
    {
        let mut strip_idx = 0usize;
        let mut y = first_top;
        let mut strip_top = first_top;
        strip_bounds.push((first_top, first_top, 0, 0));
        for (i, &h) in hs.iter().enumerate() {
            let strip_has_content = placed.iter().any(|p| p.strip == strip_idx);
            if strip_has_content && (y + h > page_bottom || y + h > strip_top + cap) {
                strip_bounds[strip_idx].1 = y;
                strip_bounds[strip_idx].3 = i - 1;
                strip_idx += 1;
                strip_bounds.push((page_top, page_top, i, i));
                y = page_top;
                strip_top = page_top;
            } else if !strip_has_content {
                strip_bounds[strip_idx].0 = y;
            }
            placed.push(Placed {
                index: i,
                strip: strip_idx,
                y0: y,
            });
            y += h;
        }
        strip_bounds[strip_idx].1 = y;
        strip_bounds[strip_idx].3 = k.measures.len().saturating_sub(1);
    }
    let nstrips = strip_bounds.len();
    let total_pages = nstrips.div_ceil(cols).max(1);

    // ---- strip positions ----
    let strip_pos: Vec<StripPos> = (0..nstrips)
        .map(|s| {
            let (top, bot, mi0, mi1) = strip_bounds[s];
            let col = s % cols;
            let pad = if cols > 1 {
                ((col_pitch - GUTTER - label_w - strip_w_full) * 0.5).max(0.0)
            } else {
                0.0
            };
            let x0 = MARGIN + (col as f64) * col_pitch + label_w + pad + stem_max + g.r + 4.0;
            StripPos {
                page: s / cols,
                x0,
                x1: x0 + staff_px,
                label_w,
                y_top: top,
                y_bot: bot,
                tick_start: k.measures[mi0].start,
                tick_end: k.measures[mi1].start + k.measures[mi1].len,
                y_first: top,
            }
        })
        .collect();

    // global tick -> (strip index, y)
    let locate = |tick: i64| -> Option<(usize, f64)> {
        let idx = k
            .measures
            .binary_search_by(|m| {
                if m.start + m.len <= tick {
                    std::cmp::Ordering::Less
                } else if m.start > tick {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let p = placed.iter().find(|p| p.index == idx)?;
        let sp = &strip_pos[p.strip];
        Some((p.strip, sp.tick_y(&g, tick)))
    };

    // ---- stop signs & continuation dots (may land in a different strip) ----
    let mut annots: Vec<Annot> = Vec::new();
    for v in &k.voices {
        for a in &v.attacks {
            let xs: Vec<f64> = a.pitches.iter().map(|&p| note_x_rel(k, &g, p)).collect();
            if xs.is_empty() {
                continue;
            }
            let rel_cx = (xs.iter().cloned().fold(f64::INFINITY, f64::min)
                + xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
                / 2.0;
            if let Some(st) = a.stop_tick {
                if let Some((s, y)) = locate(st) {
                    annots.push(Annot::Stop {
                        page: strip_pos[s].page,
                        x: strip_pos[s].x0 + rel_cx,
                        y,
                    });
                }
            }
            for d in &a.dots {
                if let Some((s, y)) = locate(*d) {
                    for &xr in &xs {
                        annots.push(Annot::Dot {
                            page: strip_pos[s].page,
                            x: strip_pos[s].x0 + xr,
                            y,
                        });
                    }
                }
            }
        }
    }

    // ---- pages ----
    for _ in 0..total_pages {
        pdf.page();
    }

    for s in 0..nstrips {
        let sp = &strip_pos[s];
        pdf.set_active_page(sp.page);
        draw_staff(k, pdf, &g, min_u, max_u, sp);
        for p in placed.iter().filter(|p| p.strip == s) {
            draw_measure(k, pdf, &g, sp, p);
        }
        draw_notes(k, pdf, &g, sp);
    }

    for a in &annots {
        match a {
            Annot::Stop { page, x, y } => {
                pdf.set_active_page(*page);
                pdf.path(&[(*x - 2.8, *y - 2.2), (*x, *y + 2.2), (*x + 2.8, *y - 2.2)], 1.1);
            }
            Annot::Dot { page, x, y } => {
                pdf.set_active_page(*page);
                pdf.circle(*x, *y, g.r * 0.42, true, 0.3);
            }
        }
    }

    // ---- titles & page numbers ----
    pdf.set_active_page(0);
    let t = if k.title.is_empty() { "Untitled" } else { &k.title };
    pdf.text(PAGE_W / 2.0, 42.0, 16.0, t, true, 1);
    if !k.composer.is_empty() {
        pdf.text(PAGE_W - MARGIN, 60.0, 11.0, &k.composer, false, 2);
    }
    if let Some(bpm) = k.tempo_bpm {
        pdf.text(
            MARGIN,
            60.0,
            9.0,
            &format!("Tempo: quarter = {} bpm", bpm.round()),
            false,
            0,
        );
    }
    pdf.text(
        MARGIN,
        74.0,
        7.5,
        "stems: -> right hand   <- left hand   v = release   . = held over bar line",
        false,
        0,
    );
    if total_pages > 1 {
        for p in 0..total_pages {
            pdf.set_active_page(p);
            pdf.text(PAGE_W / 2.0, PAGE_H - 18.0, 8.0, &format!("{}", p + 1), false, 1);
        }
    }
}

fn draw_staff(k: &KlavarScore, pdf: &mut PdfBuilder, g: &Geo, min_u: i32, max_u: i32, sp: &StripPos) {
    let x0 = sp.x0;
    let (y_top, y_bot) = (sp.y_top - 6.0, sp.y_bot + 6.0);
    let lo_oct = (k.midi_min as i32).div_euclid(12) - 1;
    let hi_oct = (k.midi_max as i32).div_euclid(12) - 1;
    for oct in lo_oct..=hi_oct {
        for pc in [1u8, 3, 6, 8, 10] {
            let pitch = ((oct + 1) * 12 + pc as i32) as u8;
            let up = u_pos(pitch);
            if up < min_u - 1 || up > max_u + 1 {
                continue;
            }
            let x = x0 + (up - min_u) as f64 * g.u;
            let is_trio = matches!(pc, 6 | 8 | 10);
            let is_center = pitch == 61 || pitch == 62; // C#4 / D#4
            if is_center {
                pdf.line(x, y_top, x, y_bot, 1.0, &[1.8, 1.8]);
            } else {
                pdf.line(x, y_top, x, y_bot, if is_trio { 1.5 } else { 0.9 }, &[]);
            }
        }
    }
}

fn draw_measure(k: &KlavarScore, pdf: &mut PdfBuilder, g: &Geo, sp: &StripPos, p: &Placed) {
    let m = &k.measures[p.index];
    let y0 = p.y0;
    let unit = 480.0 * 4.0 / m.den as f64;
    let unit_pt = unit / 480.0 * g.qn;
    let h = m.len as f64 / 480.0 * g.qn;
    let y1 = y0 + h;
    let (x0, x1) = (sp.x0, sp.x1);

    // bar line
    pdf.line(x0 - 8.0, y0, x1 + 8.0, y0, 1.4, &[]);
    // count lines
    for b in 1..m.num.max(1) {
        let y = y0 + b as f64 * unit_pt;
        if y >= y1 - 0.01 {
            break;
        }
        pdf.line(x0, y, x1, y, 0.7, &[4.0, 3.0]);
    }
    // time signature at start (in this strip's own label gutter)
    if k.time_sig_ticks.contains(&m.start) {
        pdf.text(
            x0 - sp.label_w + 2.0,
            y0 + 22.0,
            8.0,
            &format!("{}/{}", m.num, m.den),
            true,
            0,
        );
    }
    if k.opts.show_bar_numbers {
        pdf.text(x0 - sp.label_w + 2.0, y0 + 4.0, 7.0, &m.number.to_string(), false, 0);
    }
    if k.opts.show_count_numbers {
        for b in 0..m.num.max(1) {
            let y = y0 + b as f64 * unit_pt;
            if y >= y1 - 0.01 && b > 0 {
                break;
            }
            pdf.text(x0 - 6.0, y + 2.0, 6.0, &(b + 1).to_string(), false, 2);
        }
    }
    if k.opts.show_subdivisions {
        let mut offs: Vec<f64> = Vec::new();
        for v in &k.voices {
            for a in &v.attacks {
                if a.tick >= m.start && a.tick < m.start + m.len {
                    let off = (a.tick - m.start) as f64 % unit;
                    if off > 0.5 && off < unit - 0.5 && !offs.iter().any(|o| (o - off).abs() < 0.01) {
                        offs.push(off);
                    }
                }
            }
        }
        for off in offs {
            let y = y0 + off / 480.0 * g.qn;
            pdf.line(x0, y, x0 + 0.42 * (x1 - x0), y, 0.5, &[2.0, 2.0]);
        }
    }
    // final bar line
    if p.index == k.measures.len() - 1 {
        pdf.line(x0 - 8.0, y1, x1 + 8.0, y1, 3.2, &[]);
    }
}

fn draw_notes(k: &KlavarScore, pdf: &mut PdfBuilder, g: &Geo, sp: &StripPos) {
    for v in &k.voices {
        let stem = g.stem_len(v.stem_index);
        let dir = match v.hand {
            Hand::Right => 1.0,
            Hand::Left => -1.0,
        };
        for a in &v.attacks {
            if !sp.contains(a.tick) {
                continue;
            }
            let y = sp.tick_y(g, a.tick);
            let xs: Vec<f64> = a.pitches.iter().map(|&p| sp.x0 + note_x_rel(k, g, p)).collect();
            if xs.is_empty() {
                continue;
            }
            let x_anchor = if dir > 0.0 {
                xs.iter().cloned().fold(f64::INFINITY, f64::min)
            } else {
                xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            };
            pdf.line(x_anchor, y, x_anchor + dir * stem, y, 1.3, &[]);
            for (&pitch, &x) in a.pitches.iter().zip(xs.iter()) {
                if black(pitch) {
                    pdf.circle(x, y - g.r - 0.6, g.r, true, 0.5);
                } else {
                    pdf.circle(x, y + g.r + 0.6, g.r, false, 1.2);
                }
            }
        }
    }
}
