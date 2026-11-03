#!/usr/bin/env python3
"""Transcripts for the README and the org guides.

Cog calls ``transcript`` and writes the text into the page. ``cog --check``
fails when a later run of the same commands differs.

The log leaf hashes the evidence time, so the printed root changes every
second unless the clock is held still. The binary reads the wall clock and
has no sample switch, so a small interposer answers ``clock_gettime`` with
``DEEDAR_SAMPLE_UNIX`` for these processes only.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# The log leaf hashes this instant. It has to be one fixed second so two
# machines print one root; 1700000000 (2023-11-14T22:13:20Z) is a round
# value a reader can tell from a wall clock. The date itself is not special.
SAMPLE_UNIX = 1_700_000_000

# A server that cannot start exits before this returns; one that can is
# still waiting on stdin, because it speaks only after an initialize
# request. One second is longer than this process takes to reach that wait
# on a cold runner, and short enough that regenerating the pages does not
# sit on it.
MCP_WAIT_SECONDS = 1

# Ed25519 seeds are 32 bytes, which is what the store loads. The sample
# seed is zeros so every run signs with one key and the signature repeats.
SEED_BYTES = 32

# Display path, then the path under the scratch directory. Longer display
# paths are substituted first so ``/tmp/bag`` does not eat ``/tmp/bag2``.
DISPLAYS = (
    ("/tmp/deeds", "store"),
    ("/tmp/recv", "recv"),
    ("/tmp/bag2", "bag2"),
    ("/tmp/bag", "bag"),
    ("/tmp/note2.rs", "note2.rs"),
    ("/tmp/note.rs", "note.rs"),
    ("/tmp/sender.head", "sender.head"),
    # Not ``store/../host.key``: that path is the keyed-hash host key, and
    # the sample seed has to stay the signing key only.
    ("/tmp/host.key", "keys/host.key"),
)

# ``deedar-mcp`` has to be tried before ``deedar``, or the shorter name
# consumes the longer one.
_COMMAND = re.compile(r"(?<![\w/.-])deedar-mcp(?![\w-])|(?<![\w/.-])deedar(?![\w-])")
_ACCESSION = re.compile(r"(?m)^accession=(\S+)\s*$")
_LOG_SIZE = re.compile(r"log of (\d+) entries")
_SIGNER = re.compile(r"ed25519:[0-9a-f]{64}")

# How each transcript is wrapped. ``md`` is a console fence, ``org`` an Org
# console block, ``text`` an Org text block.
FENCES = {
    "readme": "md",
    "gs-store": "org",
    "gs-mint": "org",
    "gs-check": "org",
    "gs-supersede": "org",
    "gs-export": "org",
    "gs-receive": "org",
    "ht-pipe": "org",
    "ht-sign": "org",
    "ht-keep": "org",
    "ht-import": "org",
    "ht-audit": "org",
    "ht-backfill": "org",
    "ht-attest": "text",
    "ht-delete": "org",
    "ht-mcp": "org",
}

_READY = False
_BINS: dict[str, Path] = {}
_SHIM: Path | None = None
_CACHED: dict[str, str] | None = None

_SHIM_C = r"""
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

/* DEEDAR_SAMPLE_UNIX is the whole clock. Unset or garbage exits, so a run
   that forgot it cannot write a transcript that will not repeat. */

static time_t sample_unix(void) {
    const char *raw = getenv("DEEDAR_SAMPLE_UNIX");
    char *end = NULL;
    long long parsed;
    if (raw == NULL || *raw == '\0') {
        fprintf(stderr, "DEEDAR_SAMPLE_UNIX is unset\n");
        _exit(1);
    }
    parsed = strtoll(raw, &end, 10);
    if (end == raw || *end != '\0' || parsed < 0) {
        fprintf(stderr, "DEEDAR_SAMPLE_UNIX is not a unix time\n");
        _exit(1);
    }
    return (time_t)parsed;
}

