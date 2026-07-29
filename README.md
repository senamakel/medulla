![Medulla terminal UI](./docs/screen.png)

# Medulla

Medulla is an open-source Rust client and terminal UI for running work through OpenHuman and local coding-agent harnesses. It gives you one place to chat with an orchestrator, follow live harness sessions, manage workers and workspaces, keep a local task list, and run durable multi-step workflows.

The public repository contains two crates:

- `medulla`, a UI-free SDK with backend HTTP/SSE, local core-socket, mock, daemon, task, workflow, session, and tiny.place integrations.
- `medulla-tui`, the `ratatui` application that ships the `medulla` binary.

Medulla is a client. The OpenHuman core and hosted services provide the orchestration runtime; this repository provides the terminal experience, protocol clients, worker adapters, and local state.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/tinyhumansai/medulla/main/install.sh | sh
medulla login
medulla
```

The installer verifies the release checksum when a checksum tool is available and installs to `~/.medulla/bin`. With no credentials, the TUI offers the offline mock runtime. You can also run it directly with `medulla --mock`.

To offer a machine's installed coding-agent CLIs as a worker:

```sh
medulla daemon
```

Use `medulla daemon --headless` for a service process. The daemon supports Claude Code, Codex, and OpenCode, and communicates with an OpenHuman owner over encrypted tiny.place messages. `medulla codex`, `medulla claude`, and `medulla opencode` wrap an interactive local harness and can bridge its session to an owner.

## Current features

### OpenHuman sessions

The TUI uses the OpenHuman session and message APIs through the SDK's runtime boundary. Sessions persist across restarts, stream events over HTTP/SSE, and expose task state, agent lanes, token usage, pending questions, and live harness output. The local core-socket runtime and scripted mock runtime implement the same UI-facing contract.

### Workers, hosts, and workspaces

The Routing and Agents surfaces show the placement chain:

```text
Host -> Harness -> Workspace -> Agent
```

Hosts can be paired over tiny.place. Harnesses describe installed provider CLIs and their capacity. Workspaces identify where work may run and can include an advisory `MEDULLA.md` profile. Agents are rendered from the declared placement rather than guessed from incomplete data. The daemon also lets a machine advertise approved workspace roots and review incoming contact requests.

### Local tasks and GitHub sources

The Tasks tab stores an operator-owned task list in `tasks.json` under the Medulla home. Tasks support descriptions, status, stable IDs, recurrence, and source metadata. GitHub issues can be synchronized into the list with configurable repository, state, label, filter, and token settings. Local edits survive synchronization, and writes use locking plus atomic replacement.

### Durable workflows

Workflows are saved directed acyclic graphs whose agent steps run on real harnesses. They can be created and edited through the CLI, TUI copilot, or the workflow MCP server, then validated, dry-run, started, resumed, inspected, and cancelled. Workflow files may live in the user store or in a repository's `.medulla/workflows` directory. Runs keep records and checkpoints so a paused workflow can continue after a restart.

### Harness visibility and control

Medulla normalizes provider events from Claude Code, Codex, and OpenCode into shared session data. The Agents tab can show the live terminal screen, plans, todo items, sub-agents, files, and diagnostics when a provider reports them. The daemon and wrappers preserve provider-specific credentials and run the actual CLI in its own workspace.

## Documentation

The [GitBook documentation](https://tinyhumans.gitbook.io/medulla) covers:

- [Getting started](gitbooks/developers/getting-started.md)
- [CLI reference](gitbooks/developers/cli-reference.md)
- [Tasks and sources](gitbooks/features/tasks-and-sources.md)
- [Workflows](docs/workflows.md)
- [Workers and sessions](gitbooks/features/workers-and-sessions.md)
- [Workspace profiles](gitbooks/features/workspace-profiles.md)
- [Routing](gitbooks/features/routing.md)
- [Architecture](gitbooks/developers/architecture.md)
- [Configuration](gitbooks/developers/configuration.md)
- [Authentication](gitbooks/developers/authentication.md)

## Build and test

```sh
make init
cargo run -- --mock
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The workspace uses recursive submodules, including the vendored workflow engine. See [Contributing](gitbooks/developers/contributing.md) for the full development setup.

## Repository layout

Reusable protocol and runtime code belongs in [`src/sdk`](src/sdk). Rendering, input handling, and process wiring belong in [`src/tui`](src/tui). Generated `target/` and `.medulla-state/` data should never be committed.
