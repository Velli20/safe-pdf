use std::{env, fs, hint::black_box, path::Path, time::Instant};

use pdf_document::reader::PdfReader;

fn parse(bytes: &[u8]) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let report = PdfReader.read_with_report(bytes, None)?;
    let pages = report.document().page_count();
    let diagnostics = report.diagnostics().len();
    black_box(report);
    Ok((pages, diagnostics))
}
fn profile_io(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let bytes = fs::read(path)?;
    let elapsed = started.elapsed();
    let byte_count = black_box(bytes.len());
    println!("mode=io bytes={byte_count} elapsed_ns={}", elapsed.as_nanos());
    Ok(())
}

fn profile_parse(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let byte_count = bytes.len();
    let started = Instant::now();
    let (pages, diagnostics) = parse(&bytes)?;
    let elapsed = started.elapsed();
    println!(
        "mode=parse bytes={byte_count} elapsed_ns={} pages={pages} diagnostics={diagnostics}",
        elapsed.as_nanos()
    );
    Ok(())
}

fn profile_end_to_end(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let bytes = fs::read(path)?;
    let byte_count = bytes.len();
    let (pages, diagnostics) = parse(&bytes)?;
    let elapsed = started.elapsed();
    println!(
        "mode=end-to-end bytes={byte_count} elapsed_ns={} pages={pages} diagnostics={diagnostics}",
        elapsed.as_nanos()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mode = args.next().ok_or("missing mode: io, parse, or end-to-end")?;
    let path = args.next().ok_or("missing PDF path")?;
    if args.next().is_some() {
        return Err("unexpected extra argument".into());
    }

    let path = Path::new(&path);
    match mode.to_str() {
        Some("io") => profile_io(path),
        Some("parse") => profile_parse(path),
        Some("end-to-end") => profile_end_to_end(path),
        _ => Err("unknown mode: expected io, parse, or end-to-end".into()),
    }
}
