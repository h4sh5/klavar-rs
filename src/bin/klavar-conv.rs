//! CLI: klavar-conv input.musicxml|input.mid [-o out.pdf] [options]

use klavar_rs::{klavar::Options, to_pdf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut input = None;
    let mut output = None;
    let mut opts = Options::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                output = args.get(i).cloned();
            }
            "--quarter-mm" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    opts.quarter_mm = v;
                }
            }
            "--hand" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    opts.hand_mode = v;
                }
            }
            "--counts" => opts.show_count_numbers = true,
            "--no-subdivisions" => opts.show_subdivisions = false,
            "--columns" => {
                i += 1;
                if let Some(v @ 1..=3) = args.get(i).and_then(|v| v.parse().ok()) {
                    opts.columns = v;
                }
            }
            "--note-size" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    opts.note_size = v;
                }
            }
            "-h" | "--help" => {
                println!(
                    "usage: klavar-conv INPUT.(xml|mid|midi) [-o OUT.pdf] \
                     [--quarter-mm F] [--hand 0|1|2|3|4] [--columns 1|2|3] \
                     [--note-size F] [--counts] [--no-subdivisions]\n\
                     hand: 0=auto 1=all-right 2=all-left 3=split-at-middle-C 4=per-part\n\
                     columns: strips per page (staff is compressed to fit); default: automatic\n\
                     note-size: note/spacing width scale 0.1-3 (line weights unchanged); default: 1.0"
                );
                return;
            }
            s if !s.starts_with('-') && input.is_none() => input = Some(s.to_string()),
            _ => {}
        }
        i += 1;
    }
    let Some(input) = input else {
        eprintln!("no input file given (try --help)");
        std::process::exit(2);
    };
    let data = match std::fs::read(&input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {}: {}", input, e);
            std::process::exit(1);
        }
    };
    match to_pdf(&data, &opts) {
        Ok(pdf) => {
            let out = output.unwrap_or_else(|| {
                let stem = std::path::Path::new(&input)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "klavarscore".into());
                format!("{}.klavar.pdf", stem)
            });
            if let Err(e) = std::fs::write(&out, &pdf) {
                eprintln!("cannot write {}: {}", out, e);
                std::process::exit(1);
            }
            println!("wrote {} ({} bytes)", out, pdf.len());
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
