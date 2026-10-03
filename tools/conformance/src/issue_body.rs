//! Titles, labels and Markdown bodies of conformance issues.
//!
//! A body leads with an alert saying what fails, a table of facts, the suspect source
//! (a permalink GitHub renders as a snippet), images of one case from the published report,
//! the case list, the commands to reproduce and verify a fix, and a JSON block for agents.
//! Long detail sits in collapsed sections.

use crate::{
    baseline::Delta,
    corpus::CorpusKind,
    issue_state::IssueState,
    issues::{Group, Part},
    model::{CaseResult, ErrorDetail, PageOutput, ProcessEvidence, Status},
    report_case,
    report_index::{IndexCase, IndexPage},
    signature::{self, StageOwner},
    source_hints,
};
use anyhow::Result;
use serde_json::json;
use std::fmt::Write as _;

/// GitHub rejects issue bodies above 65,536 characters.
const BODY_LIMIT: usize = 60_000;
/// Cases listed in the table, then in the JSON block.
const CASE_LIMITS: [usize; 3] = [25, 10, 3];
/// Width of each image in the side-by-side table.
const IMAGE_WIDTH: u32 = 280;
/// Workspace frames shown from a backtrace.
const FRAME_LIMIT: usize = 12;
/// Lines of worker stderr shown for crashes.
const STDERR_LINES: usize = 25;

/// Where the issue's links point.
pub struct BodyContext {
    /// `owner/name` of the repository.
    pub repo: String,
    /// Link to the workflow run that produced the data.
    pub run_link: Option<String>,
    /// Commit the run checked, for source permalinks.
    pub sha: Option<String>,
}

impl BodyContext {
    /// Reads the run link and commit from the GitHub Actions environment.
    pub fn from_env(repo: &str) -> Self {
        let run_link = (|| {
            let server = std::env::var("GITHUB_SERVER_URL").ok()?;
            let repository = std::env::var("GITHUB_REPOSITORY").ok()?;
            let id = std::env::var("GITHUB_RUN_ID").ok()?;
            Some(format!("{server}/{repository}/actions/runs/{id}"))
        })();
        Self {
            repo: repo.to_owned(),
            run_link,
            sha: std::env::var("GITHUB_SHA").ok(),
        }
    }

    /// Returns ` in <run link>` for comments, or nothing outside Actions.
    pub fn in_run(&self) -> String {
        self.run_link
            .as_ref()
            .map(|link| format!(" in {link}"))
            .unwrap_or_default()
    }

    /// Returns the published report of a corpus on GitHub Pages, ending in `/`.
    pub fn pages(&self, kind: CorpusKind) -> String {
        pages_url(&self.repo, kind)
    }

    fn source(&self, path: &str, line: usize) -> String {
        format!(
            "https://github.com/{}/blob/{}/{path}#L{line}",
            self.repo,
            self.sha.as_deref().unwrap_or("main")
        )
    }
}

/// Returns the published report of a corpus for a repository, ending in `/`.
pub fn pages_url(repo: &str, kind: CorpusKind) -> String {
    let (owner, name) = repo.split_once('/').unwrap_or((repo, ""));
    format!(
        "https://{}.github.io/{name}/conformance/{}/",
        owner.to_lowercase(),
        kind.as_str()
    )
}

