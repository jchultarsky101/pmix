//! Where the time goes when reading one file.
//!
//! Not a product surface: a development aid for the performance work, so
//! that a change can be attributed to a phase rather than guessed at.
//!
//! ```text
//! cargo build --release --example phases
//! ./target/release/examples/phases model.stp 5
//! ```
//!
//! The optional second argument repeats the extraction and reports the
//! mean, which steadies the reading on a busy machine. Running `pmix`
//! itself with `--verbose` reports the same split one walker at a time.

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: phases <file> [repeat]");
    let repeat: u32 = args.next().and_then(|n| n.parse().ok()).unwrap_or(1);
    let path = std::path::Path::new(&path);
    let bytes = std::fs::read(path).expect("read the file");

    let t = Instant::now();
    let exchange = pmix::step::p21::parse_bytes(&bytes).expect("parse");
    let parse = t.elapsed();

    let t = Instant::now();
    let mut document = None;
    for _ in 0..repeat.max(1) {
        document = Some(pmix::step::pmi::extract(
            &exchange,
            "phases.stp",
            &Default::default(),
        ));
    }
    let walk = t.elapsed() / repeat.max(1);
    let document = document.expect("one extraction");

    let t = Instant::now();
    let json = serde_json::to_string_pretty(&document).expect("serialise");
    let write = t.elapsed();

    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    println!(
        "parse {:>7.1}ms  walk {:>7.1}ms  json {:>6.1}ms   {} instances, {} bytes of JSON",
        ms(parse),
        ms(walk),
        ms(write),
        exchange.len(),
        json.len()
    );
}