int clock_gettime(clockid_t clk_id, struct timespec *tp) {
    if (clk_id == CLOCK_REALTIME || clk_id == CLOCK_REALTIME_COARSE) {
        tp->tv_sec = sample_unix();
        tp->tv_nsec = 0;
        return 0;
    }
    int (*next)(clockid_t, struct timespec *) = dlsym(RTLD_NEXT, "clock_gettime");
    return next(clk_id, tp);
}

int gettimeofday(struct timeval *tv, void *tz) {
    (void)tz;
    tv->tv_sec = sample_unix();
    tv->tv_usec = 0;
    return 0;
}

time_t time(time_t *tloc) {
    time_t now = sample_unix();
    if (tloc) {
        *tloc = now;
    }
    return now;
}
"""


def repo_root() -> Path:
    here = Path(__file__).resolve()
    for path in [here, *here.parents]:
        if (path / "Cargo.toml").is_file() and (path / "crates").is_dir():
            return path
    raise SystemExit("samples.py is not inside the deedar checkout")


def clean_env() -> dict[str, str]:
    """Environment for tools that must see the real clock."""
    env = os.environ.copy()
    # A pinned clock in the past makes build tools treat today's files as
    # future input and rebuild forever. Only the sample commands see it.
    env.pop("LD_PRELOAD", None)
    env.pop("DEEDAR_SAMPLE_UNIX", None)
    return env


def ensure_tools() -> None:
    global _READY, _SHIM
    if _READY:
        return
    root = repo_root()
    compiler = shutil.which("cc") or shutil.which("gcc")
    if compiler is None:
        raise SystemExit("samples need a C compiler (cc or gcc) to pin the clock")
    shim_dir = root / "target"
    shim_dir.mkdir(exist_ok=True)
    source = shim_dir / "sample-clock.c"
    _SHIM = shim_dir / "sample-clock.so"
    source.write_text(_SHIM_C)
    compiled = subprocess.run(
        [compiler, "-shared", "-fPIC", "-o", str(_SHIM), str(source), "-ldl"],
        cwd=root,
        env=clean_env(),
        text=True,
    )
    if compiled.returncode != 0:
        raise SystemExit(f"{compiler} failed to build the sample clock")
    built = subprocess.run(
        ["cargo", "build", "--quiet", "-p", "deedar-cli", "-p", "deedar-mcp"],
        cwd=root,
        env=clean_env(),
    )
    if built.returncode != 0:
        raise SystemExit("cargo build failed; the samples need the deedar binary")
    debug = root / "target" / "debug"
    _BINS["deedar"] = debug / "deedar"
    _BINS["deedar-mcp"] = debug / "deedar-mcp"
    missing = [str(path) for path in _BINS.values() if not path.is_file()]
    if missing:
        raise SystemExit("missing built binary: " + ", ".join(missing))
    _READY = True


class Session:
    """One scratch store, and the commands a reader would type there."""

    def __init__(self, name: str):
        self.name = name
        self.tmp = tempfile.TemporaryDirectory(prefix="deedar-docs-")
        self.work = Path(self.tmp.name)
        (self.work / "home").mkdir()
        (self.work / "keys").mkdir()
        self.env = clean_env()
        # The parent environment may name a real store. Samples never touch it.
        for key in ("DEEDAR_URL", "DEEDAR_HOST_SIGNING_KEY", "DEEDAR_HOST_KEY"):
            self.env.pop(key, None)
        self.env["HOME"] = str(self.work / "home")
        self.env["XDG_CONFIG_HOME"] = str(self.work / "home" / ".config")
        self.env["XDG_DATA_HOME"] = str(self.work / "home" / ".local" / "share")
        # Byte-for-byte pages. A locale that translated a tool's errors would
        # make the transcript depend on the machine.
        self.env["LC_ALL"] = "C"
        self.env["TZ"] = "UTC"
        self.env["DEEDAR_SAMPLE_UNIX"] = str(SAMPLE_UNIX)
        self.env["LD_PRELOAD"] = str(_SHIM)
        self._pairs = [(display, str(self.work / rel)) for display, rel in DISPLAYS]
        self._pairs.sort(key=lambda pair: len(pair[0]), reverse=True)

    def close(self) -> None:
        self.tmp.cleanup()

    def to_real(self, display: str) -> str:
        text = display
        for shown, real in self._pairs:
            text = text.replace(shown, real)
        return _COMMAND.sub(lambda match: str(_BINS[match.group(0)]), text)

    def to_display(self, text: str) -> str:
        pairs = sorted(self._pairs, key=lambda pair: len(pair[1]), reverse=True)
        for shown, real in pairs:
            text = text.replace(real, shown)
        if self.work.name in text:
            raise SystemExit(
                f"{self.name}: a scratch path leaked into the transcript:\n{text}"
            )
        return text

    def step(self, display: str, expect: int = 0) -> str:
        real = self.to_real(display)
        stripped = real.strip()
        if stripped.startswith("export "):
            body = stripped[len("export ") :]
            key, sep, val = body.partition("=")
            if not sep or not key:
                raise SystemExit(f"{self.name}: bad export: {display}")
            self.env[key] = val
            output = ""
            code = 0
        else:
            proc = subprocess.run(
                ["bash", "-c", real],
                cwd=self.work,
                env=self.env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                encoding="utf-8",
            )
            output = proc.stdout
            if proc.stderr:
                output += proc.stderr
            output = self.to_display(output)
            code = proc.returncode
        if code != expect:
            raise SystemExit(
                f"{self.name}: {display!r} exited {code}, expected {expect}:\n{output}"
            )
        if output and not output.endswith("\n"):
            output += "\n"
        return f"$ {display}\n{output}"

    def server_stays_up(self) -> str:
        """``deedar-mcp`` waits on stdin. The sample is the invocation."""
        proc = subprocess.Popen(
            [str(_BINS["deedar-mcp"])],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=self.env,
            cwd=self.work,
        )
        try:
            proc.wait(timeout=MCP_WAIT_SECONDS)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=MCP_WAIT_SECONDS)
            return "$ deedar-mcp\n"
        err = proc.stderr.read().decode("utf-8", "replace") if proc.stderr else ""
        raise SystemExit(
            f"{self.name}: deedar-mcp exited {proc.returncode} "
            f"before anyone spoke to it:\n{err}"
        )


def last_accession(transcript: str) -> str:
    found = _ACCESSION.findall(transcript)
    if not found:
        raise SystemExit(f"no accession in:\n{transcript}")
    return found[-1]


def log_size(transcript: str) -> str:
    found = _LOG_SIZE.findall(transcript)
    if not found:
        raise SystemExit(f"no log size in:\n{transcript}")
    return found[-1]


def mint(session: Session, note: str = "/tmp/note.rs") -> str:
    """Create the parser-patch deed and return the chunk, accession included."""
    return session.step(f"echo 'fn main() {{}}' > {note}") + session.step(
        'deedar create file --name "the parser patch" '
        f"--path {note} --agent you"
    )


def prelude(session: Session, signing: str) -> str:
    return session.step("export DEEDAR_URL=file:///tmp/deeds") + session.step(
        f"export DEEDAR_HOST_SIGNING_KEY={signing}"
    )


def readme() -> str:
    # Relative paths, as the first minute has always shown them. The store
    # is still a fresh directory, so the root is the one-deed root.
    with_session = Session("readme")
    try:
        text = prelude(with_session, "off")
        text += mint(with_session, "note.rs")
        acc = last_accession(text)
        text += with_session.step(f"deedar evidence {acc}")
        text += with_session.step(f"deedar export --into bag/data/deeds {acc}")
        text += with_session.step("deedar check bag")
        return text
    finally:
        with_session.close()


def getting_started() -> dict[str, str]:
    session = Session("getting-started")
    try:
        parts: dict[str, str] = {}
        parts["gs-store"] = prelude(session, "off") + session.step("deedar list")
        minted = mint(session)
        first = last_accession(minted)
        parts["gs-mint"] = minted
        parts["gs-check"] = session.step(f"deedar evidence {first}") + session.step(
            f"deedar current {first}"
        )
        revised = session.step(
            """echo 'fn main() { println!("parsed"); }' > /tmp/note.rs"""
        ) + session.step(
            'deedar create file --name "the parser patch v2" '
            f"--path /tmp/note.rs --agent you --supersedes {first}"
        )
        second = last_accession(revised)
        parts["gs-supersede"] = revised + session.step(f"deedar current {first}")
        parts["gs-export"] = session.step(
            f"deedar export --into /tmp/bag/data/deeds {second}"
        )
        parts["gs-receive"] = session.step("deedar check /tmp/bag")
        return parts
    finally:
        session.close()


def howto_pipe() -> str:
    session = Session("ht-pipe")
    try:
        text = prelude(session, "off")
        text += mint(session)
        first = last_accession(text)
        revised = session.step(
            """echo 'fn main() { println!("parsed"); }' > /tmp/note.rs"""
        ) + session.step(
            'deedar create file --name "the parser patch v2" '
            f"--path /tmp/note.rs --agent you --supersedes {first}"
        )
        second = last_accession(revised)
        text += revised
        both = f"{first} {second}"
        # Stale on purpose: current exits non-zero when a citation has moved.
        text += session.step(
            f"printf '%s\\n' {both} | deedar current -",
            expect=1,
        )
        text += session.step(f"printf '%s\\n' {both} | deedar evidence -")
        return text
    finally:
        session.close()


def howto_sign() -> str:
    session = Session("ht-sign")
    try:
        text = prelude(session, "/tmp/host.key")
        text += session.step(f"head -c {SEED_BYTES} /dev/zero > /tmp/host.key")
        text += session.step("chmod 600 /tmp/host.key")
        text += mint(session)
        text += session.step("deedar host")
        text += session.step("deedar log head")
        acc = last_accession(text)
        text += session.step(f"deedar export --into /tmp/bag/data/deeds {acc}")
        text += session.step(
            "printf '%s\\n' 'sample manifest' > /tmp/bag/manifest-sha256.txt"
        )
        text += session.step("deedar vouch sign /tmp/bag/manifest-sha256.txt")
        text += session.step("deedar vouch check /tmp/bag/manifest-sha256.txt")
        return text
    finally:
        session.close()


def howto_keep() -> str:
    session = Session("ht-keep")
    try:
        text = prelude(session, "off")
        text += mint(session)
        first = last_accession(text)
        text += session.step(f"deedar export --into /tmp/bag/data/deeds {first}")
        kept = session.step("deedar check /tmp/bag --keep /tmp/sender.head")
        text += kept
        text += session.step("echo second > /tmp/note2.rs")
        text += session.step(
            'deedar create file --name "the notes" --path /tmp/note2.rs --agent you'
        )
        second = last_accession(text)
        text += session.step(f"deedar export --into /tmp/bag2/data/deeds {second}")
        size = log_size(kept)
        text += session.step(f"deedar log bridge {size} > /tmp/bag2/bridge.txt")
        text += session.step("deedar check /tmp/bag2 --since /tmp/sender.head")
        return text
    finally:
        session.close()


def howto_import() -> str:
    session = Session("ht-import")
    try:
        text = prelude(session, "off")
        text += mint(session)
        acc = last_accession(text)
        text += session.step(f"deedar export --into /tmp/bag/data/deeds {acc}")
        text += session.step("export DEEDAR_URL=file:///tmp/recv")
        text += session.step("deedar import /tmp/bag", expect=1)
        text += session.step("deedar import /tmp/bag --allow-unsigned-head")
        text += session.step("deedar import /tmp/bag --allow-unsigned-head")
        return text
    finally:
        session.close()


def howto_one(name: str, commands: list[tuple[str, int]]) -> str:
    """A fresh unsigned store, one deed, then the commands under test."""
    session = Session(name)
    try:
        text = prelude(session, "off")
        text += mint(session)
        acc = last_accession(text)
        for command, expect in commands:
            text += session.step(command.format(acc=acc), expect=expect)
        return text
    finally:
        session.close()


def howto_attest() -> str:
    """The layout a reader writes by hand, using the key the binary reports."""
    session = Session("ht-attest")
    try:
        session.step("export DEEDAR_URL=file:///tmp/deeds")
        session.step("export DEEDAR_HOST_SIGNING_KEY=/tmp/host.key")
        session.step(f"head -c {SEED_BYTES} /dev/zero > /tmp/host.key")
        reported = session.step("deedar host")
        found = _SIGNER.search(reported)
        if not found:
            raise SystemExit(f"ht-attest: deedar host named no key:\n{reported}")
        return f"1\nattestation = required\nsigner = {found.group(0)}\n"
    finally:
        session.close()


def howto_mcp() -> str:
    session = Session("ht-mcp")
    try:
        return session.server_stays_up()
    finally:
        session.close()


def build_all() -> dict[str, str]:
    ensure_tools()
    parts = {"readme": readme()}
    parts.update(getting_started())
    parts["ht-pipe"] = howto_pipe()
    parts["ht-sign"] = howto_sign()
    parts["ht-keep"] = howto_keep()
    parts["ht-import"] = howto_import()
    parts["ht-audit"] = howto_one("ht-audit", [("deedar log audit", 0)])
    parts["ht-backfill"] = howto_one("ht-backfill", [("deedar log backfill", 0)])
    parts["ht-attest"] = howto_attest()
    # delete exits 0. The audit after it exits 1: the deed is gone and the
    # log entry is not, which is the gap the page is showing.
    parts["ht-delete"] = howto_one(
        "ht-delete",
        [("deedar delete {acc}", 0), ("deedar log audit", 1)],
    )
    parts["ht-mcp"] = howto_mcp()
    missing = [name for name in FENCES if name not in parts]
    if missing:
        raise SystemExit("samples not built: " + ", ".join(missing))
    times = re.findall(r"(?m)^time=(\d+)\s*$", "\n".join(parts.values()))
    if not times or any(stamp != str(SAMPLE_UNIX) for stamp in times):
        raise SystemExit(f"sample clock did not stick: {times}")
    return parts


def fence(name: str, body: str) -> str:
    if not body.endswith("\n"):
        body += "\n"
    kind = FENCES[name]
    if kind == "md":
        return f"```console\n{body}```\n"
    if kind == "org":
        return f"#+begin_src console\n{body}#+end_src\n"
    if kind == "text":
        return f"#+begin_src text\n{body}#+end_src\n"
    raise SystemExit(f"unknown fence {kind}")


def transcript(name: str) -> str:
    """The fenced sample ``name``, built twice and kept only if both agree."""
    global _CACHED
    if name not in FENCES:
        raise SystemExit(f"unknown sample {name}")
    if _CACHED is None:
        first = build_all()
        second = build_all()
        if first != second:
            for key in first:
                if first[key] != second.get(key):
                    raise SystemExit(f"sample {key} changed between two runs")
            raise SystemExit("sample set changed between two runs")
        _CACHED = first
    return fence(name, _CACHED[name])


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[1] == "--check":
        transcript("readme")
        return 0
    if len(argv) == 2 and argv[1] in FENCES:
        sys.stdout.write(transcript(argv[1]))
        return 0
    names = " ".join(sorted(FENCES))
    print(f"usage: samples.py --check | {names}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
