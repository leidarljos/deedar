`deedar-mcp` opens the same default store as the command line, under `$XDG_DATA_HOME` when it is set, and treats an empty `DEEDAR_URL` as unset.
It used to read an empty store under `~/.local/share` that the command line never wrote.
