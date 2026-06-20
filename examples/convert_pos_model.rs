//! One-off tool: convert NLTK's `weights.json` into `pos_model/weights.bincode`.
//!
//! Usage:
//!     cargo run --example convert_pos_model -- pos_model/weights.json pos_model/weights.bincode
//!
//! Run this only when updating the vendored model. The output bincode is what
//! `src/pos.rs` embeds at compile time via `include_bytes!`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: convert_pos_model <weights.json> <weights.bincode>");
        process::exit(2);
    }
    let src = &args[1];
    let dst = &args[2];

    let text = fs::read_to_string(src).expect("read weights.json");
    let parsed: HashMap<String, HashMap<String, f64>> =
        serde_json::from_str(&text).expect("parse weights.json");

    // Downcast f64 -> f32 to halve size; the perceptron tolerates the precision loss.
    let weights: HashMap<String, HashMap<String, f32>> = parsed
        .into_iter()
        .map(|(feat, inner)| {
            (
                feat,
                inner.into_iter().map(|(tag, w)| (tag, w as f32)).collect(),
            )
        })
        .collect();

    let bytes = bincode::serialize(&weights).expect("serialize bincode");
    fs::write(dst, &bytes).expect("write weights.bincode");
    eprintln!("wrote {} ({} bytes) from {}", dst, bytes.len(), src);
}
