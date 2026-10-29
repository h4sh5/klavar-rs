#!/usr/bin/env python3
"""Verify rendered klavar PDFs structurally using pymupdf."""
import pymupdf

def analyze(path):
    doc = pymupdf.open(path)
    pg = doc[0]
    lines = []   # (x1,y1,x2,y2,width,dashes)
    circles = [] # (cx,cy,r,filled)
    for d in pg.get_drawings():
        w = d.get("width", 1) or 1
        kinds = {it[0] for it in d["items"]}
        if kinds == {"c"}:
            pts = []
            for it in d["items"]:
                for v in it[1:]:
                    if isinstance(v, (list, tuple)):
                        pts.extend(v)
                    else:
                        pts.append(v)
            xs = [p.x for p in pts]; ys = [p.y for p in pts]
            circles.append(((min(xs)+max(xs))/2, (min(ys)+max(ys))/2,
                            (max(xs)-min(xs))/2, d.get("fill") is not None))
        else:
            for it in d["items"]:
                if it[0] == "l":
                    p1, p2 = it[1], it[2]
                    lines.append((p1.x, p1.y, p2.x, p2.y, w, d.get("dashes", "")))
    return lines, circles, pg.get_text()

def check(path, expect):
    lines, circles, text = analyze(path)
    hlines = [l for l in lines if abs(l[1]-l[3]) < 0.1]
    vlines = [l for l in lines if abs(l[0]-l[2]) < 0.1]
    stemR = [l for l in hlines if l[2]-l[0] > 15 and abs(l[4]-1.3) < 0.05]
    stemL = [l for l in hlines if l[0]-l[2] > 15 and abs(l[4]-1.3) < 0.05]
    barlines = [l for l in hlines if l[2]-l[0] > 60 and l[4] >= 1.3]
    dashed = [l for l in hlines if l[5] not in ("", None, "[] 0 d")]
    heads_black = [c for c in circles if c[2] > 2.0 and c[3]]
    heads_white = [c for c in circles if c[2] > 2.0 and not c[3]]
    dots = [c for c in circles if c[2] <= 2.0 and c[3]]
    print(f"--- {path}")
    print(f"  staff-vlines={len(vlines)} stems-right={len(stemR)} stems-left={len(stemL)} "
          f"barlines={len(barlines)} dashed={len(dashed)}")
    print(f"  heads: black={len(heads_black)} white={len(heads_white)}  continuation-dots={len(dots)}")
    ok = True
    def req(cond, msg):
        nonlocal ok
        if not cond:
            ok = False; print("  FAIL:", msg)
    for k, v in expect.items():
        cur = {"stemsR": len(stemR), "stemsL": len(stemL), "bar": len(barlines),
               "hblack": len(heads_black), "hwhite": len(heads_white),
               "dots": len(dots), "vl": len(vlines)}.get(k)
        if cur is not None:
            req(abs(cur - v) < 0.001 if isinstance(v, float) else cur >= v,
                f"{k} expected>={v} got {cur}")
        if k == "title":
            req(v in text, f"title {v!r} missing")
    print("  PASS" if ok else "  ****")
    return ok

allok = True
# Twinkle: 14 melody quarter-notes (RH), 14 accompaniment chords (LH)
allok &= check("/tmp/out_twinkle.pdf", {"stemsR": 14, "stemsL": 14, "bar": 8,
             "hblack": 1, "hwhite": 69, "title": "Twinkle Twinkle"})
# chromatic C3..C5 12th steps split at middle C -> 12 RH / 12 LH
allok &= check("/tmp/out_chromatic.pdf", {"stemsR": 12, "stemsL": 12, "bar": 7})
# waltz: 3/4 measures, RH melody + LH notes held over bar lines (dots)
allok &= check("/tmp/out_waltz.pdf", {"stemsR": 5, "stemsL": 2, "bar": 4, "dots": 1})
# MIDI test: 8 scale notes RH + 2 bass notes LH
allok &= check("/tmp/out_mid.pdf", {"stemsR": 8, "stemsL": 2, "bar": 3})
print("ALL PASS" if allok else "SOME FAILED")
