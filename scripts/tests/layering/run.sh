#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
checker="$repo_root/scripts/check-crate-layering.sh"
fixtures="$repo_root/scripts/tests/layering"

"$checker" "$fixtures/good.json"
if "$checker" "$fixtures/bad.json"; then
    echo "expected bad layering fixture to fail" >&2
    exit 1
fi
