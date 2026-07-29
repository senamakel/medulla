---
description: >-
  Medulla is the open-source Rust client and terminal UI for OpenHuman sessions,
  coding-agent workers, local tasks, and durable workflows.
cover: .gitbook/assets/screen.png
---

# Medulla

Medulla is a terminal UI and SDK for working with OpenHuman sessions and real coding-agent harnesses. It runs as a local client: OpenHuman provides the orchestration runtime, while Medulla handles the terminal experience, session protocol, worker adapters, local task state, and workflow execution.

The main surfaces are:

- OpenHuman-backed sessions with live HTTP/SSE updates.
- Worker, host, harness, workspace, and agent views in the TUI.
- A local task ledger with GitHub issue synchronization.
- Durable workflow graphs that run steps on Claude Code, Codex, or OpenCode.
- A daemon and interactive wrappers that connect coding-agent CLIs over tiny.place.
- An offline mock runtime for development and demos.

Start with [Getting started](developers/getting-started.md), then use the [CLI reference](developers/cli-reference.md) for commands. The [Architecture](developers/architecture.md) page explains the SDK and TUI boundary.

## Feature guides

- [Workers and sessions](features/workers-and-sessions.md)
- [Tasks and sources](features/tasks-and-sources.md)
- [Workspace profiles](features/workspace-profiles.md)
- [Orchestrator routing](features/routing.md)
- [Workflows](../docs/workflows.md)

## Project status

This repository is the public client side of the Medulla and OpenHuman system. Backend capabilities and hosted availability can change independently of the SDK. Check the release notes and the linked OpenHuman documentation before relying on a deployment-specific endpoint or provider.
