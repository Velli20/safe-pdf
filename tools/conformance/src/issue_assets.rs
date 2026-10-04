//! Images embedded in conformance issues, kept on a branch of the repository.
//!
//! GitHub's API cannot attach files to issues, and the Pages report is replaced on every
//! deploy, so issue images are committed to the `conformance-assets` branch instead and
//! linked through `raw.githubusercontent.com`. Files are named by a hash of their content,
//! so a link keeps showing the image it was written with. Every publish replaces the branch
//! with one parentless commit holding only the images open issues use, which keeps the
//! branch small: images nothing links to anymore drop out of the repository.

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Branch holding the images.
pub const BRANCH: &str = "conformance-assets";
/// Remote-tracking ref the branch is fetched into.
const TRACKING: &str = "refs/remotes/origin/conformance-assets";
/// Hex digits of the content hash in a file name.
const HASH_LEN: usize = 32;

/// Images staged for issue bodies, by file name on the branch.
pub struct Assets {
    /// `owner/name` of the repository.
    repo: String,
    /// Source file of each staged name.
    staged: BTreeMap<String, PathBuf>,
}

impl Assets {
    /// Creates an empty set for a repository.
    pub fn new(repo: &str) -> Self {
        Self {
            repo: repo.to_owned(),
            staged: BTreeMap::new(),
        }
    }

    /// Stages an image and returns its link, or `None` when the file does not exist.
    pub fn stage(&mut self, path: &Path) -> Result<Option<String>> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
        };
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("png");
        let name = format!("{}.{extension}", digest.get(..HASH_LEN).unwrap_or(&digest));
        let url = url(&self.repo, &name);
        self.staged.insert(name, path.to_owned());
        Ok(Some(url))
    }

    /// Number of staged images.
    pub fn count(&self) -> usize {
        self.staged.len()
    }

    /// Replaces the branch with the staged images plus the images `bodies` link to that the
    /// branch already holds. Does nothing when the branch would not change.
    pub fn publish<'a>(&self, bodies: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let mut keep: BTreeSet<String> = bodies
            .into_iter()
            .flat_map(|body| referenced(&self.repo, body))
            .collect();
        keep.extend(self.staged.keys().cloned());

        let fetched = git(&[
            "fetch",
            "--no-tags",
            "--depth=1",
            "origin",
            &format!("+refs/heads/{BRANCH}:{TRACKING}"),
        ])
        .is_ok();
        let old_tree = if fetched {
            Some(git(&["rev-parse", &format!("{TRACKING}^{{tree}}")])?)
        } else {
            println!("No {BRANCH} branch yet; creating it.");
            None
        };
        let mut blobs: BTreeMap<String, String> = BTreeMap::new();
        if fetched {
            for line in git(&["ls-tree", TRACKING])?.lines() {
                // `<mode> blob <sha>\t<name>`
                if let Some((meta, name)) = line.split_once('\t')
                    && let Some(sha) = meta.split_whitespace().nth(2)
                    && keep.contains(name)
                {
                    blobs.insert(name.to_owned(), sha.to_owned());
                }
            }
        }
        for (name, path) in &self.staged {
            if blobs.contains_key(name) {
                continue;
            }
            let path = path.to_str().ok_or_else(|| anyhow!("non-UTF-8 path"))?;
            blobs.insert(name.clone(), git(&["hash-object", "-w", "--", path])?);
        }
        let listing: String = blobs
            .iter()
            .map(|(name, sha)| format!("100644 blob {sha}\t{name}\n"))
            .collect();
        let tree = git_with_input(&["mktree"], &listing)?;
        if old_tree.as_deref() == Some(tree.as_str()) {
            println!("{BRANCH} is up to date ({} images).", blobs.len());
            return Ok(());
        }
        let commit = git_with_input(
            &["commit-tree", &tree],
            &format!("Images of open conformance issues ({})\n", blobs.len()),
        )?;
        git(&[
            "push",
            "--force",
            "origin",
            &format!("{commit}:refs/heads/{BRANCH}"),
        ])?;
        println!("Published {} images to {BRANCH}.", blobs.len());
        Ok(())
    }

    /// Copies the staged images to `dir` for a dry run.
    pub fn copy_to(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        for (name, path) in &self.staged {
            fs::copy(path, dir.join(name))
                .with_context(|| format!("copying {}", path.display()))?;
        }
        Ok(())
    }
}

/// Returns the link of a file on the branch.
fn url(repo: &str, name: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/{BRANCH}/{name}")
}

/// Returns the names of branch files an issue body links to.
pub fn referenced(repo: &str, body: &str) -> Vec<String> {
    let prefix = url(repo, "");
    body.split(prefix.as_str())
        .skip(1)
        .filter_map(|rest| {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
                .unwrap_or(rest.len());
            rest.get(..end)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
        .collect()
}

/// Runs git with the bot identity and returns its trimmed standard output.
fn git(args: &[&str]) -> Result<String> {
    run_git(args, None)
}

fn git_with_input(args: &[&str], input: &str) -> Result<String> {
    run_git(args, Some(input))
}

fn run_git(args: &[&str], input: Option<&str>) -> Result<String> {
    let mut child = Command::new("git")
        .args(args)
        .env("GIT_AUTHOR_NAME", "github-actions[bot]")
        .env(
            "GIT_AUTHOR_EMAIL",
            "41898282+github-actions[bot]@users.noreply.github.com",
        )
        .env("GIT_COMMITTER_NAME", "github-actions[bot]")
        .env(
            "GIT_COMMITTER_EMAIL",
            "41898282+github-actions[bot]@users.noreply.github.com",
        )
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow!("cannot run git: {error}"))?;
    if let Some(input) = input
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_by_content_and_finds_links_again() {
        let dir = std::env::temp_dir().join(format!("issue-assets-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("p0-ref.png"), dir.join("p1-ref.png"));
        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"same").unwrap();
        let mut assets = Assets::new("owner/repo");
        let link = assets.stage(&a).unwrap().unwrap();
        assert_eq!(assets.stage(&b).unwrap(), Some(link.clone()));
        assert_eq!(assets.count(), 1);
        assert!(
            link.starts_with("https://raw.githubusercontent.com/owner/repo/conformance-assets/")
        );
        assert!(link.ends_with(".png"));
        assert_eq!(assets.stage(&dir.join("missing.png")).unwrap(), None);

        let body = format!("<img src=\"{link}\" width=\"280\"> and <img src=\"{link}\">");
        let names = referenced("owner/repo", &body);
        let name = link.rsplit('/').next().unwrap();
        assert_eq!(names, [name, name]);
        assert!(referenced("other/repo", &body).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
