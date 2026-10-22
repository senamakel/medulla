//! Writing, refreshing, and removing generated skills — the managed-file
//! discipline that makes it safe to touch `~/.claude`.
//!
//! One rule governs every path in here: a file we did not write is never
//! overwritten. Ownership is proved by the marker line [`super::render`] puts
//! at the top of everything it generates, so a hand-written
//! `~/.claude/skills/medulla-babysit/SKILL.md` survives an install intact and
//! is reported as a collision instead.
//!
//! Removal carries one deliberate exception, and only under
//! [`sync`]`(.., prune = true)`: the `medulla-` slug prefix is Medulla's
//! namespace, so a `medulla-*` skill directory with no enabled workflow behind
//! it is retired even when its marker is missing or unreadable. Without that,
//! a leftover written by a release whose marker we can no longer parse — or
//! one an operator's editor mangled — is undeletable by any command, and the
//! harness goes on advertising a workflow that does not exist. A `medulla-*`
//! directory that *does* match an enabled workflow keeps the marker rule
//! intact: unmanaged content there is a collision, never a removal.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::marker::parse_marker;
use super::render::{render, render_command, slug_for, SLUG_PREFIX};
use super::targets::{
    command_path, commands_dir, dedupe_by_skills_dir, legacy_codex_skills_dir, skill_path,
    skills_dir, target_root,
};
use super::{
    FileAction, FileOutcome, InstallOptions, InstallReport, InstalledSkill, SkillTarget,
    SkillWorkflow,
};

/// Installs the given workflows into every configured target.
///
/// Disabled workflows are skipped entirely — a workflow that may not run should
/// not be advertised to a harness as runnable. Command files are written only
/// when [`InstallOptions::with_commands`] is set and the target has a command
/// layout.
///
/// # Errors
///
/// Propagates filesystem errors from creating directories or writing files.
/// A collision is *not* an error: it lands in the report as
/// [`FileAction::SkippedUnmanaged`] or [`FileAction::SlugCollision`] so one bad
/// path does not abort the rest.
pub fn install(workflows: &[SkillWorkflow], opts: &InstallOptions) -> io::Result<InstallReport> {
    install_over(workflows, opts, &BTreeSet::new())
}

/// [`install`], told which paths a prune in the same pass has just emptied.
///
/// Only a dry run needs telling: a real prune has already deleted the file, so
/// the write sees an absent path by itself. Under `dry_run` nothing was removed
/// and the stale marker is still on disk, and reading it would report a
/// [`FileAction::SlugCollision`] for a write the real run performs — the one
/// thing a dry run must never do is disagree with the run it predicts.
fn install_over(
    workflows: &[SkillWorkflow],
    opts: &InstallOptions,
    cleared: &BTreeSet<PathBuf>,
) -> io::Result<InstallReport> {
    let mut report = InstallReport::default();
    // Which workflow has already claimed each path in *this* run. Without it a
    // dry run would report two `Created`s where the real run writes one file
    // and collides on the second, and the two must agree.
    let mut claimed: HashMap<PathBuf, String> = HashMap::new();
    for target in &dedupe_by_skills_dir(&opts.targets, opts.scope, &opts.root) {
        let root = target_root(*target, opts.scope, &opts.root);
        for summary in workflows.iter().filter(|summary| summary.enabled) {
            let skill = render(summary);
            let path = skill_path(*target, &root, &skill.slug);
            report.files.push(write_managed(
                &path,
                &skill.body,
                *target,
                &summary.id,
                opts,
                &mut claimed,
                cleared,
            )?);

            // Codex reads its deprecated `.codex/skills` root as well as the
            // `.agents/skills` one we now write, and a skill discovered under
            // two names is worse than one discovered under none: Codex drops a
            // `$slug` mention it cannot resolve to exactly one skill, silently.
            // So an install also retires what an earlier version of this
            // command left behind. Marker-gated like every other removal, so a
            // file the operator wrote there themselves stays.
            if *target == SkillTarget::Codex {
                if let Some(outcome) = retire_legacy_codex(&root, &skill.slug, &summary.id, opts)? {
                    report.files.push(outcome);
                }
            }

            if opts.with_commands {
                if let Some(path) = command_path(*target, &root, &skill.slug) {
                    let body = render_command(&skill, summary);
                    report.files.push(write_managed(
                        &path,
                        &body,
                        *target,
                        &summary.id,
                        opts,
                        &mut claimed,
                        cleared,
                    )?);
                }
            }
        }
    }
    Ok(report)
}

