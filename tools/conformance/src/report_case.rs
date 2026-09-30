//! Per-case evidence bundle: `summary.md` for readers, `report.json` and `objects.txt` for detail.

use crate::{
    baseline::Delta,
    model::{CaseResult, Diagnostic, ErrorDetail, PageResult, ProcessEvidence, Status},
    report_index::{self, Index, IndexCase},
    run::RunOptions,
    signature, source_hints,
};
use anyhow::Result;
use std::{fmt::Write as _, fs, path::Path};

/// Bytes shown on each side of a byte offset in `objects.txt`.
const EXCERPT_RADIUS: usize = 400;
/// Diagnostics listed in the summary.
const DIAGNOSTIC_LIMIT: usize = 15;
/// Lines of worker stderr quoted in the summary.
const STDERR_LINES: usize = 40;

/// Writes the bundle of a failing case; passing cases get none.
pub fn write(options: &RunOptions, out: &Path, result: &CaseResult, index: &Index) -> Result<()> {
    if !report_index::needs_bundle(result) {
        return Ok(());
    }
    let dir = out.join("cases").join(&result.dir);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(result)?,
    )?;
    let excerpts = objects(result)?;
    if !excerpts.is_empty() {
        fs::write(dir.join("objects.txt"), &excerpts)?;
    }
    let entry = index.cases.iter().find(|case| case.id == result.case.id);
    fs::write(
        dir.join("summary.md"),
        summary(options, result, entry, index, !excerpts.is_empty())?,
    )?;
    Ok(())
}

fn delta_text(delta: Delta, baseline: Option<Status>) -> String {
    match (delta, baseline) {
        (Delta::New, _) | (_, None) => "not in baseline".to_owned(),
        (Delta::Regressed, Some(old)) => format!("**regressed** (baseline: {})", old.as_str()),
        (Delta::Improved, Some(old)) => format!("improved (baseline: {})", old.as_str()),
        (Delta::Unchanged, Some(old)) => format!("unchanged (baseline: {})", old.as_str()),
    }
}