/// Returns the viewer link selecting one case, and optionally one page.
pub fn viewer_link(base: &str, case: &str, page: Option<usize>) -> String {
    let mut link = format!("{base}#case={}", percent_encode(case));
    if let Some(page) = page {
        let _ = write!(link, "&page={page}");
    }
    link
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Returns the plain-words issue title.
pub fn title(group: &Group<'_>) -> String {
    let status = group.status();
    let rest = group
        .signature
        .split_once(": ")
        .map_or(group.signature.as_str(), |(_, rest)| rest);
    let title = match status {
        Status::Mismatch => format!("Render differs: {}", describe_mismatch(rest)),
        Status::ReadError => format!("Read error: {}", strip_origin(rest)),
        Status::RenderError => format!("Render error: {}", strip_origin(rest)),
        _ => {
            let stage = signature::signature_stage(&group.signature).unwrap_or("startup");
            let what = if status == Status::Timeout {
                "timeout"
            } else {
                "crash"
            };
            match signature::stage_owner(stage) {
                StageOwner::Harness => format!("Harness {what} in stage {stage}"),
                StageOwner::Reference => format!("PDFium {what} in stage {stage}"),
                StageOwner::SafePdf if status == Status::Timeout => {
                    format!("Timeout in stage {stage}")
                }
                StageOwner::SafePdf => format!("Crash: {}", strip_stage(rest)),
            }
        }
    };
    let mut title = format!("[conformance] {title}");
    if title.chars().count() > 200 {
        title = title.chars().take(199).collect();
        title.push('…');
    }
    title
}

fn strip_origin(rest: &str) -> &str {
    rest.rsplit_once(" @ ").map_or(rest, |(message, _)| message)
}

fn strip_stage(rest: &str) -> &str {
    rest.rsplit_once(" (stage ")
        .map_or(rest, |(message, _)| message)
}

fn describe_mismatch(rest: &str) -> String {
    let Some((class, subject)) = rest.split_once(" / ") else {
        return rest.to_owned();
    };
    let class = match class {
        "missing_ink" => "missing ink",
        "extra_ink" => "extra ink",
        "color_shift" => "wrong color",
        "reshaped" => "different shape",
        "offset" => "offset",
        other => other,
    };
    format!("{class} in {subject}")
}

/// Returns the crate named by an error signature's origin (`… @ pdf-canvas::PathRequired`).
fn origin_crate(signature: &str) -> Option<&str> {
    let (_, origin) = signature.rsplit_once(" @ ")?;
    let (krate, _) = origin.split_once("::")?;
    (krate.starts_with("pdf-") && !krate.contains(['<', ' '])).then_some(krate)
}

/// Returns the labels of a group.
pub fn labels(group: &Group<'_>) -> Vec<String> {
    let mut labels = vec!["conformance".to_owned()];
    labels.extend(
        group
            .parts
            .iter()
            .map(|part| format!("conformance:{}", part.run.kind.as_str())),
    );
    if matches!(group.status(), Status::Crash | Status::Timeout) {
        let stage = signature::signature_stage(&group.signature).unwrap_or("startup");
        labels.push(
            match signature::stage_owner(stage) {
                StageOwner::SafePdf => "crash",
                StageOwner::Harness => "harness",
                StageOwner::Reference => "reference",
            }
            .to_owned(),
        );
    }
    if group.regressed() {
        labels.push("regression".to_owned());
    }
    if let Some(krate) = origin_crate(&group.signature) {
        labels.push(format!("area:{krate}"));
    }
    labels
}

/// Returns the color and description of a label.
pub fn label_style(label: &str) -> (&'static str, String) {
    match label {
        "conformance" => ("5319e7", "Filed by the conformance workflow".to_owned()),
        "crash" => ("b60205", "Safe-PDF panics, aborts or times out".to_owned()),
        "harness" => (
            "fbca04",
            "The conformance harness failed, not Safe-PDF".to_owned(),
        ),
        "reference" => ("fbca04", "The PDFium reference failed".to_owned()),
        "regression" => ("d93f0b", "Worse than the recorded baseline".to_owned()),
        other => match other.strip_prefix("area:") {
            Some(krate) => ("0e8a16", format!("Conformance failures defined in {krate}")),
            None => ("c5def5", "Conformance corpus".to_owned()),
        },
    }
}

/// One case of the group with the pages that carry its signature.
struct Entry<'a> {
    part: &'a Part<'a>,
    case: &'a IndexCase,
    pages: Vec<&'a IndexPage>,
}

