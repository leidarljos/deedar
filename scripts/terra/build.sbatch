#!/usr/bin/env bash
#SBATCH --job-name=deedar-build
#SBATCH --time=00:30:00
#SBATCH --nodes=1
#SBATCH --ntasks-per-node=1
#SBATCH --cpus-per-task=4
#SBATCH --mem=8G
#SBATCH --partition=cpu
#SBATCH --output=%x-%j.out
# The release binaries of this checkout on the build host, copied to
# ~/bin-out for the seat to fetch.
set -uo pipefail
cd "$HOME/Git/Github/deedar" || exit 1
git pull -q --ff-only 2>&1 | tail -1
echo "== deedar $(git log --oneline -1 | cut -c1-70)"
cargo fmt --all --check && echo FMT-CLEAN
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -E "^(error|warning)" -A 8 | head -30
cargo build --release --workspace 2>&1 | tail -1
status=$?
mkdir -p "$HOME/bin-out"
for b in target/release/*; do
  [ -f "$b" ] && [ -x "$b" ] && case "$b" in *.d) ;; *) cp -f "$b" "$HOME/bin-out/" && echo "copied $(basename "$b")";; esac
done
echo BUILD-DONE
exit $status
