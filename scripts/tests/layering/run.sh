#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
checker="$repo_root/scripts/check-crate-layering.sh"
fixtures="$repo_root/scripts/tests/layering"
allowlist="$(mktemp)"
trap 'rm -f "$allowlist"' EXIT

"$checker" "$fixtures/good.json"
if "$checker" "$fixtures/bad.json"; then
    echo "expected bad layering fixture to fail" >&2
    exit 1
fi
if "$checker" "$fixtures/terminal-dependencies.json"; then
    echo "expected normal ratatui/crossterm dependencies to fail" >&2
    exit 1
fi
"$checker" "$fixtures/dev-build-terminal-dependencies.json"

printf 'medulla-link ratatui\nmedulla-link crossterm\n' > "$allowlist"
CRATE_LAYERING_ALLOWLIST="$allowlist" \
    "$checker" "$fixtures/terminal-dependencies.json"