/// Removes managed files for anything outside `workflows` when `prune` is set,
/// then installs `workflows`.
///
/// The pruned set is "every generated file under the target directories whose
/// workflow is not in `workflows`" — which covers a deleted workflow, a renamed
/// one, and a workflow that has since been disabled (disabled workflows are not
/// installed, so they are not in the keep set either).
///
/// Membership is decided by marker first and by the `medulla-` slug prefix
/// second: a `medulla-*` skill directory or command file that no enabled
/// workflow claims is removed even when it carries no marker we can read. That
/// prefix is Medulla's namespace, and a leftover we cannot identify is exactly
/// the one an operator cannot get rid of any other way. Neighbours outside the
/// prefix are left alone, as everywhere else, and so is anything a *kept*
/// workflow's slug points at — that stays under the marker rule, where foreign
/// content is a collision rather than a deletion.
///
/// Codex's deprecated `.codex/skills` root is swept too, since
/// [`install`] only retires what a still-installed workflow left there. That
/// sweep keys off Codex being *requested*, not off Codex surviving
/// [`dedupe_by_skills_dir`]: `--harness generic,codex` collapses to Generic
/// alone, because both write `.agents/skills`, and the legacy leftovers would
/// otherwise go unswept purely because of the order the harnesses were named.
///
/// A `medulla-*` entry that is a symlink to a directory is never pruned on the
/// prefix rule. `read_dir` resolves it, so the `SKILL.md` behind it belongs to
/// whatever the operator linked in — outside the root entirely — and deleting
/// it would destroy content Medulla never wrote. A marked file reached that way
/// is still ours to remove: we wrote the marker, so we know what it is.
///
/// Pruning runs *first* because a rename can hand the new id the old one's
/// path: `deploy.prod` renamed to `deploy-prod` still slugifies to
/// `medulla-deploy-prod`, and installing first would find the old marker
/// sitting there, report a [`FileAction::SlugCollision`], and then delete that
/// file in the prune — leaving the renamed workflow with no skill at all until
/// some later pass. Clearing the stale owner first lets the same pass write the
/// new file. The order is otherwise immaterial: prune only ever touches files
/// whose workflow is absent from `workflows`, which are exactly the ones the
/// install pass does not write.
///
/// # Errors
///
/// Propagates filesystem errors from removal or from the install pass.
pub fn sync(
    workflows: &[SkillWorkflow],
    opts: &InstallOptions,
    prune: bool,
) -> io::Result<InstallReport> {
    let mut report = InstallReport::default();
    let mut cleared: BTreeSet<PathBuf> = BTreeSet::new();
    if prune {
        let enabled = || workflows.iter().filter(|summary| summary.enabled);
        let keep_ids: BTreeSet<&str> = enabled().map(|summary| summary.id.as_str()).collect();
        let keep_slugs: BTreeSet<String> = enabled().map(|summary| slug_for(&summary.id)).collect();

        for target in &dedupe_by_skills_dir(&opts.targets, opts.scope, &opts.root) {
            let root = target_root(*target, opts.scope, &opts.root);
            let candidates = scan_candidates(*target, &root)?;
            for stale in prunable(candidates, &keep_ids, &keep_slugs) {
                cleared.insert(stale.path.clone());
                report.files.push(remove_managed(&stale, opts)?);
            }
        }

        // Deliberately outside the loop above and keyed off the requested
        // targets: the deduped list may have dropped Codex, and `.codex/skills`
        // is a root no other target shares, so it must be swept exactly once
        // either way.
        if opts.targets.contains(&SkillTarget::Codex) {
            let root = target_root(SkillTarget::Codex, opts.scope, &opts.root);
            let candidates = scan_skill_dirs(SkillTarget::Codex, &legacy_codex_skills_dir(&root))?;
            for stale in prunable(candidates, &keep_ids, &keep_slugs) {
                cleared.insert(stale.path.clone());
                report.files.push(remove_managed(&stale, opts)?);
            }
        }
    }

    report
        .files
        .extend(install_over(workflows, opts, &cleared)?.files);
    Ok(report)
}