fn cluster_text(index: &Index, signature: &str, id: &str) -> String {
    match index.cluster(signature) {
        Some(cluster) if cluster.cases.len() > 1 => format!(
            " (shared by {} failures in {} documents, including {}; see TRIAGE.md)",
            cluster.count,
            cluster.cases.len(),
            cluster
                .cases
                .iter()
                .filter(|other| *other != id)
                .take(3)
                .map(|other| format!("`{other}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => " (unique to this document)".to_owned(),
    }
}

fn summary(
    options: &RunOptions,
    result: &CaseResult,
    entry: Option<&IndexCase>,
    index: &Index,
    has_objects: bool,
) -> Result<String> {
    let mut text = String::new();
    let case = &result.case;
    writeln!(text, "# {}: {}\n", case.id, result.status.as_str())?;
    if let Some(signature) = &result.signature {
        writeln!(
            text,
            "- Signature: `{signature}`{}",
            cluster_text(index, signature, &case.id)
        )?;
    }
    if let Some(entry) = entry {
        writeln!(
            text,
            "- Baseline: {}",
            delta_text(entry.delta, entry.baseline)
        )?;
    }
    writeln!(
        text,
        "- File: `{}`",
        case.path
            .strip_prefix(&options.root)
            .unwrap_or(&case.path)
            .display()
    )?;
    if let Some(sha) = &result.sha256 {
        writeln!(text, "- SHA-256: `{sha}`")?;
    }
    if let Some(note) = &case.note {
        writeln!(text, "- Corpus note: {note}")?;
    }
    if result.md5_mismatch {
        writeln!(
            text,
            "- Warning: the file's MD5 differs from the corpus manifest"
        )?;
    }
    if case.password.is_some() {
        writeln!(text, "- Opened with the manifest password")?;
    }
    writeln!(text, "- Reproduce: `{}`", options.reproduce(&case.id, None))?;
    if options.pdfium.is_some() {
        writeln!(
            text,
            "- Reference: {} at {} px/pt, page content only. Neither renderer draws annotations or form fields here.\n",
            index.pdfium, options.scale
        )?;
    } else {
        let annotated = result.read.as_ref().is_some_and(|read| {
            read.inventory
                .features
                .iter()
                .any(|(feature, _)| feature.starts_with("annotation:"))
        });
        writeln!(
            text,
            "- Reference: official pdfium_tests golden image (PDFium `pdfium_test`, 72 dpi). Goldens include annotations and \
             form fields, but Safe-PDF's compared render is page content only{}.\n",
            if annotated {
                "; this document has annotations, so differences where they sit are expected"
            } else {
                ""
            }
        )?;
    }

    writeln!(text, "## Read\n")?;
    match (&result.read, &result.read_process) {
        (Some(read), _) => {
            match (&read.safe_error, read.safe_pages) {
                (Some(error), _) => {
                    writeln!(text, "Safe-PDF failed to read the document.\n")?;
                    error_block(&mut text, error)?;
                }
                (None, Some(pages)) => writeln!(text, "Safe-PDF loaded {pages} pages.")?,
                (None, None) => {}
            }
            match (&read.reference_error, read.reference_pages) {
                (Some(error), _) => writeln!(text, "PDFium could not open the document: {error}")?,
                (None, Some(pages)) => writeln!(text, "PDFium loaded {pages} pages.")?,
                (None, None) => {}
            }
            diagnostics(&mut text, &read.diagnostics, has_objects)?;
            writeln!(text, "\n## Features\n")?;
            if read.safe_error.is_some() {
                writeln!(text, "No inventory: Safe-PDF did not load the document.")?;
            } else if read.inventory.features.is_empty() {
                writeln!(
                    text,
                    "No notable features: device color spaces and standard filters only."
                )?;
            } else {
                let features: Vec<String> = read
                    .inventory
                    .features
                    .iter()
                    .map(|(name, count)| format!("{name} ×{count}"))
                    .collect();
                writeln!(text, "{}\n", features.join(", "))?;
                if !read.inventory.non_embedded_fonts.is_empty() {
                    writeln!(
                        text,
                        "Non-embedded fonts (renderers substitute differently, so small text differences are expected): {}\n",
                        read.inventory.non_embedded_fonts.join(", ")
                    )?;
                }
                writeln!(
                    text,
                    "Likely crates: {}",
                    read.inventory.likely_crates.join(", ")
                )?;
            }
        }
        (None, Some(process)) => {
            writeln!(text, "The read worker failed.\n")?;
            process_block(&mut text, process)?;
        }
        (None, None) => writeln!(text, "The file is not available locally.")?,
    }

    for page in &result.pages {
        page_section(&mut text, options, result, page, index)?;
    }

    writeln!(text, "\n## Legend\n")?;
    writeln!(
        text,
        "- `pN-compare.png`: reference | Safe-PDF | diff. Diff colors: red = ink missing in Safe-PDF, \
         blue = extra ink in Safe-PDF, magenta = hue differs, orange = same hue but different intensity (shape), yellow = anti-aliasing or 1 px displacement (tolerated), pale blue hatching = annotation area (not compared), gray = matching.\n\
         - Region classes: missing_ink / extra_ink = one renderer drew nothing there; color_shift = same shapes, different color; \
         reshaped = both drew but glyph shapes or geometry differ (often font substitution); offset = same content shifted by a few pixels.\n\
         - `pN-regionK.png`: reference | Safe-PDF crops of region K, enlarged.\n\
         - Region pixels are `[x0, y0, x1, y1]` from the top-left. Page space is PDF user space `[left, bottom, right, top]`.\n\
         - Draw `#n` is the n-th Safe-PDF backend call on the page. Bounds are device pixels after clipping, and draws are listed tightest fit first.\n\
         - `pN-content.txt` lists the page's content stream operators. `report.json` has full backtraces and worker stderr."
    )?;
    Ok(text)
}

fn page_section(
    text: &mut String,
    options: &RunOptions,
    result: &CaseResult,
    page: &PageResult,
    index: &Index,
) -> Result<()> {
    writeln!(text, "\n## Page {}: {}\n", page.page, page.status.as_str())?;
    if page.status == Status::FontSubstitution {
        writeln!(
            text,
            "- Only text differs, and the document uses non-embedded fonts that renderers substitute differently. Expected, not a failure."
        )?;
    }
    if let Some(signature) = &page.signature
        && page.status.is_failure()
    {
        writeln!(
            text,
            "- Signature: `{signature}`{}",
            cluster_text(index, signature, &result.case.id)
        )?;
    }
    writeln!(
        text,
        "- Reproduce: `{}`",
        options.reproduce(&result.case.id, Some(page.page))
    )?;
    let Some(output) = &page.output else {
        writeln!(text)?;
        return process_block(text, &page.process);
    };
    if let Some(metrics) = &output.metrics {
        writeln!(
            text,
            "- Mismatch {:.3}% of pixels; mean |ΔRGB| {:.2}; ink coverage reference {:.2}% vs Safe-PDF {:.2}%; {} x {} px; {} Safe-PDF draw calls",
            metrics.mismatch * 100.0,
            metrics.mean_abs_rgb,
            metrics.reference_ink * 100.0,
            metrics.safe_ink * 100.0,
            output.size[0],
            output.size[1],
            output.draw_count
        )?;
        if metrics.ignored > 0.0 {
            writeln!(
                text,
                "- {:.1}% of the page is covered by annotations and was not compared (pale blue hatching in the diff)",
                metrics.ignored * 100.0
            )?;
        }
    }
    if let Some(error) = &output.reference_error {
        writeln!(text, "- PDFium could not render this page: {error}")?;
    }
    let images: Vec<String> = output
        .files
        .iter()
        .filter(|file| file.ends_with(".png"))
        .map(|file| format!("[{file}]({file})"))
        .collect();
    if !images.is_empty() {
        writeln!(text, "- Images: {}", images.join(", "))?;
    }
    if let Some(listing) = output
        .files
        .iter()
        .find(|file| file.ends_with("-content.txt"))
    {
        writeln!(text, "- Content stream: [{listing}]({listing})")?;
    }
    if let Some(error) = &output.safe_error
        && result.status != Status::ReadError
    {
        writeln!(text, "\nSafe-PDF failed to render the page.\n")?;
        error_block(text, error)?;
    }
    if !output.regions.is_empty() {
        writeln!(
            text,
            "\n| # | class | pixels | page space | differing px | reference text | Safe-PDF text |\n|---|---|---|---|---|---|---|"
        )?;
        for (rank, region) in output.regions.iter().enumerate() {
            let class = match region.shift {
                Some([dx, dy]) => format!("offset ({dx:+}, {dy:+}) px"),
                None => region.class.as_str().to_owned(),
            };
            writeln!(
                text,
                "| {rank} | {class} | {:?} | {} | {} | {} | {} |",
                region.pixels,
                region
                    .page_space
                    .map(|[l, b, r, t]| format!("[{l:.1}, {b:.1}, {r:.1}, {t:.1}]"))
                    .unwrap_or_default(),
                region.mismatched,
                cell(&region.reference_text),
                cell(&region.safe_text)
            )?;
        }
        for (rank, region) in output.regions.iter().enumerate().take(3) {
            if region.draws.is_empty() {
                writeln!(
                    text,
                    "\nRegion {rank}: no Safe-PDF draw call touches this area."
                )?;
                continue;
            }
            writeln!(text, "\nRegion {rank} Safe-PDF draws:")?;
            for draw in &region.draws {
                let [x0, y0, x1, y1] = draw.bounds;
                writeln!(
                    text,
                    "- #{} {} {} at [{x0:.0}, {y0:.0}, {x1:.0}, {y1:.0}]",
                    draw.seq, draw.kind, draw.detail
                )?;
            }
        }
    }
    let stderr = stderr_tail(&page.process.stderr);
    if !stderr.trim().is_empty() && page.status.is_failure() {
        writeln!(text, "\nWorker stderr (tail):\n\n```\n{stderr}\n```")?;
    }
    Ok(())
}

fn cell(text: &str) -> String {
    if text.is_empty() {
        return "—".to_owned();
    }
    // Line breaks stay visible: stray control characters in extracted text are evidence.
    let flat = text
        .replace("\r\n", "↵")
        .replace(['\n', '\r'], "↵")
        .replace('|', "\\|");
    format!("`{}`", flat.replace('`', "'"))
}

/// Returns the error message followed by its causes, skipping causes that repeat the
/// message above them (wrappers often display their source unchanged).
pub fn distinct_chain(error: &ErrorDetail) -> Vec<&str> {
    let mut messages: Vec<&str> = vec![error.message.as_str()];
    for cause in &error.chain {
        if messages
            .last()
            .is_none_or(|previous| !previous.ends_with(cause.as_str()))
        {
            messages.push(cause);
        }
    }
    messages
}

fn error_block(text: &mut String, error: &ErrorDetail) -> Result<()> {
    let messages = distinct_chain(error);
    writeln!(text, "```")?;
    for (depth, message) in messages.iter().enumerate() {
        if depth == 0 {
            writeln!(text, "{message}")?;
        } else {
            writeln!(text, "Caused by: {message}")?;
        }
    }
    writeln!(text, "```")?;
    let mut hints: Vec<String> = Vec::new();
    for message in &messages {
        for hint in source_hints::locate(message) {
            if !hints.contains(&hint) {
                hints.push(hint);
            }
        }
    }
    if !hints.is_empty() {
        writeln!(text, "\nDefined at: {}", hints.join(", "))?;
    }
    if error.backtrace.is_none() {
        writeln!(
            text,
            "\nNo backtrace: this error was constructed directly rather than through a traced `From` conversion. \
             Search for constructions of the variant above."
        )?;
    }
    if let Some(backtrace) = &error.backtrace {
        let frames = signature::workspace_frames(backtrace);
        if !frames.is_empty() {
            writeln!(
                text,
                "\nWorkspace frames, innermost first (full trace in report.json):\n"
            )?;
            for frame in frames {
                writeln!(text, "- `{}` at `{}`", frame.function, frame.location)?;
            }
        }
    }
    Ok(())
}

fn diagnostics(text: &mut String, diagnostics: &[Diagnostic], has_objects: bool) -> Result<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }
    writeln!(
        text,
        "\n{} recoverable read diagnostics{}:\n",
        diagnostics.len(),
        if has_objects {
            " (raw bytes near each in objects.txt)"
        } else {
            ""
        }
    )?;
    for diagnostic in diagnostics.iter().take(DIAGNOSTIC_LIMIT) {
        let origin = diagnostic
            .backtrace
            .as_deref()
            .map(signature::workspace_frames)
            .and_then(|frames| frames.into_iter().next())
            .map(|frame| format!(" @ `{}`", frame.location))
            .unwrap_or_default();
        writeln!(
            text,
            "- {}{}{}{}: {}{origin}",
            diagnostic.kind,
            diagnostic
                .object
                .as_ref()
                .map(|o| format!(" obj {o}"))
                .unwrap_or_default(),
            diagnostic
                .byte_offset
                .map(|o| format!(" offset {o}"))
                .unwrap_or_default(),
            diagnostic
                .page
                .map(|p| format!(" page {p}"))
                .unwrap_or_default(),
            diagnostic.message.replace('\n', " ")
        )?;
    }
    if diagnostics.len() > DIAGNOSTIC_LIMIT {
        writeln!(
            text,
            "- … {} more in report.json",
            diagnostics.len().saturating_sub(DIAGNOSTIC_LIMIT)
        )?;
    }
    Ok(())
}

fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.starts_with(crate::process::STAGE_MARKER))
        .collect();
    lines
        .get(lines.len().saturating_sub(STDERR_LINES)..)
        .unwrap_or_default()
        .join("\n")
}

