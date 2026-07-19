use std::{env, fs, hint::black_box, path::Path};

use pdf_document::reader::PdfReader;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os().nth(1).ok_or("missing PDF path")?;
    let bytes = fs::read(Path::new(&path))?;
    let byte_count = bytes.len();

    let profiler = dhat::Profiler::new_heap();
    let report = PdfReader.read_with_report(&bytes, None)?;
    println!(
        "mode=parse-dhat bytes={byte_count} pages={} diagnostics={}",
        report.document().page_count(),
        report.diagnostics().len()
    );
    black_box(report);
    drop(profiler);
    Ok(())
}
