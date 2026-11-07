#!/usr/bin/env bash
# Release benchmark: times the core commands against the memo chat and
# appends one row per command to bench/results.csv. Sends, edits and
# deletes one message there, nothing else.
#   bench/run.sh "<memo chat name>" [binary]
set -euo pipefail
chat=${1:?usage: bench/run.sh "<memo chat name>" [binary]}
bin=${2:-target/release/openkakao-cli}
csv="$(dirname "$0")/results.csv"
version=$("$bin" --version | awk '{print $2}')
date=$(date +%F)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
[ -f "$csv" ] || echo "version,date,command,ms" > "$csv"
now() { python3 -c 'import time; print(int(time.time() * 1000))'; }
run() { # name, then the command
  local name=$1; shift
  local start end
  start=$(now)
  "$@" >/dev/null 2>&1 || { echo "$name failed" >&2; return 1; }
  end=$(now)
  echo "$version,$date,$name,$((end - start))" | tee -a "$csv"
}
flags=(--json --no-prefix --unattended --allow-non-interactive-send)
text="openkakao bench $(date +%s)"
run ax-read "$bin" --json ax-read "$chat" -n 20
run ax-read-expand "$bin" --json ax-read "$chat" -n 20 --expand
run local-send "$bin" "${flags[@]}" local-send -y "$chat" "$text"
run local-edit "$bin" "${flags[@]}" local-edit -y "$chat" --match "$text" "$text edited"
run local-delete "$bin" "${flags[@]}" local-delete -y "$chat" --match "$text edited" --for-me
run ax-chats-unread "$bin" --json ax-chats --unread
run ax-export "$bin" --json ax-export "$chat" --output "$tmp"
