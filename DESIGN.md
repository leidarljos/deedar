# deedar

A unit of work produces something. deedar records it as a **deed**:
here is the work, this is its identity, take it. The next unit
opens that deed by `id`.

deedar is the writer. A deed is the noun. `quote`, `patch`, `set`
and the rest are kinds — handlers and bodies — not the object. The
object is always a deed (`deed-quote-rfc2094-nll`).

`DEEDAR_URL` is the contract. There is no daemon. Clients speak
`schema/deedar.capnp`: `create`, `get`, `list`, `trail`, `evidence`,
`delete`, `leave`, `timestamp`, `current`. `get` accepts a slug or a
unique `sha256:` of the deed or of one product path. Next work
takes deed ids as `--input`; they become `sources`. `deedar migrate`
(and `FsStore::open`) write `{store}/layout` when that file is
missing.

```
work happens
     |
deedar.create --> deed + evidence
                       |
        get / list ----+--> a viewer paints face
        next work -----+--> --input ids --> trail
        evidence ------+--> check the deed, its bytes, and sources
        leave ---------+--> dest/{id} + hash sidecar
        timestamp -----+--> RFC 3161 TimeStampReq over evidence
        current -------+--> follow --supersedes to the tip
        migrate -------+--> write {store}/layout when missing
```

Any client that can open a `file://` URL and speak those verbs can
use it: a command line, a harness after a run, a review bot, a
mailer, a CI job.

## Uses

- After an agent run, name the patch or file so the next run takes
  `--input` instead of rereading the transcript.
- Bind a research excerpt to its edition and URLs; the next sitting
  opens `deed-quote-rfc2094-nll`.
- Name a photo select (`set`) or a mix (`clip`); the next sitting
  opens the membership or the in/out points.
- Keep a mail draft’s Message-ID so the next sitting can send on
  that thread.
- Hand a CI job a deed id for the artifact it must test or publish.

claimdag, packset, or vissue can cite a deed id. Those products
keep their own writers.

## Envelope

A deed is `id`, `kind`, `paths`, `sources`, `producedBy`, `grants`,
plus a typed `body` and a `face`.

| Field | Job |
|---|---|
| `id` | Accession (`deed-<kind>-<slug>`). Frozen after create (Engelbart Journal). `get` also accepts a unique `sha256:` of the canonical deed or of one product path. |
| `kind` | Handler for what was made, with `face` (Plan 9 plumber). |
| `paths` | Write-once addresses of the product bytes (Plan 9 Venti). |
| `sources` | Prior deed ids, paths, or URLs. `trail` walks that graph (Bush). |
| `producedBy` | Agent plus optional activity id (W3C PROV-DM Entity, Activity, Agent). |
| `grants` | Hosts and paths allowed while the work ran. |
| `body` | Kind-hard fields the next unit needs. |
| `face` | How a viewer looks at it: `syntax`, `sheet`, `letter`, `waveform`, `document`. Face follows media type and body. |

`producedBy.activityId` is whatever the caller uses for the live
assignment. When that caller is
[claimdag](https://github.com/haoZeke/claimdag), it is a `WorkId`.

## Kinds

Each kind is a handler. The body is what the next unit needs.

| Kind | Body the next unit needs |
|---|---|
| `file` | Write-once address and media type. Face follows that media type. |
| `set` | Title and ordered member addresses. |
| `quote` | A range in a frozen edition, plus the source URLs (Nelson transclusion). |
| `patch` | Tree, diffs, and the functionaries who signed the step. |
| `mailDraft` | Message-ID, In-Reply-To, subject, and the draft’s write-once path (RFC 5322). |
| `clip` | Source addresses, in/out points, duration, and the rendered path. Face is `waveform`. |
| `page` | URL and snapshot address. |
| `form` | Blank identity, field values, and the signed path. |
| `table` | Named measures. |
| `procedure` | Ordered steps and which are done. |
| `event` | When, where, who. |

## Store

- Mint `id` as an accession. After create the deed is frozen. A
  later take mints a new deed and records the old accession in
  `sources`. `--supersedes <id>` also writes `{new}.supersedes` and
  `{prior}.successor` sidecar files; `deedar current <id>` walks
  those to the tip. `get` of the old id still returns that frozen
  deed. Delete writes a tombstone.
- `{store}/layout` is `1` after `open` or `deedar migrate`. A store
  without that file still opens; `open` writes the file.
- Put product bytes in write-once storage. Name those addresses in
  `paths` and in every kind body that points at a file.
- deedar issues **evidence** at create: a keyed hash over the
  canonical deed (paths, `producedBy`, `grants` included) and a
  clock the create caller does not supply. `deedar evidence <id>`
  checks: not tombstoned, each `sha256:` path still hashes, keyed
  hash matches, and every deed `sources` names can itself be
  evidenced. A configured host key requires a host sidecar and a
  different keyed-hash construction than `writer.key`. Create fails
  if a body path is not a readable file.
- `leave` copies write-once product bytes for `file`, `set`, and
  `clip` into `dest/{id}/` and writes `manifest.json`
  (`claim_generator=deedar`, deed id, kind, hash of the
  concatenated leaving bytes). The same sitting writes an RFC 3161
  TimeStampReq over the evidence bytes (`{id}.tsq` when
  `DEEDAR_TSA` is unset; `{id}.tsr` when an authority replies). The
  frozen deed and keyed-hash evidence stay put. Other kinds refuse.
  `get` and `evidence` do not require a manifest.

## Cite

Other tools can name a deed. They do not store the product.

```
unit of work
  |
  +-- deedar.create --> deed id --> next unit --input
  +-- claimdag complete can name the id
  +-- packset Remember: can cite the id
  +-- vissue heading can cite the id
```

[claimdag](https://github.com/haoZeke/claimdag) is CAS claim and
complete on a DAG. `summary` is the only open text on `work.bin`.
`complete` can name a deed id. Kind, bytes, and trail stay on the
deed.

## Language

The record library and the writer are ordinary crates.

## Work

- Host-issued evidence when a policy process is the writer.
