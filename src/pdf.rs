//! Minimal clean-room PDF writer (vector graphics + WinAnsi text, base-14 fonts).

pub struct PdfBuilder {
    pub w: f64,
    pub h: f64,
    pages: Vec<Vec<u8>>,
    cur: usize,
}

fn n(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let s = format!("{}", r);
    s
}

fn esc(s: &str) -> Vec<u8> {
    // WinAnsi-ish: keep Latin-1, replace others with '?'
    let mut out = Vec::new();
    for ch in s.chars() {
        let b: u8 = if (0x20..0x7f).contains(&(ch as u32)) {
            ch as u8
        } else if let Some(l) = to_latin1(ch) {
            l
        } else {
            b'?'
        };
        if b == b'(' || b == b')' || b == b'\\' {
            out.push(b'\\');
        }
        out.push(b);
    }
    out
}

fn to_latin1(ch: char) -> Option<u8> {
    let cp = ch as u32;
    if cp >= 0xA0 && cp <= 0xFF {
        Some(cp as u8)
    } else {
        match ch {
            '\u{2018}' => Some(0x91),
            '\u{2019}' => Some(0x92),
            '\u{201C}' => Some(0x93),
            '\u{201D}' => Some(0x94),
            '\u{2013}' => Some(0x96),
            '\u{2014}' => Some(0x97),
            '\u{2022}' => Some(0x95),
            '\u{2026}' => Some(0x85),
            _ => None,
        }
    }
}

impl PdfBuilder {
    pub fn new(w: f64, h: f64) -> Self {
        PdfBuilder { w, h, pages: Vec::new(), cur: 0 }
    }

    pub fn page(&mut self) {
        self.pages.push(Vec::new());
        self.cur = self.pages.len() - 1;
    }

    pub fn set_active_page(&mut self, i: usize) {
        if !self.pages.is_empty() {
            self.cur = i.min(self.pages.len() - 1);
        }
    }

    fn op(&mut self, s: &str) {
        let p = &mut self.pages[self.cur];
        p.extend_from_slice(s.as_bytes());
        p.push(b'\n');
    }

    fn y(&self, v: f64) -> f64 {
        self.h - v // our drawing API uses y-down from page top
    }

    /// Draw a line (y-down coordinates).
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, dash: &[f64]) {
        self.op(&format!("{} w", n(width)));
        if dash.is_empty() {
            self.op("[] 0 d");
        } else {
            let d: Vec<String> = dash.iter().map(|v| n(*v)).collect();
            self.op(&format!("[{}] 0 d", d.join(" ")));
        }
        self.op(&format!("1 J 1 j"));
        self.op(&format!(
            "{} {} m {} {} l S",
            n(x1),
            n(self.y(y1)),
            n(x2),
            n(self.y(y2))
        ));
    }

    /// Circle; filled or stroked.
    pub fn circle(&mut self, x: f64, y: f64, r: f64, filled: bool, width: f64) {
        let cy = self.y(y);
        let k = r * 0.5523;
        self.op(&format!("{} w", n(width)));
        self.op("[] 0 d");
        self.op(&format!("{} {} m", n(x + r), n(cy)));
        self.op(&format!(
            "{} {} {} {} {} {} c",
            n(x + r),
            n(cy - k),
            n(x + k),
            n(cy - r),
            n(x),
            n(cy - r)
        ));
        self.op(&format!(
            "{} {} {} {} {} {} c",
            n(x - k),
            n(cy - r),
            n(x - r),
            n(cy - k),
            n(x - r),
            n(cy)
        ));
        self.op(&format!(
            "{} {} {} {} {} {} c",
            n(x - r),
            n(cy + k),
            n(x - k),
            n(cy + r),
            n(x),
            n(cy + r)
        ));
        self.op(&format!(
            "{} {} {} {} {} {} c",
            n(x + k),
            n(cy + r),
            n(x + r),
            n(cy + k),
            n(x + r),
            n(cy)
        ));
        self.op("h");
        self.op(if filled { "f" } else { "S" });
    }

    /// Poly-line through points (stroke).
    pub fn path(&mut self, pts: &[(f64, f64)], width: f64) {
        if pts.is_empty() {
            return;
        }
        self.op(&format!("{} w", n(width)));
        self.op("[] 0 d");
        self.op("1 J 1 j");
        self.op(&format!("{} {} m", n(pts[0].0), n(self.y(pts[0].1))));
        for p in &pts[1..] {
            self.op(&format!("{} {} l", n(p.0), n(self.y(p.1))));
        }
        self.op("S");
    }

    /// Text with baseline at (x, y) y-down; align: 0 left, 1 center, 2 right.
    /// Approximate width estimate for alignment.
    pub fn text(&mut self, x: f64, y: f64, size: f64, s: &str, bold: bool, align: u8) {
        let eff = if bold { 0.58 } else { 0.52 };
        let w = s.chars().count() as f64 * size * eff;
        let x = match align {
            1 => x - w / 2.0,
            2 => x - w,
            _ => x,
        };
        let font = if bold { "/F2" } else { "/F1" };
        let bytes = esc(s);
        let mut cmd = format!("BT {} {} Tf ", font, n(size));
        cmd.push_str(&format!("{} {} Td (", n(x), n(self.y(y))));
        let p = &mut self.pages[self.cur];
        p.extend_from_slice(cmd.as_bytes());
        p.extend_from_slice(&bytes);
        p.extend_from_slice(b") Tj ET\n");
    }

    pub fn build(self) -> Vec<u8> {
        let npages = self.pages.len().max(1);
        let mut out: Vec<u8> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new(); // object offsets (1-based)
        macro_rules! obj {
            ($content:expr) => {{
                offsets.push(out.len());
                let c = $content;
                out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
                out.extend_from_slice(c.as_bytes());
                out.extend_from_slice(b"\nendobj\n");
            }};
        }

        out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

        let pages_kids: Vec<String> = (0..npages).map(|i| format!("{} 0 R", 5 + 2 * i)).collect();
        // 1: catalog
        obj!(String::from("<< /Type /Catalog /Pages 2 0 R >>"));
        // 2: pages
        obj!(format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            pages_kids.join(" "),
            npages
        ));
        // 3: F1, 4: F2
        obj!(String::from(
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
        ));
        obj!(String::from(
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
        ));
        for (i, content) in self.pages.iter().enumerate() {
            let page_obj = 5 + 2 * i;
            let content_obj = page_obj + 1;
            obj!(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                n(self.w), n(self.h), content_obj
            ));
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", content_obj).as_bytes());
            out.extend_from_slice(format!("<< /Length {} >>\nstream\n", content.len()).as_bytes());
            out.extend_from_slice(content);
            out.extend_from_slice(b"endstream\nendobj\n");
        }
        let xref_pos = out.len();
        let total = offsets.len() + 1;
        out.extend_from_slice(format!("xref\n0 {}\n", total).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                total, xref_pos
            )
            .as_bytes(),
        );
        out
    }
}
