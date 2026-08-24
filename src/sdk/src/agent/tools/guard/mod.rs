//! Containment: every path a tool touches must resolve inside the turn's
//! checkout.
//!
//! # Why resolution happens before the check, not after
//!
//! A turn is given a checkout (`cwd`) and the tools below take relative paths.
//! Checking the *textual* path for `..` is not enough — a symlink inside the
//! checkout pointing at `/etc` is a perfectly ordinary-looking relative path
//! that lands outside it. So the path is canonicalized first and the containment
//! test is applied to the result, which is the only form that reflects where a
//! write would actually go.
//!
//! # A path that does not exist yet still has to be checked
//!
//! `write_file` creates files, so canonicalizing the target itself would fail on
//! exactly the call that matters most. The parent directory is canonicalized
//! instead and the final component appended, which is the deepest form that can
//! be resolved without the file being there.

use std::path::{Component, Path, PathBuf};

/// Why a path was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The path resolved outside the turn's checkout.
    #[error("path escapes the workspace: {0}")]
    Escapes(String),
    /// The path could not be resolved at all.
    #[error("path cannot be resolved: {0}")]
    Unresolvable(String),
}

/// Resolve `candidate` against `root` and confirm it stays inside.
///
/// `candidate` may be relative (resolved against `root`) or absolute (still
/// required to sit inside `root`). Returns the canonical path so the caller
/// operates on the same location the check approved — re-deriving it would
/// reopen the gap the check closed.
///
/// # Errors
///
/// [`PathError::Escapes`] when the resolved path is outside `root`, and
/// [`PathError::Unresolvable`] when neither the path nor its parent exists.
pub fn contained(root: &Path, candidate: &str) -> Result<PathBuf, PathError> {
    let root = root
        .canonicalize()
        .map_err(|e| PathError::Unresolvable(format!("workspace root: {e}")))?;
    let joined = {
        let raw = Path::new(candidate);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            root.join(raw)
        }
    };

    let resolved = match joined.canonicalize() {
        Ok(path) => path,
        // The target does not exist yet — a create. Resolve the deepest thing
        // that does (the parent) and re-attach the final component.
        Err(_) => {
            let parent = joined
                .parent()
                .ok_or_else(|| PathError::Unresolvable(candidate.to_string()))?;
            let name = joined
                .file_name()
                .ok_or_else(|| PathError::Unresolvable(candidate.to_string()))?;
            let parent = parent
                .canonicalize()
                .map_err(|e| PathError::Unresolvable(format!("{candidate}: {e}")))?;
            parent.join(name)
        }
    };

    if !resolved.starts_with(&root) {
        return Err(PathError::Escapes(candidate.to_string()));
    }
    // A resolved path cannot still contain `..` unless canonicalization was
    // skipped for the final component above, which is the create case — an
    // explicit re-check rather than an assumption.
    if resolved
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(PathError::Escapes(candidate.to_string()));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests;
