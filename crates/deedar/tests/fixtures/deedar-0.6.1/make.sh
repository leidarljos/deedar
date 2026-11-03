#!/bin/bash
# Writes this fixture with one deedar binary: a store of five quote deeds, the
# head kept after the third, the signed head after the fifth, the log list,
# one receipt, the bridge from size 3, and a satchel of all five.
# The checked-in copy is the output of the deedar 0.6.1 binary (a58ae53);
# tests/log_0_6_1.rs reads it. The signing seed is 32 bytes of 0x07, a test
# key that signs nothing else, so it is not kept beside the fixture.
#
# usage: make.sh DEEDAR_BIN OUT_DIR
set -euo pipefail
BIN=$1; OUT=$2
mkdir -p "$OUT/home"
export HOME="$OUT/home" XDG_CONFIG_HOME="$OUT/home/.config"
printf '\x07%.0s' $(seq 32) > "$OUT/test-signing.seed"
export DEEDAR_HOST_SIGNING_KEY="$OUT/test-signing.seed"
URL="file://$OUT/store"
d() { "$BIN" --url "$URL" "$@"; }
mint() {
  d create quote --id "deed-quote-compat$1" --name "compat$1" --agent reader \
    --excerpt "what the work said, take $1" --edition https://example.invalid/a \
    --start 0 --end 26 --src-url https://example.invalid/a >/dev/null
}
for n in 0 1 2; do mint $n; done
d log head > "$OUT/kept.txt"
for n in 3 4; do mint $n; done
d log head > "$OUT/head.txt"
d log list > "$OUT/list.txt"
d log prove deed-quote-compat1 > "$OUT/receipt-compat1.txt"
d log bridge 3 > "$OUT/bridge.txt"
d export --into "$OUT/bag" deed-quote-compat0 deed-quote-compat1 deed-quote-compat2 \
  deed-quote-compat3 deed-quote-compat4 > /dev/null
