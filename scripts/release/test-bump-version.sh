#!/usr/bin/env bash
#
# test-bump-version.sh — exercise bump-version.sh against a scratch copy of a
# minimal manifest and lockfile. Run from the repository root; no cargo needed.

set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/bump-version.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=0

seed() {
  cat > "$work/Cargo.toml" <<'EOF'
[workspace.package]
version = "0.14.1"

[workspace.dependencies]
serde = { version = "1" }
EOF
  cat > "$work/Cargo.lock" <<'EOF'
[[package]]
name = "medulla"
version = "0.14.1"

[[package]]
name = "medulla-btop"
version = "0.14.1"

[[package]]
name = "medulla-link"
version = "0.14.1"

[[package]]
name = "medulla-tui"
version = "0.14.1"

[[package]]
name = "other"
version = "0.14.1"
EOF
}

expect_bump() {
  local kind="$1" want="$2" out
  seed
  out="$(bash "$script" "$kind" "$work" 2>/dev/null)"
  if [[ "$out" != "version=${want}"$'\n'"tag=v${want}" ]]; then
    echo "FAIL ${kind}: got '${out}'"
    failures=$((failures + 1))
    return
  fi
  if [[ "$(grep -c "version = \"${want}\"" "$work/Cargo.lock")" != 4 ]] ||
    ! grep -q '^name = "other"$' "$work/Cargo.lock" ||
    [[ "$(grep -A1 '^name = "other"$' "$work/Cargo.lock" | tail -1)" != 'version = "0.14.1"' ]]; then
    echo "FAIL ${kind}: lockfile not rewritten for exactly the workspace packages"
    failures=$((failures + 1))
    return
  fi
  echo "ok   ${kind} -> ${want}"
}

expect_bump patch 0.14.2
expect_bump minor 0.15.0
expect_bump major 1.0.0

seed
if bash "$script" bogus "$work" >/dev/null 2>&1; then
  echo "FAIL rejects unknown bump type"
  failures=$((failures + 1))
else
  echo "ok   rejects unknown bump type"
fi

seed
sed -i 's/name = "medulla-tui"/name = "renamed"/' "$work/Cargo.lock"
if bash "$script" patch "$work" >/dev/null 2>&1; then
  echo "FAIL rejects a lockfile missing a workspace package"
  failures=$((failures + 1))
elif ! grep -q '^version = "0.14.1"$' "$work/Cargo.toml"; then
  echo "FAIL left Cargo.toml edited after a lockfile failure"
  failures=$((failures + 1))
else
  echo "ok   rejects a lockfile missing a workspace package"
fi

exit "$failures"
