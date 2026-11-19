`deedar list` at a `--url` or `DEEDAR_URL` path with no store no longer makes the store.
It prints one stderr line that there is no store yet and how to make one, and exits 0 with an empty list.
Every other read exits 1 with that line, and `deedar url` prints the URL without opening it.
`create`, `import` and `host accept` still make the store.