impl Entry<'_> {
    fn result(&self) -> Option<&CaseResult> {
        self.part.run.results.get(&self.case.id)
    }

    fn worst_mismatch(&self) -> Option<f64> {
        self.pages
            .iter()
            .filter_map(|page| page.mismatch)
            .fold(None, |max: Option<f64>, value| {
                Some(max.map_or(value, |m| m.max(value)))
            })
    }

    fn images(&self) -> usize {
        self.pages.first().map_or(0, |page| {
            page.files
                .iter()
                .filter(|file| file.ends_with(".png"))
                .count()
        })
    }

    fn delta(&self) -> &'static str {
        let deltas: Vec<Delta> = if self.pages.is_empty() {
            vec![self.case.delta]
        } else {
            self.pages.iter().map(|page| page.delta).collect()
        };
        if deltas.contains(&Delta::Regressed) {
            "**regressed**"
        } else if deltas.iter().all(|delta| *delta == Delta::New) {
            "not in baseline"
        } else if deltas.contains(&Delta::Improved) {
            "improved"
        } else {
            "unchanged"
        }
    }

    /// Link to the PDF in its upstream repository at the corpus revision.
    fn pdf_link(&self) -> Option<String> {
        let result = self.result()?;
        let root = std::path::Path::new(&self.part.run.index.corpus_root);
        let relative = result
            .case
            .path
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        let kind = self.part.run.kind;
        let (url, pinned) = kind.source();
        let revision = self
            .part
            .run
            .index
            .corpus_revision
            .as_deref()
            .unwrap_or(pinned);
        Some(match kind {
            CorpusKind::Pdfjs => {
                let linked = result
                    .case
                    .note
                    .as_deref()
                    .is_some_and(|note| note.contains("linked file"));
                format!(
                    "{url}/blob/{revision}/{relative}{}",
                    if linked { ".link" } else { "" }
                )
            }
            CorpusKind::Pdfium => format!("{url}/+/{revision}/{relative}"),
        })
    }
}

fn entries<'a>(group: &'a Group<'a>) -> Vec<Entry<'a>> {
    let mut entries = Vec::new();
    for part in &group.parts {
        for id in &part.cluster.cases {
            let Some(case) = part.run.index.cases.iter().find(|case| &case.id == id) else {
                continue;
            };
            let pages = case
                .pages
                .iter()
                .filter(|page| page.signature.as_ref() == Some(&group.signature))
                .collect();
            entries.push(Entry { part, case, pages });
        }
    }
    entries.sort_by(|a, b| {
        b.worst_mismatch()
            .partial_cmp(&a.worst_mismatch())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.case.id.cmp(&b.case.id))
    });
    entries
}

/// Renders the issue body, shortening lists until it fits GitHub's limit.
pub fn render(group: &Group<'_>, state: &IssueState, context: &BodyContext) -> Result<String> {
    let mut body = String::new();
    for limit in CASE_LIMITS {
        body = render_with(group, state, context, limit)?;
        if body.len() <= BODY_LIMIT {
            break;
        }
    }
    Ok(body)
}

fn render_with(
    group: &Group<'_>,
    state: &IssueState,
    context: &BodyContext,
    limit: usize,
) -> Result<String> {
    let entries = entries(group);
    let showcase = entries
        .iter()
        .max_by_key(|entry| (entry.images(), entry.result().is_some()))
        .or_else(|| entries.first());
    let mut text = String::new();
    writeln!(text, "{}\n", state.marker()?)?;
    headline(&mut text, group)?;
    facts(&mut text, group, context)?;
    let suspect = showcase.and_then(|entry| suspect(entry, group.status()));
    if let Some((path, line, _)) = &suspect
        && context.sha.is_some()
    {
        // A permalink alone on a line renders as a code snippet on GitHub.
        writeln!(text, "{}\n", context.source(path, *line))?;
    }
    if let Some(entry) = showcase {
        example(&mut text, entry, group, context)?;
    }
    cases(&mut text, &entries, context, limit)?;
    reproduce(&mut text, group)?;
    agents(&mut text, group, &entries, suspect.as_ref(), context, limit)?;
    writeln!(
        text,
        "---\n<sub>Filed by the conformance workflow. Key `{}`. Close as *not planned* to stop updates. \
         How to read the images and the workflow: [tools/conformance/README.md](https://github.com/{}/blob/main/tools/conformance/README.md).</sub>",
        group.key, context.repo
    )?;
    Ok(text)
}

