#!/usr/bin/env bash
# The client's side of the Docker suite.
#
# Everything asserted here is something the in-process tests cannot prove: that
# the SSH bootstrap works against a real sshd, that UDP reaches a second machine,
# and that the session genuinely runs *there* rather than here.
set -uo pipefail

pass=0
fail=0

check() {
  local what="$1" expected="$2" output="$3"
  if grep -qF -- "$expected" <<<"$output"; then
    printf '  PASS  %s\n' "$what"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s\n         expected to find: %s\n' "$what" "$expected"
    printf '         got:\n%s\n' "$output" | sed 's/^/         | /'
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

# 1. The session runs on the other machine. `hostname` is the cleanest possible
#    proof: the client is `medulla-e2e-client`, the host is `buildbox`.
out=$(medulla remote buildbox --exec 'hostname' 2>&1)
check "the session runs on the remote machine, not this one" "buildbox" "$out"

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
check "an unknown host names the ones that are configured" "buildbox" "$out"

echo
printf 'passed %d, failed %d\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
