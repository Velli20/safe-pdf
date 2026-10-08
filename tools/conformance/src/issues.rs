//! Keeps one GitHub issue per failure cause, across corpora.
//!
//! Failures are grouped by signature over every corpus run passed in, and each group is
//! matched to an issue through the key in the issue's hidden state:
//! - no issue: create one (up to `max_new` per run, regressions and crashes first; no limit
//!   while the repository has no conformance issue yet);
//! - open issue: refresh it silently, commenting only when new documents join the cluster
//!   or it shrinks by a quarter or more;
//! - closed as completed: reopen, since the failure is back;
//! - closed as not planned: leave it alone.
//!
//! Open issues whose cluster is absent from two full runs in a row are closed as completed.
//! Read-only runs file and refresh issues but never count a cluster as absent.
//!
//! Before any issue is written, the images the bodies embed are published to the
//! `conformance-assets` branch (see [`crate::issue_assets`]), so no body links to a missing image.

use crate::{
    baseline::Delta,
    corpus::{self, CorpusKind},
    github::{self, Issue},
    issue_assets::Assets,
    issue_body::{self, BodyContext},
    issue_state::{self, CorpusState, IssueState},
    model::{CaseResult, Status},
    report_index::{Cluster, Index},
};
use anyhow::{Context, Result, bail};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    thread,
    time::Duration,
};

const LABEL: &str = "conformance";
/// Relative shrink of a cluster that is worth a comment.
const SHRINK_NOTICE: f64 = 0.25;
/// Consecutive full runs without a cluster before its issue is closed.
const CLOSE_AFTER_MISSING: u32 = 2;
/// Pause after creating or rewriting an issue. GitHub's secondary rate limit allows about
/// 80 content-creating requests a minute, which a first run filing every cluster or a
/// layout change rewriting every issue would otherwise exceed.
const WRITE_PAUSE: Duration = Duration::from_secs(1);

/// Settings of an issue run.
pub struct IssueOptions<'a> {
    /// Corpora whose last runs are reported together.
    pub kinds: &'a [CorpusKind],
    /// `owner/name` of the repository.
    pub repo: &'a str,
    /// Issues created at most per run; 0 for no limit. Not applied while the repository has
    /// no conformance issue yet, so the first run files every cluster.
    pub max_new: usize,
    /// Print planned actions and write bodies to disk without calling GitHub.
    pub dry_run: bool,
}

/// The last run of one corpus.
pub struct CorpusRun {
    /// Corpus.
    pub kind: CorpusKind,
    /// Run index.
    pub index: Index,
    /// Full case results by id; empty when `results.json` is not available.
    pub results: BTreeMap<String, CaseResult>,
}

impl CorpusRun {
    /// Loads the last run of a corpus from its output directory.
    pub fn load(kind: CorpusKind, with_results: bool) -> Result<Self> {
        let out = corpus::output_dir(kind);
        let index: Index = serde_json::from_slice(
            &fs::read(out.join("index.json"))
                .with_context(|| format!("no index.json for {}; run it first", kind.as_str()))?,
        )?;
        let results = match fs::read(out.join("results.json")) {
            Ok(bytes) if with_results => serde_json::from_slice::<Vec<CaseResult>>(&bytes)?
                .into_iter()
                .map(|result| (result.case.id.clone(), result))
                .collect(),
            _ => BTreeMap::new(),
        };
        Ok(Self {
            kind,
            index,
            results,
        })
    }
}

/// One corpus's share of a group.
pub struct Part<'a> {
    /// The corpus run.
    pub run: &'a CorpusRun,
    /// The run's cluster with the group's signature.
    pub cluster: &'a Cluster,
}

/// Failures sharing a signature across corpora; one issue each.
pub struct Group<'a> {
    /// Shared signature.
    pub signature: String,
    /// Issue key.
    pub key: String,
    /// Clusters per corpus, in the order the corpora were given.
    pub parts: Vec<Part<'a>>,
}

