# Ui

UI-facing data surface shared with the terminal app: the `agents` lanes and the tasks dispatched to them, the `hosts` projection, the `work` panel over a harness's own todos and sub-agents, the `git_review` model behind the session change set, the `checkout` identity that says which repository, worktree, and branch a directory belongs to, and small `util` helpers. Rendering-heavy screens (app, login, composer, theme) and the interactive onboarding screen live in the `medulla-tui` crate, which re-exports these data modules.

## Contents

- [`agents/`](./agents/) — Pure view-model for the agents a session rail lists: one lane per roster agent, each carrying the tasks the hub has dispatched to it, with a row model for the list and pre-wrapped transcript lines.
- [`command/`](./command/) — Slash-command parsing, the command catalog, and the `/copy` transcript helper.
- [`fleet/`](./fleet/) — Pure view-model for the fleet: project the locally registered peers onto the declared containment chain and merge that with whatever the runtime declares.
- [`harness/`](./harness/) — Read-only view-model helpers for the agent-harness contract: a compact task board rendering for a `HarnessStatus` payload, and a one-line budget note for an agent's `AgentBudgetMetadata` seat stamp. Pure formatting only — the `medulla-tui` crate turns the returned `Line`s / strings into ratatui spans.
- [`work/`](./work/) — Rendering a `WorkSnapshot` as display rows: the goal, the todo list, the sub-agents, the files touched, and how the run ended.
- [`workflows/`](./workflows/) — The UI-facing view of installed workflows: their listings, their graphs, and their runs.
- [`meters_tests.rs`](./meters_tests.rs) — Unit tests for the compact meters: bar fill, pressure colouring, the omit-rather-than-zero rule, and the lane usage accumulation.
- [`meters.rs`](./meters.rs) — Compact single-line meters: a labelled bar plus its numbers, sized to fit a header row rather than a dashboard.
- [`mod.rs`](./mod.rs) — UI-facing data surface shared with the terminal app: the `agents` lanes and the tasks dispatched to them, the `hosts` projection, the `work` panel over a harness's own todos and sub-agents, the `git_review` model behind the session change set, the `checkout` identity that says which repository, worktree, and branch a directory belongs to, and small `util` helpers. Rendering-heavy screens (app, login, composer, theme) and the interactive onboarding screen live in the `medulla-tui` crate, which re-exports these data modules.
- [`util_tests.rs`](./util_tests.rs) — Tests for the util module.
- [`util.rs`](./util.rs) — Small display helpers shared by the views.

## Maintenance

Keep this index synchronized when responsibilities move. Put shared data structures in `types.rs`, focused unit tests in `tests.rs` or a sibling `_tests.rs`, and preserve the module-level Rust documentation as the API source of truth.
