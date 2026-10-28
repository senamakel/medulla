#!/usr/bin/env bash
#
# bump-version.sh — apply a semver bump to the workspace version.
#
# Usage: scripts/release/bump-version.sh <patch|minor|major> [repo-root]
#
# The `[workspace.package]` version in the root Cargo.toml is the one version
# every member crate inherits. Bumping it also means rewriting the lockfile's
# entries for the workspace's own packages, or `cargo build --locked` in the
# release jobs rejects the tree. This script edits both files, verifies that
# they agree, and prints `version=` / `tag=` lines (also appended to
# $GITHUB_OUTPUT when that is set). It does not commit, tag, or push.

set -euo pipefail

bump="${1:-}"
root="${2:-.}"
cargo_toml="${root}/Cargo.toml"
cargo_lock="${root}/Cargo.lock"
# Workspace members whose lockfile entries carry the shared version.
packages=(medulla medulla-link medulla-tui)

case "$bump" in
  patch | minor | major) ;;
  *)
    echo "usage: $0 <patch|minor|major> [repo-root]" >&2
    exit 2
    ;;
esac

# The first `version = "…"` line is the [workspace.package] one; dependency
# versions are written inline (`{ version = … }`) and never start a line.
current="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$cargo_toml" | head -1)"
if [[ ! "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "Cargo.toml workspace version '${current}' is not plain MAJOR.MINOR.PATCH" >&2
  exit 1
fi
major="${BASH_REMATCH[1]}"
minor="${BASH_REMATCH[2]}"
patch="${BASH_REMATCH[3]}"

case "$bump" in
  major) next="$((major + 1)).0.0" ;;
  minor) next="${major}.$((minor + 1)).0" ;;
  patch) next="${major}.${minor}.$((patch + 1))" ;;
esac

python3 - "$cargo_toml" "$cargo_lock" "$current" "$next" "${packages[@]}" <<'PY'
import sys

toml_path, lock_path, current, new, *packages = sys.argv[1:]

with open(toml_path, encoding="utf-8") as f:
    toml = f.read()
old_line = f'\nversion = "{current}"\n'
if old_line not in toml:
    raise SystemExit(f"{toml_path}: no workspace version line for {current}")
toml = toml.replace(old_line, f'\nversion = "{new}"\n', 1)

with open(lock_path, encoding="utf-8") as f:
    lock = f.read()
for name in packages:
    old = f'name = "{name}"\nversion = "{current}"\n'
    if old not in lock:
        raise SystemExit(f"{lock_path}: no {name} package at version {current}")
    lock = lock.replace(old, f'name = "{name}"\nversion = "{new}"\n', 1)

with open(toml_path, "w", encoding="utf-8") as f:
    f.write(toml)
with open(lock_path, "w", encoding="utf-8") as f:
    f.write(lock)
PY

# Re-read both files so a partial edit cannot slip through as a release.
written="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$cargo_toml" | head -1)"
if [[ "$written" != "$next" ]]; then
  echo "Cargo.toml version is ${written} after bump, expected ${next}" >&2
  exit 1
fi
for name in "${packages[@]}"; do
  if ! grep -qzF "name = \"${name}\"
version = \"${next}\"" "$cargo_lock"; then
    echo "Cargo.lock ${name} is not at ${next} after bump" >&2
    exit 1
  fi
done

echo "Bumped ${current} -> ${next} (${bump})" >&2
echo "version=${next}"
echo "tag=v${next}"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "previous=${current}"
    echo "version=${next}"
    echo "tag=v${next}"
  } >> "$GITHUB_OUTPUT"
fi