impl Group<'_> {
    /// Returns the status named by the signature.
    pub fn status(&self) -> Status {
        [
            Status::Mismatch,
            Status::RenderError,
            Status::ReadError,
            Status::Crash,
            Status::Timeout,
        ]
        .into_iter()
        .find(|status| {
            self.signature
                .strip_prefix(status.as_str())
                .is_some_and(|rest| rest.starts_with(':'))
        })
        .unwrap_or(Status::Mismatch)
    }

    /// Failing pages (or documents without pages) across corpora.
    pub fn failures(&self) -> usize {
        self.parts
            .iter()
            .fold(0, |sum, part| sum.saturating_add(part.cluster.count))
    }

    /// Documents across corpora.
    pub fn documents(&self) -> usize {
        self.parts
            .iter()
            .fold(0, |sum, part| sum.saturating_add(part.cluster.cases.len()))
    }

    /// Returns the deltas of every failure in the group against the baseline.
    pub fn deltas(&self) -> Vec<Delta> {
        let mut deltas = Vec::new();
        for part in &self.parts {
            for case in part
                .run
                .index
                .cases
                .iter()
                .filter(|case| part.cluster.cases.contains(&case.id))
            {
                if case.pages.is_empty() && case.signature.as_ref() == Some(&self.signature) {
                    deltas.push(case.delta);
                }
                deltas.extend(
                    case.pages
                        .iter()
                        .filter(|page| page.signature.as_ref() == Some(&self.signature))
                        .map(|page| page.delta),
                );
            }
        }
        deltas
    }

    /// Returns true when a failure is worse than the recorded baseline.
    pub fn regressed(&self) -> bool {
        self.deltas().contains(&Delta::Regressed)
    }

    /// Returns the issue state describing this run's clusters.
    pub fn corpus_states(&self) -> BTreeMap<String, CorpusState> {
        self.parts
            .iter()
            .map(|part| {
                (
                    part.run.kind.as_str().to_owned(),
                    CorpusState {
                        failures: part.cluster.count,
                        documents: part.cluster.cases.len(),
                        missing: 0,
                    },
                )
            })
            .collect()
    }
}

/// Groups the clusters of several corpus runs by key: worst status first, regressions
/// first within a status, then largest.
pub fn groups(runs: &[CorpusRun]) -> Vec<Group<'_>> {
    let mut groups: BTreeMap<String, Group<'_>> = BTreeMap::new();
    for run in runs {
        for cluster in &run.index.clusters {
            let key = issue_state::key(&cluster.signature);
            groups
                .entry(key.clone())
                .or_insert_with(|| Group {
                    signature: cluster.signature.clone(),
                    key,
                    parts: Vec::new(),
                })
                .parts
                .push(Part { run, cluster });
        }
    }
    let mut groups: Vec<Group<'_>> = groups.into_values().collect();
    groups.sort_by_cached_key(|group| {
        (
            std::cmp::Reverse(group.status().severity()),
            !group.regressed(),
            std::cmp::Reverse(group.failures()),
            group.signature.clone(),
        )
    });
    groups
}

/// What happened to issues in one run.
#[derive(Default)]
struct Tally {
    created: usize,
    updated: usize,
    reopened: usize,
    closed: usize,
    deferred: usize,
    failed: usize,
}

/// Files and updates issues for the last runs of the given corpora.
pub fn run(options: &IssueOptions<'_>) -> Result<()> {
    if options.kinds.is_empty() {
        bail!("pass at least one --corpus");
    }
    let runs = options
        .kinds
        .iter()
        .map(|kind| CorpusRun::load(*kind, true))
        .collect::<Result<Vec<_>>>()?;
    let partial = runs.iter().any(|run| run.index.filtered);
    if partial {
        println!(
            "Some runs checked only part of their corpus; no issue is closed or counted as missing."
        );
    }
    let existing = if options.dry_run {
        github::issues(options.repo, LABEL).unwrap_or_else(|error| {
            println!("(dry run) cannot list issues, treating every cluster as new: {error:#}");
            Vec::new()
        })
    } else {
        github::issues(options.repo, LABEL)?
    };
    let existing: Vec<(Issue, IssueState)> = existing
        .into_iter()
        .filter_map(|issue| IssueState::parse(&issue.body).map(|state| (issue, state)))
        .collect();
    let groups = groups(&runs);
    let mut context = BodyContext::from_env(options.repo);
    context.images = publish_images(options, &groups, &existing)?;
    let mut sync = Sync {
        options,
        context,
        existing,
        complete: runs
            .iter()
            .filter(|run| !run.index.filtered && !run.index.read_only)
            .map(|run| run.kind.as_str().to_owned())
            .collect(),
        claimed: BTreeSet::new(),
        labels: BTreeSet::new(),
        tally: Tally::default(),
    };
    for group in &groups {
        if let Err(error) = sync.group(group) {
            sync.tally.failed = sync.tally.failed.saturating_add(1);
            println!("failed for `{}`: {error:#}", group.signature);
        }
    }
    if !partial {
        sync.absent(&runs);
    }
    let tally = &sync.tally;
    println!(
        "{} created, {} updated, {} reopened, {} closed, {} deferred by --max-new, {} failed",
        tally.created, tally.updated, tally.reopened, tally.closed, tally.deferred, tally.failed
    );
    if tally.failed > 0 {
        bail!("{} issue operations failed", tally.failed);
    }
    Ok(())
}

