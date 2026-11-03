#!/usr/bin/env bash
# Peak RSS and wall time of `deedar create file`, `evidence` and `export` on
# one artefact, for each deedar binary named. Prints one tab-separated line per
# binary, size and command: the median over RUNS fresh stores.
#
#   measurements/stream-bytes.sh BIN...
#   SIZES="1024" RUNS=3 measurements/stream-bytes.sh BIN...
#
# Needs GNU time at /usr/bin/time. SIZES is in MiB.
set -euo pipefail

sizes=${SIZES:-10 100 1024}
runs=${RUNS:-5}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# The host's own key, if any, would sign into scratch stores that list no
# signer, and `evidence` would fail for that reason.
export DEEDAR_HOST_SIGNING_KEY=off

# One sample: "<peak RSS KiB> <wall ms>" for the command after `--`.
sample() {
  local start end
  start=$(date +%s%N)
  /usr/bin/time -f '%M' -o "$work/rss" "$@" >/dev/null
  end=$(date +%s%N)
  echo "$(cat "$work/rss") $(((end - start) / 1000000))"
}

median() { sort -n | awk '{ v[NR] = $1 } END { print v[int((NR + 1) / 2)] }'; }

printf 'binary\tMiB\tcommand\tpeak_rss_KiB\twall_ms\n'
for mib in $sizes; do
  head -c "$((mib << 20))" /dev/urandom >"$work/artefact.bin"
  for bin in "$@"; do
    : >"$work/create" && : >"$work/evidence" && : >"$work/export"
    for _ in $(seq "$runs"); do
      rm -rf "$work/store" "$work/satchel"
      url="file://$work/store"
      sample "$bin" --url "$url" create file --id deed-file-artefact \
        --path "$work/artefact.bin" --agent bench >>"$work/create"
      sample "$bin" --url "$url" evidence deed-file-artefact >>"$work/evidence"
      sample "$bin" --url "$url" export deed-file-artefact \
        --into "$work/satchel" >>"$work/export"
    done
    for op in create evidence export; do
      rss=$(cut -d' ' -f1 "$work/$op" | median)
      ms=$(cut -d' ' -f2 "$work/$op" | median)
      printf '%s\t%s\t%s\t%s\t%s\n' "$(basename "$bin")" "$mib" "$op" "$rss" "$ms"
    done
  done
done