fn headline(text: &mut String, group: &Group<'_>) -> Result<()> {
    let (failures, documents) = (group.failures(), group.documents());
    let pages = plural(failures, "page");
    let docs = plural(documents, "document");
    let status = group.status();
    let (mut alert, sentence) = match status {
        Status::ReadError => (
            "WARNING",
            format!("Safe-PDF cannot read {docs}, so none of their pages render."),
        ),
        Status::RenderError => (
            "WARNING",
            format!("Safe-PDF fails to render {pages} in {docs}."),
        ),
        Status::Mismatch => (
            "NOTE",
            format!("Safe-PDF's render differs from PDFium's on {pages} in {docs}."),
        ),
        _ => {
            let stage = signature::signature_stage(&group.signature).unwrap_or("startup");
            let what = if status == Status::Timeout {
                "runs past the time limit"
            } else {
                "crashes"
            };
            match signature::stage_owner(stage) {
                StageOwner::SafePdf => (
                    "CAUTION",
                    format!("Safe-PDF {what} in stage `{stage}` on {docs}."),
                ),
                StageOwner::Harness => (
                    "NOTE",
                    format!(
                        "The conformance harness {what} in stage `{stage}` on {docs}. \
                         This is a harness problem, not a Safe-PDF failure."
                    ),
                ),
                StageOwner::Reference => (
                    "NOTE",
                    format!(
                        "The PDFium reference {what} in stage `{stage}` on {docs}. Nothing to compare against."
                    ),
                ),
            }
        }
    };
    let mut sentence = sentence;
    if group.regressed() {
        alert = "CAUTION";
        sentence.push_str(
            " Some of these were better in the recorded baseline, so this is a regression.",
        );
    }
    writeln!(text, "> [!{alert}]\n> {sentence}\n")?;
    Ok(())
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn facts(text: &mut String, group: &Group<'_>, context: &BodyContext) -> Result<()> {
    writeln!(text, "| | |\n|---|---|")?;
    writeln!(
        text,
        "| Signature | `{}` |",
        group.signature.replace('|', "\\|")
    )?;
    let corpora: Vec<String> = group
        .parts
        .iter()
        .map(|part| {
            format!(
                "[{}]({}) {} in {}",
                part.run.kind.as_str(),
                context.pages(part.run.kind),
                plural(part.cluster.count, "failure"),
                plural(part.cluster.cases.len(), "document")
            )
        })
        .collect();
    writeln!(text, "| Corpora | {} |", corpora.join(" · "))?;
    let deltas = group.deltas();
    let regressed = deltas
        .iter()
        .filter(|delta| **delta == Delta::Regressed)
        .count();
    let baseline = if deltas.iter().all(|delta| *delta == Delta::New) {
        "not in the baseline".to_owned()
    } else if regressed > 0 {
        format!("**{} regressed**", plural(regressed, "failure"))
    } else {
        "known failure, unchanged".to_owned()
    };
    writeln!(text, "| Baseline | {baseline} |")?;
    let reference = group
        .parts
        .first()
        .map(|part| part.run.index.pdfium.clone())
        .unwrap_or_default();
    writeln!(text, "| Reference | {reference} |")?;
    if let Some(link) = &context.run_link {
        writeln!(
            text,
            "| Latest run | [workflow run]({link}){} |",
            context
                .sha
                .as_deref()
                .and_then(|sha| sha.get(..7))
                .map(|sha| format!(" at `{sha}`"))
                .unwrap_or_default()
        )?;
    }
    writeln!(text)?;
    Ok(())
}

/// Returns `(path, line, variant)` of the error template behind a case's failure.
fn suspect(entry: &Entry<'_>, status: Status) -> Option<(String, usize, String)> {
    let error = error_of(entry, status)?;
    let innermost = error.chain.last().unwrap_or(&error.message);
    let (path, line) = source_hints::location(innermost)?;
    let variant = source_hints::origin(innermost)
        .map(|(_, variant)| variant)
        .unwrap_or_default();
    Some((path, line, variant))
}

fn error_of<'a>(entry: &'a Entry<'_>, status: Status) -> Option<&'a ErrorDetail> {
    let result = entry.result()?;
    match status {
        Status::ReadError => result.read.as_ref()?.safe_error.as_ref(),
        Status::RenderError => page_output(entry)?.safe_error.as_ref(),
        _ => None,
    }
}

fn page_output<'a>(entry: &'a Entry<'_>) -> Option<&'a PageOutput> {
    let page = entry.pages.first()?.page;
    entry
        .result()?
        .pages
        .iter()
        .find(|result| result.page == page)?
        .output
        .as_ref()
}

fn process_of<'a>(entry: &'a Entry<'_>) -> Option<&'a ProcessEvidence> {
    let result = entry.result()?;
    match entry.pages.first() {
        Some(page) => result
            .pages
            .iter()
            .find(|result| result.page == page.page)
            .map(|result| &result.process),
        None => result.read_process.as_ref(),
    }
}

fn example(
    text: &mut String,
    entry: &Entry<'_>,
    group: &Group<'_>,
    context: &BodyContext,
) -> Result<()> {
    let base = context.pages(entry.part.run.kind);
    let page = entry.pages.first();
    writeln!(
        text,
        "### Example: `{}` ({}{})\n",
        entry.case.id,
        entry.part.run.kind.as_str(),
        page.map(|page| format!(", page {}", page.page))
            .unwrap_or_default()
    )?;
    if let Some(page) = page {
        let viewer = viewer_link(&base, &entry.case.id, Some(page.page));
        let cells: Vec<String> = [("ref", "PDFium"), ("safe", "Safe-PDF"), ("diff", "Difference")]
            .into_iter()
            .filter_map(|(suffix, caption)| {
                let file = format!("p{}-{suffix}.png", page.page);
                page.files.contains(&file).then(|| {
                    format!(
                        "<td align=\"center\"><a href=\"{viewer}\"><img src=\"{base}cases/{}/{file}\" width=\"{IMAGE_WIDTH}\" alt=\"{caption}\"></a><br><sub>{caption}</sub></td>",
                        entry.case.dir
                    )
                })
            })
            .collect();
        if !cells.is_empty() {
            writeln!(text, "<table><tr>{}</tr></table>\n", cells.join(""))?;
            if page.files.iter().any(|file| file.ends_with("-diff.png")) {
                writeln!(
                    text,
                    "<sub>Difference: red = ink missing in Safe-PDF, blue = extra ink, magenta = hue differs, orange = same hue, different shape. Click an image for the interactive viewer.</sub>\n"
                )?;
            }
        }
    }
    let status = group.status();
    if let Some(error) = error_of(entry, status) {
        writeln!(text, "```text")?;
        for (depth, message) in report_case::distinct_chain(error).iter().enumerate() {
            if depth == 0 {
                writeln!(text, "{message}")?;
            } else {
                writeln!(text, "Caused by: {message}")?;
            }
        }
        writeln!(text, "```\n")?;
        if let Some(backtrace) = &error.backtrace {
            let frames = signature::workspace_frames(backtrace);
            if !frames.is_empty() {
                writeln!(
                    text,
                    "<details><summary>Workspace frames, innermost first</summary>\n"
                )?;
                for frame in frames.iter().take(FRAME_LIMIT) {
                    writeln!(text, "- `{}` at `{}`", frame.function, frame.location)?;
                }
                writeln!(text, "\n</details>\n")?;
            }
        }
    }
    if matches!(status, Status::Crash | Status::Timeout)
        && let Some(process) = process_of(entry)
    {
        if let Some((location, message)) = signature::panic_message(&process.stderr) {
            writeln!(text, "Panic at `{location}`: {message}\n")?;
        }
        let lines: Vec<&str> = process
            .stderr
            .lines()
            .filter(|line| !crate::process::is_marker(line))
            .collect();
        let tail = lines
            .get(lines.len().saturating_sub(STDERR_LINES)..)
            .unwrap_or_default()
            .join("\n");
        writeln!(
            text,
            "Worker {} after {} ms in stage `{}` ({}).\n",
            if process.timed_out {
                "timed out"
            } else {
                "crashed"
            },
            process.elapsed_ms,
            process.stage.as_deref().unwrap_or("startup"),
            process.exit_status
        )?;
        if !tail.trim().is_empty() {
            writeln!(
                text,
                "<details><summary>Worker stderr (tail)</summary>\n\n```text\n{tail}\n```\n\n</details>\n"
            )?;
        }
    }
    if status == Status::Mismatch
        && let Some(output) = page_output(entry)
    {
        regions(text, output)?;
    }
    Ok(())
}

fn regions(text: &mut String, output: &PageOutput) -> Result<()> {
    if let Some(metrics) = &output.metrics {
        writeln!(
            text,
            "{:.2}% of pixels differ; ink coverage PDFium {:.2}% vs Safe-PDF {:.2}%.\n",
            metrics.mismatch * 100.0,
            metrics.reference_ink * 100.0,
            metrics.safe_ink * 100.0
        )?;
    }
    if output.regions.is_empty() {
        return Ok(());
    }
    writeln!(
        text,
        "| Region | Class | Page space | Differing px | PDFium text | Safe-PDF text |\n|---|---|---|---|---|---|"
    )?;
    for (rank, region) in output.regions.iter().enumerate().take(4) {
        writeln!(
            text,
            "| {rank} | {} | {} | {} | {} | {} |",
            region.class.as_str(),
            region
                .page_space
                .map(|[l, b, r, t]| format!("[{l:.0}, {b:.0}, {r:.0}, {t:.0}]"))
                .unwrap_or_default(),
            region.mismatched,
            cell(&region.reference_text),
            cell(&region.safe_text)
        )?;
    }
    writeln!(text)?;
    let draws: Vec<String> = output
        .regions
        .iter()
        .enumerate()
        .take(3)
        .flat_map(|(rank, region)| {
            region.draws.iter().take(5).map(move |draw| {
                let [x0, y0, x1, y1] = draw.bounds;
                format!(
                    "- region {rank}: #{} {} {} at [{x0:.0}, {y0:.0}, {x1:.0}, {y1:.0}]",
                    draw.seq,
                    draw.kind,
                    hex_code(&draw.detail)
                )
            })
        })
        .collect();
    if !draws.is_empty() {
        writeln!(
            text,
            "<details><summary>Safe-PDF draw calls in the regions</summary>\n\n{}\n\n</details>\n",
            draws.join("\n")
        )?;
    }
    Ok(())
}

/// Wraps `#RRGGBB` colors in backticks so GitHub shows a swatch next to them.
fn hex_code(detail: &str) -> String {
    detail
        .split(' ')
        .map(|word| {
            let color = word.trim_end_matches(',');
            if color.len() == 7
                && color.starts_with('#')
                && color.chars().skip(1).all(|c| c.is_ascii_hexdigit())
            {
                word.replacen(color, &format!("`{color}`"), 1)
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn cell(text: &str) -> String {
    if text.is_empty() {
        return "—".to_owned();
    }
    let flat: String = text
        .replace("\r\n", "↵")
        .replace(['\n', '\r'], "↵")
        .replace('|', "\\|")
        .replace('`', "'")
        .chars()
        .take(80)
        .collect();
    format!("`{flat}`")
}

fn cases(
    text: &mut String,
    entries: &[Entry<'_>],
    context: &BodyContext,
    limit: usize,
) -> Result<()> {
    writeln!(text, "### Cases\n")?;
    writeln!(
        text,
        "| Case | Corpus | Pages | Mismatch | Baseline | Links |\n|---|---|---|---|---|---|"
    )?;
    for entry in entries.iter().take(limit) {
        let base = context.pages(entry.part.run.kind);
        let pages = if entry.pages.is_empty() {
            "document".to_owned()
        } else {
            entry
                .pages
                .iter()
                .map(|page| page.page.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut links = vec![format!(
            "[viewer]({})",
            viewer_link(
                &base,
                &entry.case.id,
                entry.pages.first().map(|page| page.page)
            )
        )];
        if entry.case.bundle {
            links.push(format!(
                "[summary]({base}cases/{}/summary.md)",
                entry.case.dir
            ));
        }
        if let Some(pdf) = entry.pdf_link() {
            links.push(format!("[PDF]({pdf})"));
        }
        writeln!(
            text,
            "| `{}` | {} | {pages} | {} | {} | {} |",
            entry.case.id.replace('|', "\\|"),
            entry.part.run.kind.as_str(),
            entry
                .worst_mismatch()
                .map(|value| format!("{:.2}%", value * 100.0))
                .unwrap_or_else(|| "—".to_owned()),
            entry.delta(),
            links.join(" · ")
        )?;
    }
    if entries.len() > limit {
        writeln!(
            text,
            "\n…and {} more; filter the viewer by this signature to see them all.",
            entries.len().saturating_sub(limit)
        )?;
    }
    writeln!(text)?;
    Ok(())
}

fn reproduce(text: &mut String, group: &Group<'_>) -> Result<()> {
    writeln!(
        text,
        "### Reproduce\n\n> [!TIP]\n> Downloads only these PDFs and compares against the published PDFium images, so no local PDFium build is needed. For a corpus that is only read on `main`, it reruns the cases listed below without reference images, which still shows crashes, timeouts and errors:\n>\n> ```console\n> cargo conformance repro {}\n> ```\n>\n> `cargo conformance render <pdf> --page <n>` renders one downloaded PDF and times each Safe-PDF step.\n",
        group.key
    )?;
    writeln!(
        text,
        "### Done when\n\n- [ ] `cargo conformance verify {}` exits 0: no listed case fails with this signature\n- [ ] A unit test next to the fix covers the case\n- [ ] The fixing pull request says `Fixes #<this issue>`\n",
        group.key
    )?;
    Ok(())
}

fn agents(
    text: &mut String,
    group: &Group<'_>,
    entries: &[Entry<'_>],
    suspect: Option<&(String, usize, String)>,
    context: &BodyContext,
    limit: usize,
) -> Result<()> {
    let cases: Vec<serde_json::Value> = entries
        .iter()
        .take(limit)
        .map(|entry| {
            let base = context.pages(entry.part.run.kind);
            json!({
                "corpus": entry.part.run.kind.as_str(),
                "id": entry.case.id,
                "pages": entry.pages.iter().map(|page| page.page).collect::<Vec<_>>(),
                "mismatch": entry.worst_mismatch(),
                "pdf": entry.pdf_link(),
                "summary": entry.case.bundle.then(|| format!("{base}cases/{}/summary.md", entry.case.dir)),
                "run": entry.case.reproduce,
            })
        })
        .collect();
    let value = json!({
        "key": group.key,
        "signature": group.signature,
        "status": group.status().as_str(),
        "failures": group.failures(),
        "documents": group.documents(),
        "suspect": suspect.map(|(path, line, variant)| json!({
            "path": path,
            "line": line,
            "variant": variant,
        })),
        "reproduce": format!("cargo conformance repro {}", group.key),
        "verify": format!("cargo conformance verify {}", group.key),
        "cases": cases,
    });
    writeln!(
        text,
        "<details><summary>For agents: machine-readable summary</summary>\n\n```json\n{}\n```\n\n</details>\n",
        serde_json::to_string_pretty(&value)?
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        issues::{self, CorpusRun},
        model::{Case, PageResult},
        report_index::{Cluster, Index},
    };
    use std::{collections::BTreeMap, path::PathBuf};

    const SIGNATURE: &str =
        "render_error: The current operation requires an active path @ pdf-canvas::PathRequired";

    fn corpus_run() -> CorpusRun {
        let page = IndexPage {
            page: 0,
            status: Status::RenderError,
            mismatch: None,
            signature: Some(SIGNATURE.to_owned()),
            delta: Delta::New,
            baseline: None,
            files: vec!["p0-ref.png".to_owned(), "p0-content.txt".to_owned()],
            reproduce: "cargo conformance run --corpus pdfjs --case clippath --page 0".to_owned(),
            render: None,
        };
        let case = IndexCase {
            id: "clippath".to_owned(),
            dir: "clippath-1234abcd".to_owned(),
            status: Status::RenderError,
            signature: Some(SIGNATURE.to_owned()),
            delta: Delta::New,
            baseline: None,
            note: Some("pdf.js eq test".to_owned()),
            likely_crates: Vec::new(),
            reproduce: "cargo conformance run --corpus pdfjs --case clippath".to_owned(),
            bundle: true,
            pages: vec![page],
        };
        let error = ErrorDetail {
            message: "PDF canvas error: The current operation requires an active path".to_owned(),
            chain: vec![
                "The current operation requires an active path".to_owned(),
                "The current operation requires an active path".to_owned(),
            ],
            backtrace: None,
        };
        let result = CaseResult {
            case: Case {
                id: "clippath".to_owned(),
                path: PathBuf::from("/corpus/test/pdfs/clippath.pdf"),
                password: None,
                first_page: None,
                last_page: None,
                expected_md5: None,
                note: Some("pdf.js eq test".to_owned()),
            },
            status: Status::RenderError,
            sha256: None,
            md5_mismatch: false,
            read: None,
            read_process: None,
            signature: Some(SIGNATURE.to_owned()),
            pages: vec![PageResult {
                status: Status::RenderError,
                output: Some(PageOutput {
                    safe_error: Some(error),
                    ..PageOutput::default()
                }),
                page: 0,
                process: ProcessEvidence::default(),
                signature: Some(SIGNATURE.to_owned()),
            }],
            dir: "clippath-1234abcd".to_owned(),
        };
        CorpusRun {
            kind: CorpusKind::Pdfjs,
            index: Index {
                corpus: "pdfjs".to_owned(),
                corpus_root: "/corpus".to_owned(),
                corpus_revision: Some("abc123".to_owned()),
                pdfium: "PDFium libpdfium.so".to_owned(),
                scale: 1.5,
                tolerance: 0.002,
                filtered: false,
                read_only: false,
                totals: BTreeMap::new(),
                page_totals: BTreeMap::new(),
                clusters: vec![Cluster {
                    signature: SIGNATURE.to_owned(),
                    count: 1,
                    cases: vec!["clippath".to_owned()],
                    likely_crates: Vec::new(),
                }],
                cases: vec![case],
            },
            results: BTreeMap::from([("clippath".to_owned(), result)]),
        }
    }

    #[test]
    fn renders_a_self_contained_body() {
        let runs = [corpus_run()];
        let groups = issues::groups(&runs);
        let group = groups.first().unwrap();
        let context = BodyContext {
            repo: "Velli20/safe-pdf".to_owned(),
            run_link: Some("https://github.com/Velli20/safe-pdf/actions/runs/1".to_owned()),
            sha: Some("0123456789abcdef".to_owned()),
        };
        let state = IssueState {
            key: group.key.clone(),
            corpora: group.corpus_states(),
        };
        let body = render(group, &state, &context).unwrap();
        if std::env::var_os("SHOW_BODY").is_some() {
            println!("{}\n\n{body}", title(group));
        }
        assert_eq!(
            title(group),
            "[conformance] Render error: The current operation requires an active path"
        );
        assert_eq!(
            labels(group),
            ["conformance", "conformance:pdfjs", "area:pdf-canvas"]
        );
        assert_eq!(IssueState::parse(&body), Some(state));
        assert!(body.contains("> [!WARNING]\n> Safe-PDF fails to render 1 page in 1 document."));
        assert!(body.contains(
            "<img src=\"https://velli20.github.io/safe-pdf/conformance/pdfjs/cases/clippath-1234abcd/p0-ref.png\""
        ));
        assert!(body.contains(
            "[PDF](https://github.com/mozilla/pdf.js/blob/abc123/test/pdfs/clippath.pdf)"
        ));
        assert_eq!(body.matches("requires an active path\n").count(), 1);
        assert!(body.contains(&format!("cargo conformance verify {}", group.key)));
    }

    #[test]
    fn describes_mismatches_in_words() {
        assert_eq!(
            describe_mismatch("extra_ink / fill (tiling pattern)"),
            "extra ink in fill (tiling pattern)"
        );
        assert_eq!(describe_mismatch("blank page"), "blank page");
    }

    #[test]
    fn finds_the_origin_crate() {
        assert_eq!(
            origin_crate("render_error: no path @ pdf-canvas::PathRequired"),
            Some("pdf-canvas")
        );
        assert_eq!(
            origin_crate("render_error: x @ <pdf_renderer::PdfRenderer>::render"),
            None
        );
        assert_eq!(origin_crate("mismatch: blank page"), None);
    }

    #[test]
    fn links_into_pages() {
        let base = pages_url("Velli20/safe-pdf", CorpusKind::Pdfjs);
        assert_eq!(
            base,
            "https://velli20.github.io/safe-pdf/conformance/pdfjs/"
        );
        assert_eq!(
            viewer_link(&base, "issue 1#2", Some(3)),
            "https://velli20.github.io/safe-pdf/conformance/pdfjs/#case=issue%201%232&page=3"
        );
    }

    #[test]
    fn marks_hex_colors() {
        assert_eq!(
            hex_code("#F280FF alpha 0.50, Winding"),
            "`#F280FF` alpha 0.50, Winding"
        );
        assert_eq!(hex_code("#F280FF, Winding"), "`#F280FF`, Winding");
        assert_eq!(hex_code("axial shading, EvenOdd"), "axial shading, EvenOdd");
    }
}
