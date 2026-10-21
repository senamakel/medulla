#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$repo_root" "${1:-}" <<'PY'
import json
import os
import pathlib
import subprocess
import sys


root = pathlib.Path(sys.argv[1])
metadata_file = sys.argv[2]
if metadata_file:
    metadata = json.loads(pathlib.Path(metadata_file).read_text(encoding="utf-8"))
else:
    output = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(output.stdout)

allowlist = set()
allow_file = pathlib.Path(
    os.environ.get("CRATE_LAYERING_ALLOWLIST", root / "scripts/crate-layering.allow")
)
for line in allow_file.read_text(encoding="utf-8").splitlines():
    line = line.partition("#")[0].strip()
    if line:
        fields = line.split()
        if len(fields) != 2:
            raise SystemExit(f"invalid allowlist entry: {line!r}")
        allowlist.add(tuple(fields))

violations = []
for package in metadata.get("packages", []):
    package_name = package["name"]
    manifest = pathlib.PurePath(package["manifest_path"])
    if "crates" not in manifest.parts:
        continue
    if package_name in {"medulla", "medulla-tui"}:
        continue

    for dependency in package.get("dependencies", []):
        dependency_name = dependency["name"]
        if dependency_name in {"medulla", "medulla-tui"}:
            violations.append(
                f"{package_name} depends on {dependency_name}; primitive crates must remain leaves"
            )
        if (
            dependency_name in {"ratatui", "crossterm"}
            and dependency.get("kind") is None
            and (package_name, dependency_name) not in allowlist
        ):
            violations.append(
                f"{package_name} has a normal dependency on {dependency_name}; "
                "terminal UI dependencies belong in medulla-tui"
            )

if violations:
    print("crate layering violations:", file=sys.stderr)
    for violation in violations:
        print(f"  - {violation}", file=sys.stderr)
    raise SystemExit(1)

print("crate layering check passed")
PY
