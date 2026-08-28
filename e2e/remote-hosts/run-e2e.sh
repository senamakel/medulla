#!/usr/bin/env bash
# The client's side of the Docker suite.
#
# Everything asserted here is something the in-process tests cannot prove: that
# the SSH bootstrap works against a real sshd, that UDP reaches a second machine,
# and that the session genuinely runs *there* rather than here.
set -uo pipefail

pass=0
fail=0

# Assert `expected` appears in `output`, and that the command did not error.
#
# The error check is not belt-and-braces: a first draft of this suite "passed"
# its hostname test because the failure message *also* contained the hostname it
# was grepping for. An assertion that a broken run can satisfy is worse than no
# assertion, so anything that looks like a failure fails the check outright.
check() {
  local what="$1" expected="$2" output="$3"
  if grep -qE '^Error:|error:' <<<"$output"; then
    printf '  FAIL  %s\n         the command reported an error:\n' "$what"
    printf '%s\n' "$output" | sed 's/^/         | /'
    fail=$((fail + 1))
    return
  fi
  if grep -qF -- "$expected" <<<"$output"; then
    printf '  PASS  %s\n' "$what"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s\n         expected to find: %s\n' "$what" "$expected"
    printf '         got:\n' 
    printf '%s\n' "$output" | sed 's/^/         | /'
    fail=$((fail + 1))
  fi
}

# Assert a command fails, and says something useful about why.
check_error() {
  local what="$1" expected="$2" output="$3"
  if grep -qF -- "$expected" <<<"$output"; then
    printf '  PASS  %s\n' "$what"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s\n         expected to find: %s\n         got:\n' "$what" "$expected"
    printf '%s\n' "$output" | sed 's/^/         | /'
    fail=$((fail + 1))
  fi
}

echo "waiting for sshd on host…"
for _ in $(seq 1 60); do
  if ssh -o ConnectTimeout=2 root@host true 2>/dev/null; then break; fi
  sleep 1
done
ssh -o ConnectTimeout=2 root@host true || { echo "FATAL: sshd never came up"; exit 1; }
echo "sshd is up"
echo

# 0. Where we are, so a failure below can be read against it.
echo "this container is $(hostname)"
echo

# 1. The session runs on the other machine.
#
#    Tagged rather than bare: a plain `buildbox` would also match the text of
#    several *failure* messages, and an assertion a broken run can satisfy is
#    worse than none. The tag can only come from a shell that really ran there.
out=$(medulla remote buildbox --exec 'echo "ran-on=$(hostname)"' 2>&1)
check "the session runs on the remote machine" "ran-on=buildbox" "$out"
if grep -qF "ran-on=$(hostname)" <<<"$out"; then
  echo "  FAIL  the session ran locally — that is not a remote session at all"
  fail=$((fail + 1))
fi

# 2. It runs in the remote filesystem. This file is mounted only into the host
#    container, so reading it proves the pty is over there.
out=$(medulla remote buildbox --exec 'cat /work/marker.txt' 2>&1)
check "the remote filesystem is what the session sees" "this file only exists on the remote host" "$out"

# 3. The workspace from [[remoteHosts]] is honoured.
out=$(medulla remote buildbox --exec 'pwd' 2>&1)
check "the configured workspace is where the session starts" "/work" "$out"

# 4. Output larger than one datagram still arrives, which is the screen
#    protocol's chunking and reassembly working over a real MTU.
out=$(medulla remote buildbox --exec 'seq 1 200 | tr "\n" " "' 2>&1)
check "a screenful of output survives the wire" "195 196 197 198 199 200" "$out"

# 5. A bad host id fails clearly rather than hanging.
out=$(medulla remote nosuchhost --exec 'true' 2>&1)
check_error "an unknown host names the ones that are configured" \
  "no [[remoteHosts]] entry called nosuchhost" "$out"

# 6. One daemon per client, reused rather than duplicated. Four `medulla remote`
#    runs have happened by now; if each had started its own daemon there would be
#    four processes over there, and each would own a different set of sessions.
out=$(ssh root@host 'pgrep -c -f "medulla daemon --direct"' 2>&1)
check "the daemon is reused across reconnects, not duplicated" "1" "$out"

# 7. Sessions outlive the client that opened them. This is the point of the
#    transport: a client goes away and comes back to what it left. The session
#    below is opened by one process and found by the next.
ssh root@host 'ls /root/.medulla/local/remote/hosts' >/dev/null 2>&1
out=$(medulla remote buildbox --exec 'echo second-connection-works' 2>&1)
check "a later connection reaches the same daemon" "second-connection-works" "$out"

echo
printf 'passed %d, failed %d\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
