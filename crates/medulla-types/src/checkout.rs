//! Value type describing the Git checkout a session is using.

/// Which repository, worktree, and branch a directory sits in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checkout {
    /// The repository directory name.
    pub repo: Option<String>,
    /// The registered linked worktree name, or `None` for the primary checkout.
    pub worktree: Option<String>,
    /// The checked-out branch, or `None` for detached HEAD.
    pub branch: Option<String>,
    /// The abbreviated commit HEAD resolves to.
    pub head: Option<String>,
}

impl Checkout {
    /// Whether this directory is inside a repository.
    pub fn is_repository(&self) -> bool {
        self.repo.is_some()
    }
    /// Whether this is a linked worktree.
    pub fn is_worktree(&self) -> bool {
        self.worktree.is_some()
    }
    /// The branch label, using `@<head>` for detached checkouts.
    pub fn branch_label(&self) -> Option<String> {
        match (&self.branch, &self.head) {
            (Some(branch), _) => Some(branch.clone()),
            (None, Some(head)) => Some(format!("@{head}")),
            (None, None) => None,
        }
    }
    /// A one-line description, or `None` outside a repository.
    pub fn summary(&self) -> Option<String> {
        let repo = self.repo.as_deref()?;
        let mut text = repo.to_owned();
        if let Some(worktree) = &self.worktree {
            text.push_str(" ⑂ ");
            text.push_str(worktree);
        }
        if let Some(branch) = self.branch_label() {
            text.push_str(" · ");
            text.push_str(&branch);
        }
        Some(text)
    }
}