/// Publishes the images the bodies of `groups` embed, keeping those open issues still link
/// to, and returns the link of each local image.
fn publish_images(
    options: &IssueOptions<'_>,
    groups: &[Group<'_>],
    existing: &[(Issue, IssueState)],
) -> Result<BTreeMap<PathBuf, String>> {
    let mut assets = Assets::new(options.repo);
    let mut links = BTreeMap::new();
    for path in groups.iter().flat_map(issue_body::embedded_images) {
        if let Some(link) = assets.stage(&path)? {
            links.insert(path, link);
        }
    }
    if options.dry_run {
        let dir = corpus::state_dir().join("issues").join("assets");
        assets.copy_to(&dir)?;
        println!(
            "(dry run) {} images copied to {} instead of the assets branch",
            assets.count(),
            dir.display()
        );
        return Ok(links);
    }
    let open = existing
        .iter()
        .filter(|(issue, _)| issue.is_open())
        .map(|(issue, _)| issue.body.as_str());
    assets
        .publish(open)
        .context("publishing issue images; no issue was changed")?;
    Ok(links)
}

struct Sync<'a> {
    options: &'a IssueOptions<'a>,
    context: BodyContext,
    existing: Vec<(Issue, IssueState)>,
    /// Corpora with a full run here, whose absent clusters count as missing.
    complete: BTreeSet<String>,
    /// Issue numbers matched to a group in this run.
    claimed: BTreeSet<u64>,
    /// Labels known to exist.
    labels: BTreeSet<String>,
    tally: Tally,
}

