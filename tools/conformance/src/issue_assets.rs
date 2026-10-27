//! Keeps the files an issue body shows as assets of one pre-release, so its images and
//! evidence outlive the published report and the run artifacts.
//!
//! Each asset is named `<issue key>--<content hash>--<file>`. An unchanged file keeps its
//! name and is not uploaded again; a changed one gets a new name, so GitHub's image cache never
//! shows an outdated image. Assets of an issue's earlier files are deleted once it has new
//! ones, and all of them when the issue is closed.

use crate::{corpus, github};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// Tag of the pre-release holding the assets.
pub const TAG: &str = "conformance-assets";
/// Hex digits of the content hash in an asset name.
const HASH_DIGITS: usize = 12;

/// Download links of an issue's files, by file name.
pub type AssetLinks = BTreeMap<String, String>;

/// Returns the download link of an asset of the pre-release.
pub fn asset_url(repo: &str, name: &str) -> String {
    format!("https://github.com/{repo}/releases/download/{TAG}/{name}")
}

/// The pre-release's assets in one repository.
pub struct AssetStore {
    repo: String,
    /// Names of the assets the pre-release holds.
    names: BTreeSet<String>,
}

impl AssetStore {
    /// Lists the pre-release's assets, creating the pre-release when it does not exist yet.
    pub fn open(repo: &str) -> Result<Self> {
        let release = format!("repos/{repo}/releases/tags/{TAG}");
        let id = match github::gh_idempotent(&["api", &release, "--jq", ".id"]) {
            Ok(id) => id,
            Err(_) => {
                github::gh(&[
                    "release",
                    "create",
                    TAG,
                    "--repo",
                    repo,
                    "--prerelease",
                    "--title",
                    "Conformance issue assets",
                    "--notes",
                    "Images and evidence shown in conformance issues. Managed by the conformance \
                     workflow; assets are removed when their issue closes.",
                ])
                .context("creating the conformance assets pre-release")?;
                github::gh_idempotent(&["api", &release, "--jq", ".id"])?
            }
        };
        let names = github::gh_idempotent(&[
            "api",
            "--paginate",
            &format!("repos/{repo}/releases/{}/assets", id.trim()),
            "--jq",
            ".[].name",
        ])?;
        Ok(Self {
            repo: repo.to_owned(),
            names: names.lines().map(str::to_owned).collect(),
        })
    }

    /// Uploads the files of an issue that the pre-release does not hold yet, deletes the
    /// issue's assets of earlier files, and returns the links of `files`.
    pub fn upload(&mut self, key: &str, dir: &Path, files: &[String]) -> Result<AssetLinks> {
        let staging = corpus::state_dir().join("issue-assets");
        fs::create_dir_all(&staging)?;
        let mut links = AssetLinks::new();
        let mut current = BTreeSet::new();
        for file in files {
            let source = dir.join(file);
            let bytes =
                fs::read(&source).with_context(|| format!("reading {}", source.display()))?;
            let digest = format!("{:x}", Sha256::digest(&bytes));
            let name = format!(
                "{key}--{}--{file}",
                digest.get(..HASH_DIGITS).unwrap_or(&digest)
            );
            if !self.names.contains(&name) {
                let staged = staging.join(&name);
                fs::write(&staged, &bytes)?;
                github::gh_idempotent(&[
                    "release",
                    "upload",
                    TAG,
                    &staged.display().to_string(),
                    "--repo",
                    &self.repo,
                    "--clobber",
                ])?;
                self.names.insert(name.clone());
            }
            links.insert(file.clone(), asset_url(&self.repo, &name));
            current.insert(name);
        }
        self.delete(key, &current)?;
        Ok(links)
    }

    /// Deletes every asset of an issue.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        self.delete(key, &BTreeSet::new())
    }

    /// Deletes the issue's assets that are not in `keep`.
    fn delete(&mut self, key: &str, keep: &BTreeSet<String>) -> Result<()> {
        let prefix = format!("{key}--");
        let stale: Vec<String> = self
            .names
            .iter()
            .filter(|name| name.starts_with(&prefix) && !keep.contains(*name))
            .cloned()
            .collect();
        for name in stale {
            github::gh_idempotent(&[
                "release",
                "delete-asset",
                TAG,
                &name,
                "--repo",
                &self.repo,
                "--yes",
            ])?;
            self.names.remove(&name);
        }
        Ok(())
    }
}