/// Removes the managed skills (and their commands) for the named workflow ids.
///
/// Ids that are not installed are simply absent from the report; asking to
/// uninstall something twice is not an error.
///
/// # Errors
///
/// Propagates filesystem errors from reading the target directories or
/// removing files.
pub fn uninstall(ids: &[String], opts: &InstallOptions) -> io::Result<InstallReport> {
    let wanted: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let mut report = InstallReport::default();
    for target in &dedupe_by_skills_dir(&opts.targets, opts.scope, &opts.root) {
        for managed in scan_managed(*target, &target_root(*target, opts.scope, &opts.root))? {
            if !wanted.contains(managed.workflow_id.as_str()) {
                continue;
            }
            report.files.push(remove_managed(&managed, opts)?);
        }
    }
    Ok(report)
}

/// Every managed skill currently on disk under the configured targets.
///
/// Reads markers rather than filenames, so a skill installed by an older
/// release under a different slug is still recognised as ours.
///
/// # Errors
///
/// Propagates filesystem errors other than a missing target directory, which
/// simply contributes nothing.
pub fn installed(opts: &InstallOptions) -> io::Result<Vec<InstalledSkill>> {
    let mut found = Vec::new();
    for target in &dedupe_by_skills_dir(&opts.targets, opts.scope, &opts.root) {
        for managed in scan_managed(*target, &target_root(*target, opts.scope, &opts.root))? {
            if !managed.is_skill {
                continue;
            }
            found.push(InstalledSkill {
                workflow_id: managed.workflow_id,
                slug: managed.slug,
                target: *target,
                path: managed.path,
                rev: managed.rev,
            });
        }
    }
    Ok(found)
}

/// Removes a skill an earlier release wrote to Codex's deprecated
/// `.codex/skills` root, so the same workflow is not discovered twice.
///
/// Marker-gated, and gated on the marker naming *this* workflow: a file the
/// operator wrote there themselves, or one belonging to another workflow, is
/// left exactly as it is. Returns `None` when there is nothing of ours to
/// retire, which is the normal case on every install after the first.
///
/// Why it matters: Codex still scans both roots, dedupes by canonical path
/// rather than by name, and then silently drops a `$slug` mention that resolves
/// to more than one skill. The duplicate would not error — it would quietly
/// stop the mention working.
fn retire_legacy_codex(
    root: &Path,
    slug: &str,
    workflow_id: &str,
    opts: &InstallOptions,
) -> io::Result<Option<FileOutcome>> {
    let path = legacy_codex_skills_dir(root).join(slug).join("SKILL.md");
    let Some((marked_id, rev)) = read_marker(&path)? else {
        return Ok(None);
    };
    if marked_id != workflow_id {
        return Ok(None);
    }
    remove_managed(
        &ManagedFile {
            path,
            slug: slug.to_string(),
            workflow_id: marked_id,
            rev,
            target: SkillTarget::Codex,
            is_skill: true,
        },
        opts,
    )
    .map(Some)
}

