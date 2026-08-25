set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments

# Store. Override with DEEDAR_URL=file:///abs/path
url := env("DEEDAR_URL", "file://" + justfile_directory() + "/.deedar")

default:
    @just --list

# Run the deedar command against the store.
# Example: just deedar list
#          just deedar create quote --id deed-quote-rfc2094-nll --excerpt "…" --src-url https://…
deedar *args:
    @cargo run -q -p deedar-cli -- --url "{{ url }}" "$@"

check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

test:
    cargo test --workspace

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

build:
    cargo build --workspace --release
