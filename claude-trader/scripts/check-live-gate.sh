#!/usr/bin/env bash
# Safety gate (SPEC section 11): live signing or sending code may exist ONLY under engine/src/live/.
# Exits non-zero if any such pattern is found anywhere else in the source trees.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

PATTERNS='sendTransaction|send_transaction|sendRawTransaction|send_raw_transaction|Keypair::from|Keypair\.fromSecretKey|trade-local|signTransaction|sign_transaction'
DIRS=()
for d in engine/src app/src-tauri/src app/src; do
  [ -d "$d" ] && DIRS+=("$d")
done
[ ${#DIRS[@]} -eq 0 ] && { echo "live-gate: no source dirs found"; exit 0; }

HITS="$(grep -rnE --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.js' --include='*.jsx' \
  --exclude-dir=node_modules --exclude-dir=target --exclude-dir=dist \
  "$PATTERNS" "${DIRS[@]}" 2>/dev/null | grep -v '^engine/src/live/')"

if [ -n "$HITS" ]; then
  echo "live-gate FAILED: live signing/sending pattern found outside engine/src/live/:" >&2
  echo "$HITS" >&2
  exit 1
fi
echo "live-gate ok: no live signing/sending code outside engine/src/live/"
exit 0
