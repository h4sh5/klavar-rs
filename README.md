# klavar-rs

A clean-room re-implementation of the [Klavar](https://klavar.com) music-notation
converter ("Klavarscript") in pure Rust → WebAssembly, with a frontend-only web UI:
upload a **MusicXML** or **MIDI** file, get a **klavar-notation PDF** — computed
entirely in the browser, nothing is uploaded anywhere.

## Layout

```
src/
  model.rs      note/event model, quantization grid, klavar staff geometry (u_pos)
  midi.rs       Standard MIDI File (SMF type 0/1) parser — no dependencies
  musicxml.rs   MusicXML parser (roxmltree; handles .xml; .mxl unzipped in JS)
  klavar.rs     the notation pipeline: hands, measures, attacks, stops, dots
  render.rs     page layout + engraving to PDF drawing primitives
  pdf.rs        minimal PDF 1.4 writer (Helvetica, xref, A4 pages)
  bin/          klavar-conv — native CLI
web/            index.html + app.js + style.css (vanilla JS, module imports pkg/)
pkg/            wasm-pack output (--target web)
web-app/        assembled deployable static site (scripts/build_web.sh)
docs/           copy of web-app
tests/          integration tests (parse → convert → PDF invariants)
scripts/        sample generation, PDF geometry verification, web build
```

## Notation model

The klavar staff is drawn vertically: **pitch across, time down**.

* 14 horizontal *units* per octave; five solid vertical staff lines per octave sit
  on the black keys (C#, D#, F#, G#, A#); the two empty units are the E|F and B|C
  key boundaries where no black key exists. Line positions/widths follow the
  published reference engraving; the two lines flanking middle C are dashed.
* Black-key notes = **filled** noteheads drawn just above the attack line;
  white-key notes = **hollow** noteheads just below it (exactly as in the
  reference KlavarSVG).
* Stems are horizontal at the attack time: **→ right hand, ← left hand**
  (chords share one stem to their outer noteheads).
* Time is marked by horizontal bar lines (measures) and dashed beat lines
  (quarter-note grid, default 7.5 mm per quarter, per the klavar standard);
  optional count numbers and subdivision ticks.
* **Note sizes** knob (UI 0–300 %): scales the whole note system — notehead
  circles grow, the space between the staff lines widens, stems reach
  farther and the staff gets wider overall (columns re-fit automatically).
  Stroke weights stay constant (`--note-size`, UI slider).
* Strips are packed **1–3 columns per page** (`--columns`, or the UI slider);
  with multiple columns the staff is compressed horizontally as needed and
  strip heights are balanced so every column fills evenly. Default is
  automatic (as many natural-width columns as fit).
* Notes shorter than their grid slot get a **release sign** (small circle on the
  stem); notes held across a bar line get **continuation dots** on the crossed
  bar line.
* Hand assignment: automatic (per part; a single part splits at middle C),
  force all-RH / all-LH, split at middle C, or strictly per part.

## CLI

```
cargo run --release --bin klavar-conv -- input.musicxml -o out.pdf
cargo run --release --bin klavar-conv -- input.mid --quarter-mm 9 --hand 3 --columns 2 --note-size 1.4
```

## Web app

```
./scripts/build_web.sh                 # → web-app/
python3 -m http.server -d web-app 8080 # → http://localhost:8080
```

Drag-and-drop (or samples), options for spacing, columns per page (1–3
slider), note sizes (0.5–2.5× slider), hands and annotations, live PDF
preview, and a download button named
from the piece's title. `.mxl` files are unzipped in JS
(`DecompressionStream`) before handing bytes to the wasm module.

## Tests

```
cargo test                    # 9 integration tests
python3 scripts/verify_pdf.py # geometry checks on rendered PDFs (needs pymupdf)
```

## Notes & simplifications

* Inputs are quantized to a 60-per-quarter-note grid (the Klavar standard).
* Instead of octave-shift diamond markers (used for very wide ranges), the
  staff is printed as wide as the piece needs — the range always fits one system.
* Ties merge into held notes; dotted durations render as the note plus dots on
  each crossed bar line.
* Clean-room: written from the published notation description and reference
  engravings (SVG/keyboard pictures); no original binary was disassembled.
