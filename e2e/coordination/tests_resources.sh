#!/usr/bin/env bash
# Real-process proof of the host Resources tab under tmux. The app and all
# harnesses run in a network-isolated container; assertions use pane text from
# the running Medulla UI, not rendered fixtures.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SDK_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

SESSION="medulla-e2e-resources-$$"
READY_TIMEOUT_S=90

capture_app() {
  tmux capture-pane -p -t "$SESSION:resources" 2>/dev/null || true
}

wait_app_regex() {
  local regex="$1" timeout="${2:-$READY_TIMEOUT_S}" deadline
  deadline=$(( $(date +%s) + timeout ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if capture_app | grep -Eq "$regex"; then return 0; fi
    sleep 0.5
  done
  capture_app > "$RUN_DIR/resources.pane" || true
  return 1
}

assert_panel_width() {
  local pane_file="$1" label="$2" min_width="$3"
  "$PYTHON_BIN" - "$pane_file" "$label" "$min_width" <<'PY'
import sys
path, label, minimum = sys.argv[1], sys.argv[2], int(sys.argv[3])
lines = open(path, encoding="utf-8").read().splitlines()
matches = [line for line in lines if label in line]
assert matches, f"no captured pane line contains {label!r}"
line = matches[0]
label_at = line.index(label)
left_candidates = [i for i in (line.find("┌"), line.find("├"), line.find("│")) if 0 <= i < label_at]
right_candidates = [i for i in (line.find("┐", label_at), line.find("┤", label_at), line.find("│", label_at)) if i >= 0]
assert left_candidates and right_candidates, f"cannot find panel borders around {label!r}: {line!r}"
width = min(right_candidates) - max(left_candidates) + 1
assert width >= minimum, f"{label} panel width {width} < {minimum}: {line!r}"
print(f"[e2e] {label} panel width: {width} columns")
PY
}

main() {
  RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/medulla-resources-e2e-XXXXXX")"
  e2e_init
  boot_llm ""

  # The app's offline runtime avoids account setup. Its local host still starts
  # the selected real coding CLI as a PTY session, exercising the same process
  # registry that the sampler consumes.
  local app_config="$RUN_DIR/app.json" app_home="$RUN_DIR/app-home" workspace="$RUN_DIR/work"
  mkdir -p "$app_home" "$workspace"
  harness_seed_home "$app_home/harness-home" "$workspace"
  "$PYTHON_BIN" - "$HARNESS" "$RUN_DIR/medulla.$HARNESS.json" "$app_config" <<'PY'
import json, sys
harness, source, target = sys.argv[1:]
try:
    config = json.load(open(source))
except FileNotFoundError:
    config = {}
config["host"] = {"enabled": True, "providers": [harness], "defaultProvider": harness}
config["resources"] = {"updateMs": 2000}
json.dump(config, open(target, "w"), indent=2)
PY

  cat > "$RUN_DIR/resources.cmd" <<EOF
#!/usr/bin/env bash
set -euo pipefail
$(harness_env "$app_home/harness-home")
export HOME=$(printf %q "$app_home")
export MEDULLA_HOME=$(printf %q "$app_home")
export MEDULLA_USER=e2e
export MEDULLA_ANALYTICS_DISABLED=1
echo \$\$ > $(printf %q "$RUN_DIR/app.pid")
exec $(printf %q "$MEDULLA_BIN") --mock --config $(printf %q "$app_config")
EOF
  chmod +x "$RUN_DIR/resources.cmd"
  tmux new-window -t "$SESSION" -n resources -c "$workspace"
  tmux send-keys -t "$SESSION:resources" "bash $(printf %q "$RUN_DIR/resources.cmd")" C-m
  wait_app_regex 'Sessions|Welcome|Start a new session' 60 \
    || fail "Medulla app did not paint its initial screen"

  local pane provider_label
  case "$HARNESS" in
    opencode) provider_label='OpenCode' ;;
    claude) provider_label='Claude Code' ;;
    codex) provider_label='Codex' ;;
  esac
  # Start a harness from the app's Sessions tab (Ctrl-T), choose this device
  # and the sole configured harness, then type the isolated workspace.
  tmux send-keys -t "$SESSION:resources" C-t
  sleep 1
  tmux send-keys -t "$SESSION:resources" Enter
  if ! wait_app_regex 'Claude Code|Codex|OpenCode' 30; then
    capture_app > "$RUN_DIR/harness-picker.pane"
    fail "session picker did not offer the configured $HARNESS harness"
  fi
  local selected attempts=0
  while [ "$attempts" -lt 10 ]; do
    selected="$(capture_app | grep '❯' | head -1 || true)"
    printf '%s' "$selected" | grep -qi "$provider_label" && break
    tmux send-keys -t "$SESSION:resources" Down
    attempts=$((attempts + 1))
    sleep 0.2
  done
  printf '%s' "$selected" | grep -qi "$provider_label" || {
    capture_app > "$RUN_DIR/harness-picker.pane"
    fail "could not select $provider_label from the harness picker"
  }
  tmux send-keys -t "$SESSION:resources" Enter
  sleep 1
  tmux send-keys -t "$SESSION:resources" Enter

  # Move to Resources after the process is launched; it is the fourth tab in
  # the workflow-enabled build and the third in slim builds.
  sleep 3
  tmux send-keys -t "$SESSION:resources" Tab Tab Tab
  wait_app_regex 'CPU.*2000ms' \
    || fail "Resources did not open at its 2000ms default interval"
  pane="$(capture_app)"
  printf '%s\n' "$pane" > "$RUN_DIR/resources-initial.pane"
  for title in 'CPU' 'Mem' 'Net' 'Agents'; do
    printf '%s\n' "$pane" | grep -q "$title" || fail "Resources screen is missing $title title"
  done

  # Wait for the real harness PTY's row to carry its registered session id.
  local deadline=$(( $(date +%s) + READY_TIMEOUT_S ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    pane="$(capture_app)"
    if printf '%s\n' "$pane" | grep -qi "$provider_label"; then break; fi
    sleep 1
  done
  printf '%s\n' "$pane" > "$RUN_DIR/resources-session.pane"
  "$PYTHON_BIN" - "$RUN_DIR/resources-session.pane" "$provider_label" <<'PY' \
    || fail "Resources did not attribute a running $HARNESS process to its session"
import sys
text = open(sys.argv[1], encoding="utf-8").read()
rows = [line for line in text.splitlines() if sys.argv[2].lower() in line.lower()]
assert rows, f"no {sys.argv[2]} process row in Resources pane"
assert any("external" not in line.lower() and "host" not in line.lower() for line in rows), \
    f"{sys.argv[2]} has no non-external session origin: {rows!r}"
print(f"[e2e] session-attributed process row: {rows[0].strip()}")
PY
  log "  app-launched $HARNESS process is visible with its session origin"

  # A second process advertises the harness name in argv but was started by
  # tmux, outside Medulla. It has no registry entry and must render as external.
  tmux new-window -t "$SESSION" -n external -c "$workspace"
  tmux send-keys -t "$SESSION:external" \
    "exec -a $(printf %q "$HARNESS") sleep 180" C-m
  deadline=$(( $(date +%s) + 30 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    pane="$(capture_app)"
    if printf '%s\n' "$pane" | grep -q 'external'; then break; fi
    sleep 1
  done
  printf '%s\n' "$pane" > "$RUN_DIR/resources-external.pane"
  "$PYTHON_BIN" - "$RUN_DIR/resources-external.pane" "$provider_label" <<'PY' \
    || fail "the unregistered $HARNESS process was not labeled external"
import sys
text = open(sys.argv[1], encoding="utf-8").read().splitlines()
rows = [line for line in text if sys.argv[2].lower() in line.lower()]
assert any("external" in line.lower() for line in rows), \
    f"no {sys.argv[2]} row marked external: {rows!r}"
print(f"[e2e] external process row: {[row.strip() for row in rows if 'external' in row.lower()][0]}")
PY
  log "  unregistered $HARNESS-like process is visible with origin external"

  # Toggle Agents off and prove the production-rendered Mem/Net borders widen.
  assert_panel_width "$RUN_DIR/resources-external.pane" 'Mem [2]' 40
  assert_panel_width "$RUN_DIR/resources-external.pane" 'Net [3]' 40
  tmux send-keys -t "$SESSION:resources" 4
  sleep 2
  pane="$(capture_app)"
  printf '%s\n' "$pane" > "$RUN_DIR/resources-agents-hidden.pane"
  printf '%s\n' "$pane" | grep -q 'Agents' && fail "Agents title remained after pressing 4"
  assert_panel_width "$RUN_DIR/resources-agents-hidden.pane" 'Mem [2]' 120
  assert_panel_width "$RUN_DIR/resources-agents-hidden.pane" 'Net [3]' 120
  tmux send-keys -t "$SESSION:resources" 4
  sleep 1
  tmux send-keys -t "$SESSION:resources" +
  wait_app_regex 'CPU.*2100ms' \
    || fail "pressing + did not advance interval label from 2000ms to 2100ms"
  tmux send-keys -t "$SESSION:resources" -
  wait_app_regex 'CPU.*2000ms' \
    || fail "pressing - did not restore the 2000ms sampler interval"
  log "  Agents visibility, Mem/Net width, and interval controls passed"

  # §7 of resources-tab.md specifies 1000 processes at a 2000ms cadence, with
  # sampler CPU p95 <=40ms per tick and <=2% of one core averaged over one
  # minute. Keep 1000 unclassified sleepers alive so they exercise the real
  # process walk without filling the Agents table. Process-level /proc stats
  # conservatively include the rest of Medulla as well as the sampler.
  local perf_seconds="${E2E_RESOURCE_PERF_SECONDS:-60}"
  local app_pid="$(cat "$RUN_DIR/app.pid")"
  "$PYTHON_BIN" - "$app_pid" "$perf_seconds" <<'PY' || fail "Resources process exceeded the sampler CPU budget"
import os, subprocess, sys, time
pid, duration = sys.argv[1], float(sys.argv[2])
fixtures = []
try:
    for _ in range(1000):
        fixtures.append(subprocess.Popen(["sleep", "120"], stdin=subprocess.DEVNULL,
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    assert len(fixtures) == 1000, f"only started {len(fixtures)} process fixtures"
    ticks = os.sysconf("SC_CLK_TCK")
    def cpu_ticks():
        fields = open(f"/proc/{pid}/stat").read().split()
        return int(fields[13]) + int(fields[14])
    if int(duration) < 2:
        raise ValueError("perf sampling must span at least one sampler tick")
    interval = 2.0
    count = max(1, int(duration // interval))
    points = [cpu_ticks()]
    start = time.monotonic()
    for _ in range(count):
        time.sleep(interval)
        points.append(cpu_ticks())
    elapsed = time.monotonic() - start
    tick_ms = [(points[i + 1] - points[i]) / ticks * 1000 for i in range(count)]
    average_pct = sum(tick_ms) / (elapsed * 10)
    ordered = sorted(tick_ms)
    p95 = ordered[min(len(ordered) - 1, int(0.95 * len(ordered)))]
    print(f"[e2e] process fixture: {len(fixtures)} unclassified sleepers; Medulla CPU p95={p95:.1f}ms/tick, average={average_pct:.3f}% of one core over {elapsed:.1f}s (limits: <=40ms/tick, <=2.000%)")
    assert p95 <= 40.0, f"Medulla CPU p95 {p95:.1f}ms per 2s tick exceeds 40ms"
    assert average_pct <= 2.0, f"Medulla CPU average {average_pct:.3f}% exceeds 2% of one core"
finally:
    for proc in fixtures:
        if proc.poll() is None:
            proc.terminate()
    for proc in fixtures:
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
PY

  log "PASS: Resources live process scenarios ($HARNESS)"
}

main "$@"
