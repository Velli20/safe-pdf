//! Failure signatures that cluster cases sharing a likely root cause.

use crate::{
    model::{DrawRef, ErrorDetail, PageOutput, ProcessEvidence, Region, RegionClass, Status},
    source_hints,
};

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

/// Replaces quoted names that contain a digit, such as resource names `'F0'` or `'Im12'`, so
/// errors naming different resources cluster together. Quoted words without digits, such as a
/// shading type, stay: they usually name what is unsupported.
fn normalize_names(message: &str) -> String {
    let mut result = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(open) = rest.find('\'') {
        let (head, tail) = rest.split_at(open);
        result.push_str(head);
        let quoted = tail.get(1..).unwrap_or_default();
        match quoted.find('\'') {
            Some(close)
                if quoted.get(..close).is_some_and(|name| {
                    !name.is_empty()
                        && name.len() <= 32
                        && name.chars().any(|c| c.is_ascii_digit())
                        && name.chars().all(|c| c.is_ascii_graphic())
                }) =>
            {
                result.push_str("'…'");
                rest = quoted.get(close.saturating_add(1)..).unwrap_or_default();
            }
            _ => {
                result.push('\'');
                rest = quoted;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Signature of a Safe-PDF error: status, normalized message, and where the error is defined.
///
/// The defining enum variant, found through its `#[error]` template, locates the cause far
/// better than the backtrace, whose innermost frame is usually the entry point that surfaced
/// the error. The backtrace frame is only used when no template matches.
pub fn error(status: Status, detail: &ErrorDetail) -> String {
    let innermost = detail
        .chain
        .last()
        .map_or(detail.message.as_str(), String::as_str);
    let origin = source_hints::origin(innermost)
        .map(|(krate, variant)| format!(" @ {krate}::{variant}"))
        .or_else(|| innermost_frame(detail).map(|function| format!(" @ {function}")))
        .unwrap_or_default();
    format!(
        "{}: {}{origin}",
        status.as_str(),
        normalize(&normalize_names(innermost))
    )
}

/// Signature of a Safe-PDF error as computed before root-cause clustering. Issues filed
/// with it are found again through it, so it must not change.
pub fn legacy_error(status: Status, detail: &ErrorDetail) -> String {
    let innermost = detail
        .chain
        .last()
        .map_or(detail.message.as_str(), String::as_str);
    let frame = innermost_frame(detail)
        .map(|function| format!(" @ {function}"))
        .unwrap_or_default();
    format!("{}: {}{frame}", status.as_str(), normalize(innermost))
}

fn innermost_frame(detail: &ErrorDetail) -> Option<String> {
    detail
        .backtrace
        .as_deref()
        .map(workspace_frames)
        .and_then(|frames| frames.into_iter().next())
        .map(|frame| frame.function)
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

/// Who owns a crash or timeout, decided by the worker stage it stopped in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageOwner {
    /// Safe-PDF was reading, rendering or tracing the document.
    SafePdf,
    /// PDFium was opening or rendering the reference.
    Reference,
    /// The harness itself (inventory, comparison, region analysis, writing files).
    Harness,
}

/// Returns the owner of a worker stage.
pub fn stage_owner(stage: &str) -> StageOwner {
    if stage.starts_with("safe") {
        StageOwner::SafePdf
    } else if stage.starts_with("reference") {
        StageOwner::Reference
    } else {
        StageOwner::Harness
    }
}

/// Returns the stage named by a crash or timeout signature (`… (stage <name>)`).
pub fn signature_stage(signature: &str) -> Option<&str> {
    signature
        .rsplit_once("(stage ")
        .and_then(|(_, rest)| rest.strip_suffix(')'))
}

/// Returns true when the region's difference is in text.
fn is_text(region: &Region) -> bool {
    !region.reference_text.is_empty() || !region.safe_text.is_empty()
}

fn is_blank(output: &PageOutput) -> bool {
    output
        .metrics
        .as_ref()
        .is_some_and(|metrics| metrics.safe_ink == 0.0 && metrics.reference_ink > 0.0005)
}

/// Returns true when every difference on the page is text in a document with non-embedded
/// fonts. Renderers substitute such fonts differently, so these pages are expected to differ
/// and are reported as `font_substitution` rather than as failures.
pub fn is_font_substitution(output: &PageOutput, non_embedded_fonts: bool) -> bool {
    non_embedded_fonts
        && !is_blank(output)
        && !output.regions.is_empty()
        && output.regions.iter().all(is_text)
}

/// Returns the paint features of a draw call that are likely to explain a difference.
fn draw_features(draw: &DrawRef) -> Vec<String> {
    let detail = draw.detail.as_str();
    let mut features = Vec::new();
    let paint = [
        ("axial shading", "axial shading"),
        ("radial shading", "radial shading"),
        ("rasterized shading", "function or mesh shading"),
        ("tiling pattern", "tiling pattern"),
    ]
    .into_iter()
    .find(|(needle, _)| detail.contains(needle))
    .map(|(_, name)| name.to_owned());
    features.extend(paint);
    if draw.kind.ends_with("image")
        && let Some(format) = detail
            .split(", ")
            .next()
            .and_then(|size_and_format| size_and_format.split_whitespace().nth(1))
    {
        features.push(format.to_owned());
    }
    if let Some(mode) = detail
        .split(", ")
        .find_map(|part| part.strip_prefix("blend "))
    {
        features.push(format!("blend {mode}"));
    }
    if detail.contains("soft mask") {
        features.push("soft mask".to_owned());
    }
    features
}

/// Signature of a visual mismatch from its most telling region and what was drawn there.
///
/// In documents with non-embedded fonts, text regions are skipped in favour of the first
/// other region, since renderers substitute such fonts differently. The drawing call's paint
/// (shading type, pattern, image format, blend mode, soft mask) separates causes that the
/// region class alone would lump together.
pub fn mismatch(output: &PageOutput, non_embedded_fonts: bool) -> String {
    if is_blank(output) {
        return "mismatch: blank page".to_owned();
    }
    let Some(region) = output
        .regions
        .iter()
        .find(|region| !(non_embedded_fonts && is_text(region)))
        .or_else(|| output.regions.first())
    else {
        return "mismatch: scattered differences".to_owned();
    };
    let subject = if !region.reference_text.is_empty() && region.safe_text.is_empty() {
        "text missing".to_owned()
    } else if is_text(region) {
        "text".to_owned()
    } else if let Some(draw) = region.draws.first() {
        let features = draw_features(draw);
        if features.is_empty() {
            draw.kind.clone()
        } else {
            format!("{} ({})", draw.kind, features.join(", "))
        }
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

/// Signature of a visual mismatch as computed before root-cause clustering. Issues filed
/// with it are found again through it, so it must not change.
pub fn legacy_mismatch(output: &PageOutput, non_embedded_fonts: bool) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Metrics;

    #[test]
    fn normalizes_resource_names_but_not_feature_names() {
        assert_eq!(
            normalize_names("Font resource 'F2' was not found"),
            "Font resource '…' was not found"
        );
        assert_eq!(
            normalize_names("Pattern resource 'sh6' was not found"),
            "Pattern resource '…' was not found"
        );
        assert_eq!(
            normalize_names("Shading type 'LatticeFormTriangleMesh' not implemented"),
            "Shading type 'LatticeFormTriangleMesh' not implemented"
        );
        assert_eq!(normalize_names("it's 'x1"), "it's 'x1");
    }

    #[test]
    fn keeps_legacy_error_signatures() {
        let detail = ErrorDetail {
            message: "Font resource 'F2' was not found at 12".to_owned(),
            chain: Vec::new(),
            backtrace: None,
        };
        assert_eq!(
            legacy_error(Status::RenderError, &detail),
            "render_error: Font resource 'F2' was not found at N"
        );
    }

    fn region(text: &str, detail: &str) -> Region {
        Region {
            pixels: [0, 0, 10, 10],
            page_space: None,
            mismatched: 100,
            class: RegionClass::ExtraInk,
            shift: None,
            reference_text: text.to_owned(),
            safe_text: text.to_owned(),
            draws: vec![DrawRef {
                seq: 1,
                kind: "fill".to_owned(),
                detail: detail.to_owned(),
                bounds: [0.0, 0.0, 10.0, 10.0],
            }],
        }
    }

    fn page(regions: Vec<Region>) -> PageOutput {
        PageOutput {
            metrics: Some(Metrics {
                mismatch: 0.1,
                safe_ink: 0.1,
                reference_ink: 0.1,
                ..Metrics::default()
            }),
            regions,
            ..PageOutput::default()
        }
    }

    #[test]
    fn separates_font_substitution_from_other_differences() {
        let text_only = page(vec![region("Hello", "#000000, Winding")]);
        assert!(is_font_substitution(&text_only, true));
        assert!(!is_font_substitution(&text_only, false));
        let mixed = page(vec![
            region("Hello", "#000000, Winding"),
            region("", "tiling pattern, blend Multiply, Winding"),
        ]);
        assert!(!is_font_substitution(&mixed, true));
        assert_eq!(
            mismatch(&mixed, true),
            "mismatch: extra_ink / fill (tiling pattern, blend Multiply)"
        );
        assert_eq!(
            legacy_mismatch(&mixed, true),
            "mismatch: extra_ink / text (non-embedded font)"
        );
    }

    #[test]
    fn names_image_formats() {
        let mut image = region("", "32x32 Gray8, rotated 90°");
        if let Some(draw) = image.draws.first_mut() {
            draw.kind = "image".to_owned();
        }
        assert_eq!(
            mismatch(&page(vec![image]), false),
            "mismatch: extra_ink / image (Gray8)"
        );
    }

    #[test]
    fn assigns_stages_to_owners() {
        assert_eq!(stage_owner("safe-render"), StageOwner::SafePdf);
        assert_eq!(stage_owner("reference-open"), StageOwner::Reference);
        assert_eq!(stage_owner("regions"), StageOwner::Harness);
        assert_eq!(
            signature_stage("timeout: time limit exceeded (stage regions)"),
            Some("regions")
        );
    }
}
