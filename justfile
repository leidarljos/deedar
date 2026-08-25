set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments

# Seat store. Override with DEEDER_URL=file:///abs/path
url := env("DEEDER_URL", "file://" + justfile_directory() + "/.deeder")

default:
    @just --list

# Run the deeder command against the seat store.
# Example: just deeder list
#          just deeder create quote --id deed-quote-rfc2094-nll --excerpt "…" --src-url https://…
deeder *args:
    @cargo run -q -p deeder-cli -- --url "{{ url }}" "$@"

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
