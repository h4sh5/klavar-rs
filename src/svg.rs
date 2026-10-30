//! SVG backend for the klavar renderer (interactive web view).
//!
//! Collects drawing operations per page and emits one SVG with the pages
//! laid out side by side. Note heads carry `class="kn" data-n="tick:pitch"`
//! so the web app can find and highlight them during playback.

use crate::render::Painter;

const INK: &str = "#1c2733";

fn f(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    format!("{}", r)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub struct SvgPainter {
    w: f64,
    h: f64,
    pages: Vec<Vec<String>>,
    cur: usize,
}

impl SvgPainter {
    pub fn new(w: f64, h: f64) -> Self {
        SvgPainter { w, h, pages: Vec::new(), cur: 0 }
    }

    fn op(&mut self, s: String) {
        self.pages[self.cur].push(s);
    }

    /// Final SVG document: all pages side by side with a small gap.
    pub fn finish(&self) -> String {
        let n = self.pages.len().max(1);
        let gap = 18.0;
        let total_w = self.w * n as f64 + gap * (n - 1) as f64;
        let mut out = String::new();
        out.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}">"#,
            f(total_w),
            f(self.h),
            f(total_w),
            f(self.h)
        ));
        for (i, ops) in self.pages.iter().enumerate() {
            let x = i as f64 * (self.w + gap);
            out.push_str(&format!(r#"<g transform="translate({},0)">"#, f(x)));
            out.push_str(&format!(
                r#"<rect width="{}" height="{}" fill='#ffffff' stroke='#d9d5cc'/>"#,
                f(self.w),
                f(self.h)
            ));
            for op in ops {
                out.push_str(op);
            }
            out.push_str("</g>");
        }
        out.push_str("</svg>");
        out
    }
}

impl Painter for SvgPainter {
    fn page(&mut self) {
        self.pages.push(Vec::new());
        self.cur = self.pages.len() - 1;
    }

    fn set_active_page(&mut self, i: usize) {
        if !self.pages.is_empty() {
            self.cur = i.min(self.pages.len() - 1);
        }
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, dash: &[f64]) {
        let dash_attr = if dash.is_empty() {
            String::new()
        } else {
            let d: Vec<String> = dash.iter().map(|v| f(*v)).collect();
            format!(r#" stroke-dasharray="{}""#, d.join(" "))
        };
        self.op(format!(
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round"{}/>"#,
            f(x1), f(y1), f(x2), f(y2), INK, f(width), dash_attr
        ));
    }

    fn circle(&mut self, x: f64, y: f64, r: f64, filled: bool, width: f64) {
        self.op(format!(
            r#"<circle cx="{}" cy="{}" r="{}" stroke="{}" stroke-width="{}" fill="{}"/>"#,
            f(x), f(y), f(r), INK, f(width), if filled { INK } else { "none" }
        ));
    }

    fn circle_tagged(&mut self, x: f64, y: f64, r: f64, filled: bool, width: f64, tag: &str) {
        self.op(format!(
            r#"<circle class="kn" data-n="{}" cx="{}" cy="{}" r="{}" stroke="{}" stroke-width="{}" fill="{}"/>"#,
            esc(tag), f(x), f(y), f(r), INK, f(width), if filled { INK } else { "none" }
        ));
    }

    fn path(&mut self, pts: &[(f64, f64)], width: f64) {
        if pts.is_empty() {
            return;
        }
        let p: Vec<String> = pts.iter().map(|(x, y)| format!("{},{}", f(*x), f(*y))).collect();
        self.op(format!(
            r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
            p.join(" "), INK, f(width)
        ));
    }

    fn text(&mut self, x: f64, y: f64, size: f64, s: &str, bold: bool, align: u8) {
        let anchor = match align {
            1 => "middle",
            2 => "end",
            _ => "start",
        };
        let weight = if bold { r#" font-weight="bold""# } else { "" };
        self.op(format!(
            r#"<text x="{}" y="{}" font-size="{}" text-anchor="{}"{} font-family="Helvetica, Arial, sans-serif" fill="{}">{}</text>"#,
            f(x), f(y), f(size), anchor, weight, INK, esc(s)
        ));
    }
}