/// Writes `body` to `path` unless something else is already there.
///
/// The five outcomes are the whole contract: absent → written, ours and
/// identical → untouched, ours and stale → rewritten, someone else's → left
/// alone as [`FileAction::SkippedUnmanaged`], another workflow's → left alone
/// as [`FileAction::SlugCollision`]. `claimed` carries the paths earlier
/// workflows in this same run took, so the second of two workflows that
/// slugify alike collides instead of overwriting.
///
/// Under `dry_run` the identical decision is made and reported, and nothing is
/// written — including the parent directories, so a dry run leaves no trace.
/// `cleared` names the paths this pass's prune emptied, which a dry run must
/// treat as absent even though the file is still there.
#[allow(clippy::too_many_arguments)]
fn write_managed(
    path: &Path,
    body: &str,
    target: SkillTarget,
    workflow_id: &str,
    opts: &InstallOptions,
    claimed: &mut HashMap<PathBuf, String>,
    cleared: &BTreeSet<PathBuf>,
) -> io::Result<FileOutcome> {
    let outcome = |action| FileOutcome {
        path: path.to_path_buf(),
        target,
        workflow_id: workflow_id.to_string(),
        action,
    };

    if claimed.get(path).is_some_and(|owner| owner != workflow_id) {
        return Ok(outcome(FileAction::SlugCollision));
    }

    let existing = if cleared.contains(path) {
        Existing::Absent
    } else {
        read_existing(path)?
    };

    let action = match existing {
        Existing::Absent => FileAction::Created,
        // Bytes we cannot even read as text are certainly not a marker of ours.
        Existing::Foreign => FileAction::SkippedUnmanaged,
        Existing::Text(text) => match parse_marker(&text) {
            None => FileAction::SkippedUnmanaged,
            // Ours, but generated for another workflow: the path is spoken for
            // and saying "unmanaged" would blame a third party for our file.
            Some((marked_id, _)) if marked_id != workflow_id => FileAction::SlugCollision,
            Some((_, rev)) if marker_rev_of(body).as_deref() == Some(rev.as_str()) => {
                FileAction::Unchanged
            }
            Some(_) => FileAction::Updated,
        },
    };

    if matches!(
        action,
        FileAction::Created | FileAction::Updated | FileAction::Unchanged
    ) {
        claimed.insert(path.to_path_buf(), workflow_id.to_string());
    }

    if matches!(action, FileAction::Created | FileAction::Updated) && !opts.dry_run {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomically(path, body)?;
    }
    Ok(outcome(action))
}

/// Replaces `path` with `body` in one step, as far as any reader is concerned.
///
/// A plain `fs::write` truncates and then fills, so a harness that opens the
/// file in that window reads a partial skill — and this is the pass that runs
/// while sessions are starting, so the window is exactly when readers show up.
/// Writing a sibling temp file and renaming it over the target closes that: a
/// reader sees either the whole previous file or the whole new one, never a
/// prefix of either. The temp file is a sibling so the rename stays within one
/// filesystem, where it is atomic.
///
/// The name carries a unique token so two writers of the same skill cannot
/// clobber each other's half-written temp file. A process id alone would not
/// do it: two threads of one process share it. They cannot normally race at
/// all — [`RefreshLock`](super::refresh::RefreshLock) serializes the managed
/// root — but `install` on the user scope takes no lock, and a leftover temp
/// file from a killed process must not be adopted as the next writer's buffer.
///
/// A failed rename removes the temp file rather than leaving it behind, and
/// the target is untouched — the reader still sees the whole previous file,
/// and no uniquely named orphan accumulates for a later writer to trip over.
fn write_atomically(path: &Path, body: &str) -> io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    // A partial write (quota exhausted, I/O error) must not leave its uniquely
    // named temp file behind either: each retry mints a fresh name, so a run of
    // failures under a full quota would otherwise accumulate orphans.
    if let Err(source) = fs::write(&temp, body) {
        let _ = fs::remove_file(&temp);
        return Err(source);
    }
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(source) => {
            let _ = fs::remove_file(&temp);
            Err(source)
        }
    }
}

/// The `rev` of freshly rendered content, read back off its own marker.
fn marker_rev_of(body: &str) -> Option<String> {
    parse_marker(body).map(|(_, rev)| rev)
}

/// Deletes one managed file, and its now-empty skill directory.
///
/// Leaving `medulla-babysit/` behind as an empty directory would make an
/// uninstalled skill look installed to anyone reading the tree.
fn remove_managed(managed: &ManagedFile, opts: &InstallOptions) -> io::Result<FileOutcome> {
    if !opts.dry_run {
        match fs::remove_file(&managed.path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        if managed.is_skill {
            if let Some(parent) = managed.path.parent() {
                // Best effort: a directory the operator put other files in
                // stays, and that is the desired outcome.
                let _ = fs::remove_dir(parent);
            }
        }
    }
    Ok(FileOutcome {
        path: managed.path.clone(),
        target: managed.target,
        workflow_id: managed.workflow_id.clone(),
        action: FileAction::Removed,
    })
}

