//! Failure signatures that cluster cases sharing a likely root cause.

use crate::model::{ErrorDetail, PageOutput, ProcessEvidence, RegionClass, Status};

/// Frames kept when trimming a backtrace to workspace code.
const FRAME_LIMIT: usize = 20;

/// A backtrace frame inside workspace crates.
pub struct Frame {
    /// Demangled function path without its hash suffix.
    pub function: String,
    /// `crates/<crate>/src/<file>.rs:<line>:<column>`.
    pub location: String,
}

/// Extracts the workspace frames of a `{:#}`-formatted std backtrace, skipping the
/// trace-capture machinery in `pdf-utils` and `From` conversions.
pub fn workspace_frames(backtrace: &str) -> Vec<Frame> {
    let mut frames = Vec::new();
    let mut function: Option<&str> = None;
    for line in backtrace.lines() {
        let trimmed = line.trim();
        if let Some(location) = trimmed.strip_prefix("at ") {
            let Some(function) = function.take() else {
                continue;
            };
            let Some(start) = location.find("/crates/") else {
                continue;
            };
            let location = location.get(start.saturating_add(1)..).unwrap_or(location);
            let function = strip_crate_hashes(strip_hash(function));
            if location.starts_with("crates/pdf-utils/src/error_trace.rs")
                || function.contains("as core::convert::From<")
            {
                continue;
            }
            frames.push(Frame {
                function,
                location: location.to_owned(),
            });
            if frames.len() >= FRAME_LIMIT {
                break;
            }
        } else if let Some((index, name)) = trimmed.split_once(": ")
            && index.chars().all(|c| c.is_ascii_digit())
            && !index.is_empty()
        {
            // Frames print as `N: 0xADDRESS - function`; inlined frames omit the address.
            function = Some(
                name.split_once(" - ")
                    .map_or(name, |(_, function)| function),
            );
        }
    }
    frames
}

fn strip_hash(function: &str) -> &str {
    match function.rsplit_once("::h") {
        Some((head, hash)) if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) => {
            head
        }
        _ => function,
    }
}

/// Removes `[0123abcd]` crate disambiguators from v0-demangled paths.
fn strip_crate_hashes(function: &str) -> String {
    let mut result = String::with_capacity(function.len());
    let mut rest = function;
    while let Some(open) = rest.find('[') {
        let (head, tail) = rest.split_at(open);
        result.push_str(head);
        match tail.find(']') {
            Some(close)
                if tail.get(1..close).is_some_and(|hash| {
                    !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit())
                }) =>
            {
                rest = tail.get(close.saturating_add(1)..).unwrap_or_default();
            }
            _ => {
                result.push('[');
                rest = tail.get(1..).unwrap_or_default();
            }
        }
    }
    result.push_str(rest);
    result
}

/// Replaces numbers so messages differing only in offsets and ids cluster together.
fn normalize(message: &str) -> String {
    // Only standalone numbers are replaced; digits inside identifiers such as JBIG2 stay.
    let mut result = String::new();
    let mut previous: Option<char> = None;
    let mut in_number = false;
    for c in message.chars() {
        if c.is_ascii_digit() && (in_number || !previous.is_some_and(char::is_alphanumeric)) {
            if !in_number {
                result.push('N');
            }
            in_number = true;
        } else {
            in_number = false;
            result.push(c);
        }
        previous = Some(c);
    }
    let mut result: String = result.chars().take(140).collect();
    if message.chars().count() > 140 {
        result.push('…');
    }
    result
}

/// Signature of a Safe-PDF error: status, normalized message, and innermost workspace frame.
pub fn error(status: Status, detail: &ErrorDetail) -> String {
    let innermost = detail
        .chain
        .last()
        .map_or(detail.message.as_str(), String::as_str);
    let frame = detail
        .backtrace
        .as_deref()
        .map(workspace_frames)
        .and_then(|frames| frames.into_iter().next())
        .map(|frame| format!(" @ {}", frame.function))
        .unwrap_or_default();
    format!("{}: {}{frame}", status.as_str(), normalize(innermost))
}

/// Returns the panic location and message from worker stderr.
pub fn panic_message(stderr: &str) -> Option<(String, String)> {
    let mut lines = stderr.lines();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.split_once("panicked at ").map(|(_, rest)| rest) {
            let location = rest.trim_end_matches(':').to_owned();
            let message = lines.next().unwrap_or_default().trim().to_owned();
            return Some((location, message));
        }
    }
    None
}

/// Signature of a crashed or timed-out worker.
pub fn process(status: Status, evidence: &ProcessEvidence) -> String {
    let stage = evidence.stage.as_deref().unwrap_or("startup");
    match panic_message(&evidence.stderr) {
        Some((location, _)) => format!("{}: panic at {location} (stage {stage})", status.as_str()),
        None => format!(
            "{}: {} (stage {stage})",
            status.as_str(),
            if evidence.timed_out {
                "time limit exceeded"
            } else {
                evidence.exit_status.as_str()
            }
        ),
    }
}

/// Signature of a visual mismatch from its largest region and what was drawn there.
///
/// Text differences in documents with non-embedded fonts are labelled separately: renderers
/// substitute different fonts, so those rarely indicate a Safe-PDF defect.
pub fn mismatch(output: &PageOutput, non_embedded_fonts: bool) -> String {
    if let Some(metrics) = &output.metrics
        && metrics.safe_ink == 0.0
        && metrics.reference_ink > 0.0005
    {
        return "mismatch: blank page".to_owned();
    }
    let Some(region) = output.regions.first() else {
        return "mismatch: scattered differences".to_owned();
    };
    let subject = if !region.reference_text.is_empty() && region.safe_text.is_empty() {
        "text missing".to_owned()
    } else if !region.reference_text.is_empty() || !region.safe_text.is_empty() {
        "text".to_owned()
    } else if let Some(draw) = region.draws.first() {
        let shaded = if draw.detail.contains("shading") {
            " (shading)"
        } else if draw.detail.contains("tiling") {
            " (pattern)"
        } else {
            ""
        };
        format!("{}{shaded}", draw.kind)
    } else if region.class == RegionClass::MissingInk {
        "nothing drawn".to_owned()
    } else {
        "unattributed".to_owned()
    };
    let substituted = if non_embedded_fonts && subject.starts_with("text") {
        " (non-embedded font)"
    } else {
        ""
    };
    format!(
        "mismatch: {} / {subject}{substituted}",
        region.class.as_str()
    )
}
