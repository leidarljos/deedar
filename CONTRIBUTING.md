# Contributing

## Changelog entries

Do not edit `CHANGELOG.md` in a pull request.
Add one fragment per change under `changelog.d/` instead, named `<PR number>.<type>.md`.
The type is one of `feature`, `bugfix`, `doc`, `removal` or `misc`.
A change with no pull request yet can use `+<branch>.<type>.md`.
Write the entry as one Markdown bullet body, one sentence per line, with no leading dash.
At release time `towncrier build --version X.Y.Z` collects the fragments into `CHANGELOG.md` and removes them.
CI runs `towncrier build --draft --version V`, with V the workspace version in `Cargo.toml`, and fails a fragment whose name is not one of those types.
