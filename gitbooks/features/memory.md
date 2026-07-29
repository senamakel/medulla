# Local state and session history

The old persona-memory guide no longer describes the current client. Medulla's
local state is split by purpose: tasks live in `tasks.json`, workflow definitions
and run records live in the workflow store, workspace registrations live in the
configuration, and session history is kept for the session-listing and upload
surfaces. OpenHuman owns the hosted orchestration session and its messages.

See [Tasks and Sources](tasks-and-sources.md), [Workflows](../../docs/workflows.md),
and [Architecture](../developers/architecture.md) for the supported interfaces.