impl Sync<'_> {
    fn group(&mut self, group: &Group<'_>) -> Result<()> {
        if let Some((issue, state)) = self
            .existing
            .iter()
            .find(|(_, state)| state.key == group.key)
            .cloned()
        {
            self.claimed.insert(issue.number);
            return self.refresh(group, &issue, &state);
        }
        self.create(group)
    }

    fn create(&mut self, group: &Group<'_>) -> Result<()> {
        // The first run files every cluster; later runs add at most `max_new` at a time.
        let limited = self.options.max_new > 0 && !self.existing.is_empty();
        if limited && self.tally.created >= self.options.max_new {
            self.tally.deferred = self.tally.deferred.saturating_add(1);
            return Ok(());
        }
        self.tally.created = self.tally.created.saturating_add(1);
        let state = IssueState {
            key: group.key.clone(),
            corpora: group.corpus_states(),
            format: issue_state::FORMAT,
        };
        let body = issue_body::render(group, &state, &self.context)?;
        let title = issue_body::title(group);
        let labels = issue_body::labels(group);
        let file = self.body_file(&group.key, &body)?;
        if self.options.dry_run {
            println!(
                "(dry run) create `{title}` [{}] body {} ({} chars)",
                labels.join(", "),
                file.display(),
                body.len()
            );
            return Ok(());
        }
        self.ensure_labels(&labels)?;
        let file = file.display().to_string();
        let mut args = vec![
            "issue",
            "create",
            "--repo",
            self.options.repo,
            "--title",
            &title,
            "--body-file",
            &file,
        ];
        for label in &labels {
            args.extend(["--label", label.as_str()]);
        }
        let url = github::gh(&args)?;
        println!("created {} for `{}`", url.trim(), group.signature);
        thread::sleep(WRITE_PAUSE);
        Ok(())
    }

    fn refresh(&mut self, group: &Group<'_>, issue: &Issue, old: &IssueState) -> Result<()> {
        if issue.is_not_planned() {
            return Ok(());
        }
        let mut state = old.clone();
        // An older body layout is rewritten even when the cluster did not change.
        state.format = issue_state::FORMAT;
        for (corpus, entry) in &mut state.corpora {
            if self.complete.contains(corpus) {
                entry.missing = entry.missing.saturating_add(1);
            }
        }
        state.corpora.extend(group.corpus_states());
        // A corpus that stopped showing the failure is dropped; the others keep it open.
        state
            .corpora
            .retain(|_, entry| entry.missing < CLOSE_AFTER_MISSING);
        if !issue.is_open() {
            self.tally.reopened = self.tally.reopened.saturating_add(1);
            self.write(group, issue, &state)?;
            return self.reopen(issue);
        }
        if state == *old {
            return Ok(());
        }
        self.tally.updated = self.tally.updated.saturating_add(1);
        self.write(group, issue, &state)?;
        if let Some(note) = change_note(old, &state) {
            self.comment(issue, &format!("{note}{}.", self.context.in_run()))?;
        }
        Ok(())
    }

    /// Counts open issues whose cluster is absent from this run, closing those absent from
    /// enough consecutive full runs.
    fn absent(&mut self, runs: &[CorpusRun]) {
        // A read-only run compares no page, so it cannot tell that any failure went away.
        let corpora: BTreeSet<&str> = runs
            .iter()
            .filter(|run| !run.index.read_only)
            .map(|run| run.kind.as_str())
            .collect();
        let absent: Vec<(Issue, IssueState)> = self
            .existing
            .iter()
            .filter(|(issue, state)| {
                issue.is_open()
                    && !self.claimed.contains(&issue.number)
                    && state
                        .corpora
                        .keys()
                        .any(|corpus| corpora.contains(corpus.as_str()))
            })
            .cloned()
            .collect();
        for (issue, old) in absent {
            if let Err(error) = self.absent_issue(&issue, &old, &corpora) {
                self.tally.failed = self.tally.failed.saturating_add(1);
                println!("failed for #{}: {error:#}", issue.number);
            }
        }
    }

    fn absent_issue(
        &mut self,
        issue: &Issue,
        old: &IssueState,
        corpora: &BTreeSet<&str>,
    ) -> Result<()> {
        let mut state = old.clone();
        for (corpus, entry) in &mut state.corpora {
            if corpora.contains(corpus.as_str()) {
                entry.missing = entry.missing.saturating_add(1);
            }
        }
        if state
            .corpora
            .values()
            .all(|entry| entry.missing >= CLOSE_AFTER_MISSING)
        {
            return self.close(
                issue,
                "completed",
                &format!(
                    "No longer seen in {CLOSE_AFTER_MISSING} full runs in a row{}, so it looks fixed. \
                     The workflow reopens this issue if the failure comes back.",
                    self.context.in_run()
                ),
            );
        }
        let body = state.apply(&issue.body)?;
        if self.options.dry_run {
            println!("(dry run) #{}: absent from this run", issue.number);
            return Ok(());
        }
        self.edit_body(issue.number, &body)
    }

    fn write(&mut self, group: &Group<'_>, issue: &Issue, state: &IssueState) -> Result<()> {
        let body = issue_body::render(group, state, &self.context)?;
        let title = issue_body::title(group);
        let labels = issue_body::labels(group);
        let file = self.body_file(&issue.number.to_string(), &body)?;
        if self.options.dry_run {
            println!(
                "(dry run) update #{} as `{title}` [{}] body {}",
                issue.number,
                labels.join(", "),
                file.display()
            );
            return Ok(());
        }
        self.ensure_labels(&labels)?;
        let number = issue.number.to_string();
        let file = file.display().to_string();
        let mut args = vec![
            "issue",
            "edit",
            &number,
            "--repo",
            self.options.repo,
            "--title",
            &title,
            "--body-file",
            &file,
        ];
        for label in &labels {
            args.extend(["--add-label", label.as_str()]);
        }
        github::gh(&args)?;
        println!("updated #{number} for `{}`", group.signature);
        thread::sleep(WRITE_PAUSE);
        Ok(())
    }

    fn edit_body(&self, number: u64, body: &str) -> Result<()> {
        let file = self.body_file(&number.to_string(), body)?;
        let number = number.to_string();
        let file = file.display().to_string();
        github::gh(&[
            "issue",
            "edit",
            &number,
            "--repo",
            self.options.repo,
            "--body-file",
            &file,
        ])?;
        Ok(())
    }

    fn reopen(&self, issue: &Issue) -> Result<()> {
        let note = format!(
            "This failure reappeared{}. Reopening.",
            self.context.in_run()
        );
        if self.options.dry_run {
            println!("(dry run) reopen #{}", issue.number);
            return Ok(());
        }
        let number = issue.number.to_string();
        github::gh(&[
            "issue",
            "reopen",
            &number,
            "--repo",
            self.options.repo,
            "--comment",
            &note,
        ])?;
        github::gh(&[
            "issue",
            "edit",
            &number,
            "--repo",
            self.options.repo,
            "--add-label",
            "regression",
        ])?;
        println!("reopened #{number}");
        Ok(())
    }

    fn close(&mut self, issue: &Issue, reason: &str, note: &str) -> Result<()> {
        self.tally.closed = self.tally.closed.saturating_add(1);
        if self.options.dry_run {
            println!("(dry run) close #{} as {reason}: {note}", issue.number);
            return Ok(());
        }
        let number = issue.number.to_string();
        github::gh(&[
            "issue",
            "close",
            &number,
            "--repo",
            self.options.repo,
            "--reason",
            reason,
            "--comment",
            note,
        ])?;
        println!("closed #{number} as {reason}");
        Ok(())
    }

    fn comment(&self, issue: &Issue, note: &str) -> Result<()> {
        if self.options.dry_run {
            println!("(dry run) comment on #{}: {note}", issue.number);
            return Ok(());
        }
        let number = issue.number.to_string();
        github::gh(&[
            "issue",
            "comment",
            &number,
            "--repo",
            self.options.repo,
            "--body",
            note,
        ])?;
        Ok(())
    }

    /// Writes a body to a file for `--body-file`; bodies exceed argument limits.
    fn body_file(&self, name: &str, body: &str) -> Result<PathBuf> {
        let dir = corpus::state_dir().join("issues");
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{name}.md"));
        fs::write(&path, body)?;
        Ok(path)
    }

    fn ensure_labels(&mut self, labels: &[String]) -> Result<()> {
        for label in labels {
            if self.labels.contains(label) {
                continue;
            }
            let (color, description) = issue_body::label_style(label);
            github::gh(&[
                "label",
                "create",
                label,
                "--repo",
                self.options.repo,
                "--color",
                color,
                "--description",
                &description,
                "--force",
            ])?;
            self.labels.insert(label.clone());
        }
        Ok(())
    }
}

