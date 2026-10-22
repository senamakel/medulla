# SDK skills facade

The public SDK path `medulla::workflows::skills` adapts workflow-engine
summaries and the file-backed catalog to the harness-independent implementation
in [`medulla-harness`](../../../../medulla-harness/src/skills/README.md).

The facade keeps the SDK's existing workflow summary and refresh signatures.
Rendering, target layouts, managed-file safety, catalog refresh, and MCP
registration live in `medulla-harness::skills` so harness support does not
depend on the workflow engine.
