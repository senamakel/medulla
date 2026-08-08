#!/usr/bin/env bash
# Build (and optionally push) the coordination e2e harness image.
#
#   bash e2e/coordination/build-image.sh              # build locally
#   PUSH=1 IMAGE=ghcr.io/tinyhumansai/medulla_e2e \
#     bash e2e/coordination/build-image.sh            # build and push
#
# One image serves every harness leg: it bakes the release `medulla` binary, the
# two link examples, and all three coding CLIs (opencode, claude, codex). Which
# one a container drives is decided at run time by `E2E_HARNESS`, so the legs
# share this build rather than paying for the Rust stage three times.
#
# `run-docker.sh` builds the same image inline for a one-off local run; this
# script exists for the case where the build and the run are separate steps —
# CI, or a shared image other checkouts pull.
#
# Env:
#   IMAGE=<repo>        image name without tag (default: medulla-e2e)
#   TAGS="a b c"        tags to apply (default: latest)
#   PLATFORM=<p>        target platform (default: the host's)
#   PUSH=1              push after building (requires a prior `docker login`)
#   NO_CACHE=1          build with --no-cache
#   CACHE_FROM/CACHE_TO BuildKit cache specs passed straight through
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SDK_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

IMAGE="${IMAGE:-medulla-e2e}"
TAGS="${TAGS:-latest}"
# Native arch by default. Never force amd64 emulation: the Rust stage and the
# CLIs would run under qemu, which is slow enough to look like a hang.
PLATFORM="${PLATFORM:-linux/$(uname -m | sed 's/aarch64/arm64/;s/x86_64/amd64/')}"

log() { printf '[build-image] %s\n' "$*" >&2; }

args=(buildx build --platform "$PLATFORM" -f "$SCRIPT_DIR/Dockerfile")
for tag in $TAGS; do
  args+=(-t "$IMAGE:$tag")
done
[ "${NO_CACHE:-0}" = "1" ] && args+=(--no-cache)
[ -n "${CACHE_FROM:-}" ] && args+=(--cache-from "$CACHE_FROM")
[ -n "${CACHE_TO:-}" ] && args+=(--cache-to "$CACHE_TO")
# Provenance attestations turn a single-platform build into a manifest list,
# which `docker run` on the same host then refuses to load.
args+=(--provenance=false)
if [ "${PUSH:-0}" = "1" ]; then
  args+=(--push)
else
  args+=(--load)
fi
args+=("$SDK_DIR")

log "building $IMAGE ($TAGS) for $PLATFORM${PUSH:+ and pushing}…"
docker "${args[@]}" >&2

log "done. Run a leg with:"
for harness in opencode claude codex; do
  log "  docker run --rm --network none -e E2E_HARNESS=$harness $IMAGE:${TAGS%% *} bash /app/e2e/coordination/run.sh"
done
