//! Data types for the Resources settings editor: one [`Row`] per editable field.

/// How a row's value is edited.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// A plain on/off toggle.
    Flag,
    /// A bounded integer: `(min, max, step)`.
    Count(u64, u64, u64),
    /// One of a fixed list of strings.
    Choice(&'static [&'static str]),
    /// A tri-state toggle: automatic, on, or off.
    OptionalFlag,
    /// Free text that may be empty, meaning automatic.
    OptionalText,
}

/// One editable Resources setting and its presentation.
#[derive(Clone, Copy)]
pub(super) struct Row {
    /// Heading the row is listed under.
    pub(super) group: &'static str,
    /// Config key the row reads and persists.
    pub(super) key: &'static str,
    /// Short label shown in the list.
    pub(super) label: &'static str,
    /// One-line explanation shown beside the value.
    pub(super) help: &'static str,
    /// How the value is edited.
    pub(super) kind: Kind,
}

/// Builds a [`Row`] in a `const` context so [`super::rows::ROWS`] can be a static table.
pub(super) const fn row(
    group: &'static str,
    key: &'static str,
    label: &'static str,
    help: &'static str,
    kind: Kind,
) -> Row {
    Row {
        group,
        key,
        label,
        help,
        kind,
    }
}
