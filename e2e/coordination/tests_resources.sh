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
  boot_llm "export MOCK_LLM_RESPONSE_DELAY_MS=15000"

  # Run a minimal real workflow in the client TUI. Its local workflow host
  # dispatches the real harness from the same Medulla process as the sampler,
  # so the resource row can be joined to its registered session origin.
  local app_config="$RUN_DIR/app.json" app_home="$RUN_DIR/app-home" workspace="$RUN_DIR/work"
  mkdir -p "$app_home" "$workspace/.medulla/workflows"
  harness_seed_home "$app_home/harness-home" "$workspace"
  "$PYTHON_BIN" - "$HARNESS" "${MEDULLA_CONFIG:-}" "$app_config" "$workspace/.medulla/workflows/resources-proof.json" <<'PY'
import json, sys
harness, source, target, workflow_path = sys.argv[1:]
try:
    config = json.load(open(source)) if source else {}
except FileNotFoundError:
    config = {}
config["resources"] = {"updateMs": 2000}
config["workflows"] = {"defaultProvider": harness}
json.dump(config, open(target, "w"), indent=2)
workflow = {
    "id": "resources-proof", "name": "Resources proof",
    "description": "Keep a real harness request visible for Resources attribution.",
    "nodes": [
        {"id": "trigger", "kind": "trigger", "name": "start", "config": {"trigger_kind": "manual"}},
        {"id": "agent", "kind": "agent", "name": "Keep harness live", "config": {"prompt": "reply with the coordination marker RESOURCES_PROOF"}},
    ],
    "edges": [{"from_node": "trigger", "to_node": "agent"}],
}
json.dump(workflow, open(workflow_path, "w"), indent=2)
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
cd $(printf %q "$workspace")
exec $(printf %q "$MEDULLA_BIN") --mock --config $(printf %q "$app_config")
EOF
  chmod +x "$RUN_DIR/resources.cmd"
  tmux new-window -t "$SESSION" -n resources -c "$workspace"
  tmux send-keys -t "$SESSION:resources" "bash $(printf %q "$RUN_DIR/resources.cmd")" C-m
  wait_app_regex 'Sessions|Welcome|Start a new session' 60 \
    || fail "Medulla app did not paint its initial screen"
  tmux send-keys -t "$SESSION:resources" Tab
  wait_app_regex 'Resources proof' || fail "the workflow fixture was not loaded"
  tmux send-keys -t "$SESSION:resources" x
  sleep 1
  tmux send-keys -t "$SESSION:resources" Tab Tab
  wait_app_regex 'CPU.*2000ms' \
    || fail "Resources did not open at its 2000ms default interval"

  local pane provider_label
  case "$HARNESS" in
    opencode) provider_label='OpenCode' ;;
    claude) provider_label='Claude' ;;
    codex) provider_label='Codex' ;;
  esac
  pane="$(capture_app)"
  printf '%s\n' "$pane" > "$RUN_DIR/resources-initial.pane"
  for title in 'CPU' 'Mem' 'Net' 'Agents'; do
    printf '%s\n' "$pane" | grep -q "$title" || fail "Resources screen is missing $title title"
  done

  # The workflow ID is the process registry's task origin until the provider
  # reports a native session id. Wait for that exact origin in production text.
  local task_id="resources-proof" deadline=$(( $(date +%s) + READY_TIMEOUT_S ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    pane="$(capture_app)"
    if printf '%s\n' "$pane" | grep -qi "$provider_label"; then break; fi
    sleep 1
  done
  printf '%s\n' "$pane" > "$RUN_DIR/resources-session.pane"
  "$PYTHON_BIN" - "$RUN_DIR/resources-session.pane" "$provider_label" "$task_id" <<'PY' \
    || fail "Resources did not attribute a running $HARNESS process to its session"
import sys
text = open(sys.argv[1], encoding="utf-8").read()
rows = [line for line in text.splitlines() if sys.argv[2].lower() in line.lower()]
assert rows, f"no {sys.argv[2]} process row in Resources pane"
assert any("external" not in line.lower() and "host" not in line.lower() for line in rows), \
    f"{sys.argv[2]} has no task/session origin: {rows!r}"
print(f"[e2e] session-attributed process row: {rows[0].strip()}")
PY
  log "  dispatched $HARNESS process is visible with session origin $task_id"

  # A second process advertises the harness name in argv but was started by
  # tmux, outside Medulla. It has no registry entry and must render as external.
  tmux new-window -t "$SESSION" -n external -c "$workspace"
  tmux send-keys -t "$SESSION:external" \
    "echo \$\$ > $(printf %q "$RUN_DIR/external.pid"); exec -a opencode sleep 180" C-m
  deadline=$(( $(date +%s) + 30 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    pane="$(capture_app)"
    if printf '%s\n' "$pane" | grep -q 'external'; then break; fi
    sleep 1
  done
  printf '%s\n' "$pane" > "$RUN_DIR/resources-external.pane"
  "$PYTHON_BIN" - "$RUN_DIR/resources-external.pane" "$(cat "$RUN_DIR/external.pid")" <<'PY' \
    || fail "the unregistered $HARNESS process was not labeled external"
import sys
text = open(sys.argv[1], encoding="utf-8").read().splitlines()
rows = [line for line in text if sys.argv[2] in line]
assert rows and any("External" in line and "external" in line.lower() for line in rows), \
    f"OpenCode PID {sys.argv[2]} has no External/external row: {rows!r}"
print(f"[e2e] external process row: {rows[0].strip()}")
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
