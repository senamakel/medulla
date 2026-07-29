![Hero Image](./docs/screen.png)

# Medulla: The Orchestrator

Medulla commands fleets of agent harnesses. Instead of driving [Claude Code](https://www.anthropic.com/claude-code), [Codex](https://github.com/openai/codex), or [OpenCode](https://github.com/sst/opencode) one terminal at a time, you run one orchestrator that decides what work to hand out, places it on a harness that can do it, and keeps a live picture of everything running underneath.

This repository is the open-source Rust workspace behind it: the `medulla` SDK and the `medulla-tui` crate that ships the `medulla` binary, a [ratatui](https://ratatui.rs/) terminal UI over an embedded OpenHuman core.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/tinyhumansai/medulla/main/install.sh | sh
```

This downloads the prebuilt `medulla` binary for your platform, verifies its SHA-256 against the release manifest (when a checksum tool such as `sha256sum`, `shasum`, or `openssl` is available), and installs to `~/.medulla/bin`. If the installer updated your `PATH`, reload your shell first, with `exec $SHELL` or a new terminal, so `medulla` resolves. Then:

```sh
medulla login   # browser OAuth; stores a verified session
medulla         # bare invocation starts the TUI
```

`medulla` runs on an OpenHuman core embedded in the same process — there is no separate server to start and no socket to attach to. With nobody signed in, the TUI opens a login screen; signing in is how you get a working orchestrator. `medulla --mock` runs the offline demo runtime instead, which needs no credentials and no network and is the fastest way to look around.

See [For developers](#for-developers) to build from source or embed the SDK.

## The TUI

The interface is a row of tabs over one orchestrator:

| Tab | What it is for |
| --- | --- |
| **Overview** | The live event feed, and a **This device** panel showing what this machine is hosting. |
| **Agents** | Agent lanes and the chat composer together, with an attachable live harness pane. |
| **Tasks** | Every task and where it came from (**All Tasks**, **Sources**). |
| **Workflows** | Authored multi-step plans: the graph, its runs, and a copilot that edits it. |
| **TokenMaxxxing** | Token spend and headroom (**Overview**, **Bounties**, **Leaderboard**). |
| **Routing** | What capacity exists: **Hosts**, **Harnesses**, **Workspaces**, **Agent Templates**, **Add Host**, **Strategies**. |
| **Memory** | Persona memory is not in this build. The tab says so rather than disappearing. |
| **Settings** | **Usage**, **Appearance**, **Config**, **Trace**, **Context**, **Account**, **Help**. |

The Workflows tab is present when the crate is built with its default `workflows` feature; a slim build without it drops the tab rather than offering one that cannot draw anything.

## Running work on this machine

A plain `medulla` is both halves of the system: the orchestrator that decides what work to hand out, and a host that runs it. The host binds an address on an in-process bus the orchestrator dispatches over, so a task for this machine never touches the network — no tiny.place identity, no contact request, no second process beside the TUI. It is on by default, serves whichever coding-agent CLIs it finds on `PATH` (`claude`, `codex`, `opencode`), and runs them in the directory you launched from. `MEDULLA_HOST=0` turns hosting off for one run.

## Adding another machine

To offer a *different* machine as a worker, run:

```sh
medulla daemon
```

On a terminal this opens the daemon's operator screen. Choose an execution mode and an installed harness, then use its four tabs to watch agent lanes, connect and message a master, manage the workspace roots advertised to that master, and approve incoming requests. The daemon creates and stores a worker-level tiny.place wallet locally; it does not need the master's backend token. Choices persist to the Medulla config, so the usual setup needs no environment variables. Use `medulla daemon --headless` for a service process; a non-terminal launch selects headless mode automatically.

Pairing needs one string to travel — the worker's address — and both halves are copied in the direction that is easy.

1. In the orchestrator, open **Routing › Add Host** and press `c`. That copies a
   single line which installs `medulla` if it is missing and starts the worker.
   Paste it into an SSH session on the machine you want to add.
2. The worker prints its address and hands it to **your** terminal's clipboard
   rather than the remote machine's, using OSC 52, so it survives the SSH
   boundary. Back in the orchestrator, press `a` and paste it, optionally
   followed by a label.

   The clipboard step needs a terminal that accepts OSC 52 — most do, but tmux
   wants `set -g set-clipboard on` and some terminals disable it for security.
   It is also skipped when the daemon's output is piped rather than attached to
   a terminal. Either way the address is printed on a line of its own, so you
   can select it by hand.

To skip the copy entirely, name the worker: run `medulla daemon --handle
build-box` and type `@build-box` into Add Host. Pass `--no-pair` when the
daemon's output is being parsed by a script.

## Telling the orchestrator what there is to work on

This device hosts a harness as well as orchestrating, so it usually has more than
one project on it. **Routing › Workspaces** lists every directory the fleet can
work in — this machine's, which you add with `a` and remove with `d`, and every
other host's, which that machine declares and this page shows read-only.

What is listed here for this device is exactly what reaches the orchestrator as
`capabilities.accessibleDirs`, alongside the harness's own summary of each
project. It is routing context, not a permission grant: a delegated task still
runs in `[host].workspace`. The list persists to `[host].workspaces` and is
advertised from the next launch.

From the command line, `medulla workspace add` drafts a [`MEDULLA.md`](https://tinyhumans.gitbook.io/medulla/features/workspace-profiles) profile for a directory *and* enrols it in the registry the orchestrator reads:

```sh
medulla init                # write a MEDULLA.md and nothing else
medulla workspace add       # profile it and register it
medulla workspace list      # show the registry (--json for machines)
medulla workspace remove .  # unregister; files and MEDULLA.md are left alone
```

## Workflows

A workflow is an authored, durable, multi-step plan whose steps each run on a real harness. The Workflows tab reads, edits, and runs them; the same operations are on the CLI, JSON in and JSON out:

```sh
medulla workflow list                  # every installed workflow
medulla workflow get <id>              # one workflow, whole
medulla workflow create <id>           # install from a document on stdin
medulla workflow validate [id]         # check a saved workflow, or stdin
medulla workflow dry-run <id>          # simulate, dispatching nothing
medulla workflow run <id>              # run against the coding CLIs on this machine
medulla workflow resume <run-id> --approve <node-id>   # release an approval gate
medulla workflow list-runs <id>        # run history
```

`medulla workflow mcp` serves the same operations over MCP. That one is not for a human to run: it is what Medulla attaches to an ACP session so the harness on the other end can author workflows itself. See [docs/workflows.md](docs/workflows.md).

## Harnesses

`medulla claude`, `medulla codex`, and `medulla opencode` launch the real CLI in your terminal exactly as if you had run it directly — unrecognized flags pass through verbatim — while bridging the session to [tiny.place](https://tiny.place) underneath, so an owner can watch it and message into it. `--no-bridge` runs a plain passthrough. `medulla sessions` lists recent Claude and Codex sessions as JSON.

Medulla also speaks [ACP](docs/acp-harnesses.md) to Claude Code, Codex, and OpenCode, which is one standard protocol in place of three bespoke integrations.

Full documentation: **[tinyhumans.gitbook.io/medulla](https://tinyhumans.gitbook.io/medulla)**

## Availability

Medulla is in **early alpha**, and access is gated. It is rolling out to a small group of OpenHuman subscribers first, alongside gated API access for select teams building agentic systems. Alpha partners get direct access to the team, and their workloads shape what Medulla becomes.

Request access and tell us what you are orchestrating.

## Documentation

The full documentation is at **[tinyhumans.gitbook.io/medulla](https://tinyhumans.gitbook.io/medulla)**.

**Features**, what Medulla does day to day:

- [Workers and Sessions](https://tinyhumans.gitbook.io/medulla/features/workers-and-sessions): capacity, threads, and what survives.
- [Tasks and Sources](https://tinyhumans.gitbook.io/medulla/features/tasks-and-sources): where a task comes from and what context it carries.
- [Workflows](https://tinyhumans.gitbook.io/medulla/features/workflows): authored multi-step plans and their runs.
- [MEDULLA.md Workspace Profiles](https://tinyhumans.gitbook.io/medulla/features/workspace-profiles): telling the orchestrator what a repo is.
- [Orchestrator Routing](https://tinyhumans.gitbook.io/medulla/features/routing): cognitive tiers, harness selection, strategies.
- [Token Efficiency and Budgets](https://tinyhumans.gitbook.io/medulla/features/token-efficiency): small surfaces and enforced budgets.

**Developers**, to install the TUI, embed the SDK, and wire your own fleet:

- [Getting Started](https://tinyhumans.gitbook.io/medulla/developers/getting-started): install, build, run, first login.
- [CLI Reference](https://tinyhumans.gitbook.io/medulla/developers/cli-reference): the TUI, the daemon, workflows, the harness wrappers, self-update.
- [Configuration](https://tinyhumans.gitbook.io/medulla/developers/configuration): the Medulla home, layered config, and the runtimes.
- [Authentication](https://tinyhumans.gitbook.io/medulla/developers/authentication): the browser loopback login flow and token handling.
- [Architecture](https://tinyhumans.gitbook.io/medulla/developers/architecture): the SDK/TUI split, the embedded core, and the tiny.place bridge.
- [Contributing](https://tinyhumans.gitbook.io/medulla/developers/contributing): build, test, lint, coverage, and releasing.
- [ACP harness transport](docs/acp-harnesses.md): one standard protocol for Claude Code, Codex, and OpenCode.
- [Workflows](docs/workflows.md): author multi-step plans, and let agents build them.

## For developers

This repo hosts the open-source Medulla Rust workspace: the [`medulla`](https://github.com/tinyhumansai/medulla/tree/main/src/sdk) SDK library and the [`medulla-tui`](https://github.com/tinyhumansai/medulla/tree/main/src/tui) app crate that ships the `medulla` binary.

The prebuilt binary installs with the one-liner under [Install](#install) above. To build from source instead:

```sh
cargo install --path src/tui   # installs the `medulla` binary
medulla                        # bare invocation starts the TUI
```

To work on it:

```sh
cargo run -- --mock                          # the offline demo runtime
cargo test                                   # unit, feature, and mocked e2e tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

`medulla run "<instruction>"` is the non-interactive path: it boots the same embedded core, submits one instruction, and streams the folded cycle events to stdout as JSON lines. It needs no TTY, which is what makes it the one to drive from CI or a container.

Full developer documentation, covering CLI subcommands, configuration, authentication, architecture, and how to build from source, lives in the [Developers](https://tinyhumans.gitbook.io/medulla/developers) section of the docs.

## Why an Orchestrator

Agent harnesses like Claude Code and Codex are remarkable at running one task deeply. But ask a harness to coordinate other harnesses and you hit the same quiet failure mode everywhere: the orchestrator is just another LLM with a transcript, and every harness it manages writes into that transcript. Model accuracy degrades well before the context window fills, so an orchestrator that reads raw harness traffic stops scaling at a handful of them. Long before the window runs out, it stops being able to think.

Orchestration is becoming the dominant pattern in agentic systems, yet it has been running on architectures designed for chat. A chat model manages one thread. An orchestrator has to hold an operation in its head: harnesses in flight, work being decomposed and delegated, results streaming back, decisions made continuously. Medulla is built for that — the bulk of the fleet's output never reaches the orchestrator's context, so what it reasons over stays small and current no matter how much is running underneath.

Fleets with everyone.
