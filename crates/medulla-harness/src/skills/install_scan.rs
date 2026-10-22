//! Discovery helpers for managed skill files and prune candidates.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::marker::parse_marker;
use super::render::SLUG_PREFIX;
use super::targets::{commands_dir, skills_dir};
use super::{FileAction, FileOutcome, InstallOptions, SkillTarget};

/// A generated file discovered on disk, with what its marker claims.
pub(super) struct ManagedFile {
    pub(super) path: PathBuf,
    pub(super) slug: String,
    pub(super) workflow_id: String,
    pub(super) rev: String,
    pub(super) target: SkillTarget,
    /// `true` for a `SKILL.md`, `false` for a slash-command file.
    pub(super) is_skill: bool,
}

/// A file in a generated file's place, with whatever marker it turned out to
/// carry.
///
/// The marker is `None` for a file we cannot identify — someone else's, an
/// unreadable one, or a `SKILL.md` that is not there at all because only the
/// directory around it survived.
struct Candidate {
    path: PathBuf,
    slug: String,
    marker: Option<(String, String)>,
    target: SkillTarget,
    is_skill: bool,
    /// Whether the directory entry this came from is a symlink, which makes
    /// `path` point outside the root we are allowed to delete under. Always
    /// `false` for a command file: removing that path unlinks the symlink
    /// itself, never what it points at.
    symlinked: bool,
}

impl Candidate {
    /// The [`ManagedFile`] a removal needs, for a candidate we have decided is
    /// ours to retire.
    ///
    /// An unmarked candidate has no workflow to name, so the report attributes
    /// it to its slug — the only identity it has left.
    pub(super) fn into_managed(self) -> ManagedFile {
        let (workflow_id, rev) = self
            .marker
            .unwrap_or_else(|| (self.slug.clone(), String::new()));
        ManagedFile {
            path: self.path,
            slug: self.slug,
            workflow_id,
            rev,
            target: self.target,
            is_skill: self.is_skill,
        }
    }
}

/// Every marked file under one target's skill and command directories.
///
/// Missing directories yield nothing rather than an error: not having a
/// `.codex` is the normal state for most machines.
pub(super) fn scan_managed(target: SkillTarget, root: &Path) -> io::Result<Vec<ManagedFile>> {
    Ok(scan_candidates(target, root)?
        .into_iter()
        .filter(|candidate| candidate.marker.is_some())
        .map(Candidate::into_managed)
        .collect())
}

/// The subset of `candidates` a prune pass should remove.
///
/// Two rules, in order. A file whose marker names a workflow goes when that
/// workflow is not kept — the long-standing behaviour. A file with no marker we
/// can read goes when its slug is in the `medulla-` namespace and no kept
/// workflow claims that slug; that is what retires a leftover from a release
/// whose marker we no longer recognise, or a directory whose `SKILL.md` an
/// operator deleted by hand.
///
/// The prefix rule stops at a symlinked directory. Its `SKILL.md` is content the
/// operator linked in from somewhere outside the root, and the prefix alone is
/// far too thin a claim to delete a file on the strength of. The marker rule is
/// unaffected: a marker we wrote identifies the file no matter where it sits.
pub(super) fn prunable(
    candidates: Vec<Candidate>,
    keep_ids: &BTreeSet<&str>,
    keep_slugs: &BTreeSet<String>,
) -> Vec<ManagedFile> {
    candidates
        .into_iter()
        .filter(|candidate| match &candidate.marker {
            Some((workflow_id, _)) => !keep_ids.contains(workflow_id.as_str()),
            None => {
                !candidate.symlinked
                    && candidate.slug.starts_with(SLUG_PREFIX)
                    && !keep_slugs.contains(&candidate.slug)
            }
        })
        .map(Candidate::into_managed)
        .collect()
}

/// Every file in a generated file's place under one target's skill and command
/// directories, marked or not, sorted by path.
///
/// Covers only the directories the target currently writes. Codex's deprecated
/// `.codex/skills` is reached through [`scan_skill_dirs`] by the prune pass
/// alone — the listing paths report what a harness will actually load, and
/// [`install`] cleans that root only on behalf of a workflow that still exists.
pub(super) fn scan_candidates(target: SkillTarget, root: &Path) -> io::Result<Vec<Candidate>> {
    let mut found = scan_skill_dirs(target, &skills_dir(target, root))?;

    if let Some(dir) = commands_dir(target, root) {
        for entry in read_dir_opt(&dir)? {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let slug = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            found.push(Candidate {
                marker: read_marker(&path)?,
                path,
                slug,
                target,
                is_skill: false,
                symlinked: false,
            });
        }
    }

    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Every entry of one skills root that sits where a generated skill directory
/// would, marked or not.
///
/// A missing directory yields nothing rather than an error: not having a
/// `.codex` is the normal state for most machines. Split out from
/// [`scan_candidates`] so the prune pass can point it at Codex's deprecated
/// `.codex/skills` root, which has no commands directory beside it.
pub(super) fn scan_skill_dirs(target: SkillTarget, skills: &Path) -> io::Result<Vec<Candidate>> {
    let mut found = Vec::new();
    for entry in read_dir_opt(skills)? {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        // An entry whose type will not read is treated as a symlink: that is the
        // conservative direction when guessing wrong means deleting through one.
        let symlinked = entry
            .file_type()
            .map(|kind| kind.is_symlink())
            .unwrap_or(true);
        let path = dir.join("SKILL.md");
        found.push(Candidate {
            marker: read_marker(&path)?,
            path,
            slug: entry.file_name().to_string_lossy().into_owned(),
            target,
            is_skill: true,
            symlinked,
        });
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// What is already at a path we are about to consider.
pub(super) enum Existing {
    /// Nothing is there.
    Absent,
    /// Something is there that is not UTF-8 text, so it cannot be ours.
    Foreign,
    /// Readable text, marker or not.
    Text(String),
}

/// Reads a candidate file, treating undecodable bytes as someone else's.
///
/// Reading bytes rather than text is load-bearing: a single non-UTF-8 file
/// anywhere in `~/.claude/commands` would otherwise fail the whole scan with an
/// `InvalidData` error and abort a sync, uninstall, or listing that has nothing
/// to do with that file.
pub(super) fn read_existing(path: &Path) -> io::Result<Existing> {
    match fs::read(path) {
        Ok(bytes) => Ok(match String::from_utf8(bytes) {
            Ok(text) => Existing::Text(text),
            Err(_) => Existing::Foreign,
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Existing::Absent),
        Err(err) => Err(err),
    }
}

/// The marker of the file at `path`, or `None` when it is absent or not ours.
pub(super) fn read_marker(path: &Path) -> io::Result<Option<(String, String)>> {
    Ok(match read_existing(path)? {
        Existing::Text(text) => parse_marker(&text),
        Existing::Absent | Existing::Foreign => None,
    })
}

/// Directory entries, treating a missing directory as empty.
fn read_dir_opt(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    entries.collect::<io::Result<Vec<_>>>()
}
