# Dockerized coordination e2e harness.
#
# Build context MUST be the sdk/ crate root:
#   docker build -f e2e/coordination/Dockerfile -t medulla-e2e .
# (run-docker.sh does this for you.)
#
# Stage 1 bakes the release `medulla` binary plus the two examples the harness
# drives. Stage 2 is a slim Debian runtime with tmux + python3 + all three real
# coding CLIs (opencode, claude, codex), and it hands run.sh prebuilt binaries
# via the *_BIN env vars so nothing is recompiled inside the container.
#
# One image serves all three harness legs; pick one at run time:
#   docker run --rm --network none -e E2E_HARNESS=claude medulla-e2e

# ---------------------------------------------------------------------------
# Stage 1: compile medulla + examples (release)
# ---------------------------------------------------------------------------
# Debian (glibc) base so the resulting binaries match the glibc runtime stage.
FROM rust:1.96.1-bookworm AS build

WORKDIR /build

# System deps for the vendored OpenHuman core's native builds. Needed only when
# the `openhuman-core` feature is on, but installed unconditionally so enabling
# it never turns into a confusing link failure here.
#   - libssl-dev + pkg-config: openssl-sys links the SYSTEM libssl
#     (-lssl -lcrypto), reached via native-tls <- hyper-tls <- reqwest, which an
#     OpenHuman dependency turns on. It is NOT vendored.
#   - X11 set: `rdev`, `enigo` and `arboard` are NON-optional OpenHuman
#     dependencies (input events, synthetic input, clipboard), so they are in
#     the graph even on a headless build and their build scripts fail without
#     these. Not needed by medulla itself.
#   - libgit2 needs nothing: libgit2-sys builds its own C (libgit2_vendored)
#     using cc, which the rust image already provides.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev libx11-dev libxi-dev libxtst-dev libxdo-dev \
 && rm -rf /var/lib/apt/lists/*

# The workspace's vendored path deps all live under vendor/openhuman/vendor/*
# now (see docs/vendoring.md). src/ carries every member crate (src/sdk,
# src/tui, src/link) including their tests/ and examples/ trees.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY vendor ./vendor

# One shot; path-dep crates make the dummy-main cache trick brittle, so just
# build. Cold compile is ~25 min. A BuildKit cache mount keeps the downloaded
# crate registry across rebuilds so only compilation reruns, not the fetch.
# (target/ is NOT cache-mounted — the runtime stage COPYs binaries out of it.)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release \
    --bin medulla \
    --example mock_link_forwarder \
    --example coordination_owner

# ---------------------------------------------------------------------------
# Stage 2: runtime — tmux + python3 + all three coding CLIs + baked binaries
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

# Every CLI the harness can drive is baked in, pinned. The image is one image
# for all three legs rather than three images: they share the expensive Rust
# build stage, and a leg is selected at *run* time with `-e E2E_HARNESS=…`.
#
#   opencode  sst/opencode, published under anomalyco/opencode. glibc tarball to
#             match the debian runtime.
#   claude    Anthropic's own release channel. The version is resolved against
#             the published manifest, whose checksum is then verified — the same
#             thing `claude.ai/install.sh` does, without piping a script whose
#             content is not pinned.
#   codex     openai/codex. The musl build, which is static and therefore
#             indifferent to the runtime's glibc.
#
# BuildKit sets TARGETARCH only when --platform is passed; otherwise fall back to
# the native dpkg architecture (never default it — a wrong-arch binary surfaces
# as an "Exec format error" far from here).
ARG OPENCODE_VERSION=1.17.18
ARG CLAUDE_VERSION=2.1.226
ARG CODEX_VERSION=0.147.0
ARG TARGETARCH

RUN set -eux; \
    apt-get update; \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        tmux python3 curl bash ca-certificates git; \
    rm -rf /var/lib/apt/lists/*; \
    ARCH="${TARGETARCH:-$(dpkg --print-architecture)}"; \
    case "$ARCH" in \
        arm64) OC_ARCH=arm64; CLAUDE_PLATFORM=linux-arm64; CODEX_TRIPLE=aarch64-unknown-linux-musl ;; \
        amd64) OC_ARCH=x64;   CLAUDE_PLATFORM=linux-x64;   CODEX_TRIPLE=x86_64-unknown-linux-musl ;; \
        *) echo "unsupported arch=$ARCH" >&2; exit 1 ;; \
    esac; \
    DL="curl -fsSL --http1.1 --retry 5 --retry-delay 3 --retry-all-errors"; \
    \
    $DL -o /tmp/opencode.tar.gz \
        "https://github.com/anomalyco/opencode/releases/download/v${OPENCODE_VERSION}/opencode-linux-${OC_ARCH}.tar.gz"; \
    tar -xzf /tmp/opencode.tar.gz -C /usr/local/bin; \
    rm /tmp/opencode.tar.gz; \
    chmod +x /usr/local/bin/opencode; \
    /usr/local/bin/opencode --version; \
    \
    CLAUDE_BASE="https://downloads.claude.ai/claude-code-releases"; \
    $DL -o /tmp/claude-manifest.json "${CLAUDE_BASE}/${CLAUDE_VERSION}/manifest.json"; \
    CLAUDE_SUM="$(python3 -c "import json,sys; print(json.load(open('/tmp/claude-manifest.json'))['platforms']['${CLAUDE_PLATFORM}']['checksum'])")"; \
    $DL -o /usr/local/bin/claude "${CLAUDE_BASE}/${CLAUDE_VERSION}/${CLAUDE_PLATFORM}/claude"; \
    echo "${CLAUDE_SUM}  /usr/local/bin/claude" | sha256sum -c -; \
    rm /tmp/claude-manifest.json; \
    chmod +x /usr/local/bin/claude; \
    /usr/local/bin/claude --version; \
    \
    $DL -o /tmp/codex.tar.gz \
        "https://github.com/openai/codex/releases/download/rust-v${CODEX_VERSION}/codex-${CODEX_TRIPLE}.tar.gz"; \
    tar -xzf /tmp/codex.tar.gz -C /tmp; \
    mv "/tmp/codex-${CODEX_TRIPLE}" /usr/local/bin/codex; \
    rm /tmp/codex.tar.gz; \
    chmod +x /usr/local/bin/codex; \
    /usr/local/bin/codex --version

# Prebuilt binaries from the build stage, onto PATH.
COPY --from=build /build/target/release/medulla                      /usr/local/bin/medulla
COPY --from=build /build/target/release/examples/mock_link_forwarder /usr/local/bin/mock_link_forwarder
COPY --from=build /build/target/release/examples/coordination_owner  /usr/local/bin/coordination_owner

# The harness scripts (run.sh, mock_llm.py, the per-harness configs, fixtures).
WORKDIR /app
COPY e2e/coordination /app/e2e/coordination

# Hand run.sh its prebuilt binaries so it never shells out to cargo, and every
# coding CLI by absolute path so PATH order can never pick a different one.
# `E2E_HARNESS` chooses which of them the run actually drives.
ENV MEDULLA_BIN=/usr/local/bin/medulla \
    FORWARDER_BIN=/usr/local/bin/mock_link_forwarder \
    OWNER_BIN=/usr/local/bin/coordination_owner \
    OPENCODE_BIN=/usr/local/bin/opencode \
    CLAUDE_BIN=/usr/local/bin/claude \
    CODEX_BIN=/usr/local/bin/codex \
    OPENCODE_DISABLE_AUTOUPDATE=1 \
    DISABLE_AUTOUPDATER=1 \
    E2E_HARNESS=opencode \
    TERM=xterm-256color

# Not root. Claude Code refuses `--dangerously-skip-permissions` under root or
# sudo, which is exactly the flag a fleet host runs its harness with — so a root
# container would either fail the claude leg or force it to be tested with a
# flag combination no real host uses. The other two CLIs are equally happy here,
# and an unprivileged run is closer to a real host besides.
RUN useradd --create-home --shell /bin/bash e2e
USER e2e
ENV HOME=/home/e2e

CMD ["bash", "/app/e2e/coordination/run.sh"]