/// Returns a comment for a cluster change people should hear about: new documents in a
/// corpus, or a total shrink of at least a quarter.
fn change_note(old: &IssueState, new: &IssueState) -> Option<String> {
    let grown: Vec<String> = new
        .corpora
        .iter()
        .filter_map(|(corpus, entry)| {
            let before = old.corpora.get(corpus).map_or(0, |entry| entry.documents);
            (entry.documents > before && entry.missing == 0)
                .then(|| format!("{corpus} {} → {} documents", before, entry.documents))
        })
        .collect();
    if !grown.is_empty() {
        return Some(format!("New documents fail this way: {}", grown.join(", ")));
    }
    let (before, after) = (old.failures(), new.failures());
    let shrink = before.checked_sub(after).and_then(|drop| {
        let ratio = f64::from(u32::try_from(drop).ok()?) / f64::from(u32::try_from(before).ok()?);
        (ratio >= SHRINK_NOTICE).then_some(drop)
    })?;
    Some(format!(
        "{shrink} fewer failures ({before} → {after}); a fix may be partial"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: &str = "render_error: Shading type 'LatticeFormTriangleMesh' not implemented @ pdf-canvas::UnsupportedFeature";

    fn corpus_run(kind: CorpusKind) -> CorpusRun {
        CorpusRun {
            kind,
            index: Index {
                corpus: kind.as_str().to_owned(),
                corpus_root: "/corpus".to_owned(),
                corpus_revision: None,
                pdfium: "PDFium".to_owned(),
                scale: 1.5,
                tolerance: 0.002,
                filtered: false,
                read_only: false,
                totals: BTreeMap::new(),
                page_totals: BTreeMap::new(),
                clusters: vec![Cluster {
                    signature: CURRENT.to_owned(),
                    count: 2,
                    cases: vec!["a".to_owned(), "b".to_owned()],
                    likely_crates: Vec::new(),
                }],
                cases: Vec::new(),
            },
            results: BTreeMap::new(),
        }
    }

    fn issue(number: u64, body: String) -> Issue {
        Issue {
            number,
            state: "OPEN".to_owned(),
            state_reason: None,
            body,
        }
    }

    #[test]
    fn files_new_issues_and_counts_absent_ones() {
        let runs = [
            corpus_run(CorpusKind::Pdfium),
            corpus_run(CorpusKind::Pdfjs),
        ];
        let options = IssueOptions {
            kinds: &[CorpusKind::Pdfium, CorpusKind::Pdfjs],
            repo: "owner/repo",
            max_new: 15,
            dry_run: true,
        };
        let gone = state(&[("pdfjs", 2, 1)]);
        let existing = vec![
            // Filed before root-cause keys; it has no state, so it is not matched.
            issue(370, "Conformance key: `conf-1b1118ae25c9`.".to_owned()),
            issue(999, gone.apply("Body").unwrap()),
        ];
        let mut sync = Sync {
            options: &options,
            context: BodyContext {
                repo: "owner/repo".to_owned(),
                run_link: None,
                sha: None,
                images: BTreeMap::new(),
            },
            existing: existing
                .into_iter()
                .filter_map(|issue| IssueState::parse(&issue.body).map(|state| (issue, state)))
                .collect(),
            complete: ["pdfium".to_owned(), "pdfjs".to_owned()].into(),
            claimed: BTreeSet::new(),
            labels: BTreeSet::new(),
            tally: Tally::default(),
        };
        assert_eq!(sync.existing.len(), 1);
        let groups = groups(&runs);
        assert_eq!(groups.len(), 1);
        for group in &groups {
            sync.group(group).unwrap();
        }
        assert_eq!(sync.tally.created, 1);
        sync.absent(&runs);
        // Absent from one full run: counted, not closed yet.
        assert_eq!(sync.tally.closed, 0);
        assert_eq!(sync.tally.failed, 0);
    }

    fn state(corpora: &[(&str, usize, usize)]) -> IssueState {
        IssueState {
            format: issue_state::FORMAT,
            key: "conf2-000000000000".to_owned(),
            corpora: corpora
                .iter()
                .map(|(name, failures, documents)| {
                    (
                        (*name).to_owned(),
                        CorpusState {
                            failures: *failures,
                            documents: *documents,
                            missing: 0,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn small_changes_stay_silent() {
        assert_eq!(
            change_note(
                &state(&[("pdfjs", 220, 180)]),
                &state(&[("pdfjs", 218, 180)])
            ),
            None
        );
        assert_eq!(
            change_note(&state(&[("pdfjs", 4, 2)]), &state(&[("pdfjs", 6, 2)])),
            None
        );
    }

    #[test]
    fn new_documents_and_large_shrinks_are_noted() {
        let grown = change_note(&state(&[("pdfjs", 3, 2)]), &state(&[("pdfjs", 4, 3)]));
        assert!(grown.is_some_and(|note| note.contains("pdfjs 2 → 3")));
        let joined = change_note(
            &state(&[("pdfjs", 3, 2)]),
            &state(&[("pdfjs", 3, 2), ("pdfium", 1, 1)]),
        );
        assert!(joined.is_some_and(|note| note.contains("pdfium 0 → 1")));
        let shrunk = change_note(&state(&[("pdfjs", 8, 4)]), &state(&[("pdfjs", 4, 4)]));
        assert!(shrunk.is_some_and(|note| note.starts_with("4 fewer failures")));
    }
}
