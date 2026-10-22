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

  # Resources is the fourth tab in the workflow-enabled build.
  tmux send-keys -t "$SESSION:resources" Tab Tab Tab
  wait_app_regex 'CPU.*2000ms' \
    || fail "Resources did not open at its 2000ms default interval"
  local pane
  pane="$(capture_app)"
  printf '%s\n' "$pane" > "$RUN_DIR/resources-initial.pane"
  for title in 'CPU' 'Mem' 'Net' 'Agents'; do
    printf '%s\n' "$pane" | grep -q "$title" || fail "Resources screen is missing $title title"
  done

  # Start a harness from the app (Ctrl-T), choose this device and the sole
  # configured harness, then type the isolated workspace into the picker.
  tmux send-keys -t "$SESSION:resources" C-t
  sleep 1
  tmux send-keys -t "$SESSION:resources" Enter
  sleep 1
  local picker_line
  picker_line="$(capture_app | grep -Ei "[❯ ]+(${HARNESS}|OpenCode|Claude|Codex)" | head -1 || true)"
  if [ -z "$picker_line" ]; then
    capture_app > "$RUN_DIR/harness-picker.pane"
    fail "session picker did not offer the configured $HARNESS harness"
  fi
  tmux send-keys -t "$SESSION:resources" Enter
  sleep 1
  tmux send-keys -t "$SESSION:resources" -l "$workspace"
  tmux send-keys -t "$SESSION:resources" Enter

  # Wait for a real harness PTY and its registered session origin.
  local deadline=$(( $(date +%s) + READY_TIMEOUT_S ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    pane="$(capture_app)"
    if printf '%s\n' "$pane" | grep -qiE "${HARNESS}.*session|session.*${HARNESS}"; then break; fi
    sleep 1
  done
  printf '%s\n' "$pane" > "$RUN_DIR/resources-session.pane"
  printf '%s\n' "$pane" | grep -qiE 'session|host' \
    || fail "Resources did not attribute a running $HARNESS process to its session"
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
  printf '%s\n' "$pane" | grep -q 'external' \
    || fail "the unregistered $HARNESS process was not labeled external"
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
  log "  Agents visibility, Mem/Net width, and interval controls passed"

  # §7 of resources-tab.md sets the default-cadence average at <=2% of one
  # core over one minute. Measure the real Medulla process in /proc while the
  # Resources view is active. This is a conservative upper bound that includes
  # the rest of the app; Linux is the CI fixture and the result is logged.
  local perf_seconds="${E2E_RESOURCE_PERF_SECONDS:-60}"
  local app_pid="$(cat "$RUN_DIR/app.pid")"
  "$PYTHON_BIN" - "$app_pid" "$perf_seconds" <<'PY' || fail "Resources process exceeded the 2% one-core budget"
import os, sys, time
pid, duration = sys.argv[1], float(sys.argv[2])
def cpu_ticks():
    fields = open(f"/proc/{pid}/stat").read().split()
    return int(fields[13]) + int(fields[14])
ticks = os.sysconf("SC_CLK_TCK")
before_ticks, before_time = cpu_ticks(), time.monotonic()
time.sleep(duration)
after_ticks, after_time = cpu_ticks(), time.monotonic()
percent = (after_ticks - before_ticks) / ticks / (after_time - before_time) * 100
print(f"[e2e] Medulla Resources process CPU: {percent:.3f}% of one core over {after_time-before_time:.1f}s (budget <=2.000%)")
assert percent <= 2.0, f"process CPU {percent:.3f}% exceeds 2% budget"
PY

  log "PASS: Resources live process scenarios ($HARNESS)"
}

main "$@"
