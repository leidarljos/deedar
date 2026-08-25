# deeder

A unit of work produces something. deeder records it as a **deed**:
here is the work, this is its identity, take it. The next unit
opens that deed by `id`.

deeder is the writer. A deed is the noun. `quote`, `patch`, `set`
and the rest are kinds — handlers and bodies — not the object. The
object is always a deed (`deed-quote-rfc2094-nll`).

`DEEDER_URL` is the contract. There is no daemon. Clients speak
`schema/deeder.capnp`: `create`, `get`, `list`, `trail`, `evidence`,
`delete`. Next work takes deed ids as `--input`; they become
`sources`.

```
work happens
     |
deeder.create --> deed + evidence
                       |
        get / list ----+--> a viewer paints face
        next work -----+--> --input ids --> trail
        evidence ------+--> check the deed and its bytes
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

A seat that already has claimdag, packset, or vissue can *cite* a
deed id. Those products keep their own writers.

## Envelope

A deed is `id`, `kind`, `paths`, `sources`, `producedBy`, `grants`,
plus a typed `body` and a `face`.

| Field | Job |
|---|---|
| `id` | Accession (`deed-<kind>-<slug>`). Frozen after create (Engelbart Journal). |
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
| `file` | Write-once address and media type. Face follows that media type. Photo `file` carries Content Credentials (C2PA) when the file leaves this store. |
| `set` | Title and ordered member addresses (Adobe XMP). Photo `set` carries Content Credentials when the set leaves this store. |
| `quote` | A range in a frozen edition, plus the source URLs (Nelson transclusion). |
| `patch` | Tree, diffs, and the functionaries who signed the step (in-toto). |
| `mailDraft` | Message-ID, In-Reply-To, subject, and the draft’s write-once path (RFC 5322). |
| `clip` | Source addresses, in/out points, duration, and the rendered path. Content Credentials when the clip leaves this store. Face is `waveform`. |
| `page` | URL and snapshot address. |
| `form` | Blank identity, field values, and the signed path. |
| `table` | Named measures. |
| `procedure` | Ordered steps and which are done. |
| `event` | When, where, who. |

## Store

- Mint `id` as an accession. After create the deed is frozen. A
  later take mints a new deed and records the old accession in
  `sources`. Delete writes a tombstone.
- Put product bytes in write-once storage. Name those addresses in
  `paths` and in every kind body that points at a file.
- deeder issues **evidence** at create: a keyed hash over the
  canonical deed, its write-once addresses, `producedBy`, `grants`,
  and a clock the create caller does not supply. `deeder evidence
  <id>` is the next sitting's check: not tombstoned, each `sha256:`
  path still hashes, keyed hash matches.
- When a host policy process is the writer, that process issues the
  evidence. When a deed is copied off this machine, bind that
  signature to an RFC 3161 timestamp.

## Cite

Other tools can name a deed. They do not store the product.

```
unit of work
  |
  +-- deeder.create --> deed id --> next unit --input
  +-- claimdag complete can name the id
  +-- packset Remember: can cite the id
  +-- vissue heading can cite the id
```

[claimdag](https://github.com/haoZeke/claimdag) is CAS claim and
complete on a DAG. `summary` is the only open text on `work.bin`.
`complete` can name a deed id. Kind, bytes, and trail stay on the
deed.

## Language

The record library and the writer are ordinary crates. C is a fit
when evidence and the write-once store should live in the same
process as a C policy daemon.

## Work

- Host-issued evidence when a policy process is the writer.
- Content Credentials when a `clip`, photo `set`, or photo `file`
  leaves this store.
- RFC 3161 timestamp when a deed is copied off this machine.