fn process_block(text: &mut String, process: &ProcessEvidence) -> Result<()> {
    writeln!(
        text,
        "Worker {} after {} ms in stage `{}` ({}).",
        if process.timed_out {
            "timed out"
        } else {
            "crashed"
        },
        process.elapsed_ms,
        process.stage.as_deref().unwrap_or("startup"),
        process.exit_status
    )?;
    if process
        .stage
        .as_deref()
        .is_some_and(|stage| stage.starts_with("reference"))
    {
        writeln!(text, "The failing stage is PDFium's, not Safe-PDF's.")?;
    }
    if let Some((location, message)) = signature::panic_message(&process.stderr) {
        writeln!(text, "\nPanic at `{location}`: {message}")?;
    }
    if let Some(stdout) = &process.stdout {
        writeln!(text, "\n{stdout}")?;
    }
    let stderr = stderr_tail(&process.stderr);
    if !stderr.trim().is_empty() {
        writeln!(text, "\n```\n{stderr}\n```")?;
    }
    Ok(())
}

/// Returns raw-byte excerpts around diagnostic offsets, diagnostic objects, and offsets in errors.
fn objects(result: &CaseResult) -> Result<String> {
    let Some(read) = &result.read else {
        return Ok(String::new());
    };
    let mut targets: Vec<(String, Option<usize>, Option<String>)> = read
        .diagnostics
        .iter()
        .map(|d| {
            (
                format!("{}: {}", d.kind, d.message),
                d.byte_offset,
                d.object.clone(),
            )
        })
        .collect();
    if let Some(error) = &read.safe_error {
        for message in std::iter::once(&error.message).chain(&error.chain) {
            if let Some(offset) = offset_in(message) {
                targets.push((message.clone(), Some(offset), None));
            }
        }
    }
    targets.retain(|(_, offset, object)| offset.is_some() || object.is_some());
    if targets.is_empty() {
        return Ok(String::new());
    }
    let bytes = fs::read(&result.case.path)?;
    let mut text = String::new();
    for (label, offset, object) in targets.iter().take(20) {
        let position = offset.or_else(|| {
            let needle = format!("{} obj", object.as_deref().unwrap_or_default());
            find_object(&bytes, needle.as_bytes())
        });
        let Some(position) = position else {
            continue;
        };
        let start = position.saturating_sub(EXCERPT_RADIUS);
        let end = position.saturating_add(EXCERPT_RADIUS).min(bytes.len());
        writeln!(
            text,
            "=== {label}\n--- bytes {start}..{end} (offset {position} marked with ⟦⟧)"
        )?;
        writeln!(
            text,
            "{}⟦⟧{}\n",
            escape(bytes.get(start..position).unwrap_or_default()),
            escape(bytes.get(position..end).unwrap_or_default())
        )?;
    }
    Ok(text)
}

fn offset_in(message: &str) -> Option<usize> {
    let rest = message.split("offset ").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Finds `N G obj` not preceded by a digit, so `2 0 obj` does not match inside `12 0 obj`.
fn find_object(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .find(|(position, window)| {
            *window == needle
                && position
                    .checked_sub(1)
                    .and_then(|before| haystack.get(before))
                    .is_none_or(|byte| !byte.is_ascii_digit())
        })
        .map(|(position, _)| position)
}

fn escape(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| match byte {
            b'\n' => "\n".to_owned(),
            b'\r' => "\\r".to_owned(),
            b'\t' => "\t".to_owned(),
            0x20..=0x7e => char::from(byte).to_string(),
            _ => format!("\\x{byte:02x}"),
        })
        .collect()
}
