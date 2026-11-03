//! What a seat is told by `deedar-mcp`.
//!
//! The server is started on a scratch store. Every tool is listed and called.
//! A good accession comes back as the fields that tool publishes. An accession
//! the store does not hold, and a `sha256:` that names more than one deed,
//! come back as the error text rather than a row.

use std::path::{Path, PathBuf};

use deed::{Body, Deed, DeedId, Source};
use deedar::{deed_digest, Client, CreateRequest};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{Peer, RoleClient, ServiceError, ServiceExt};
use serde_json::{json, Map, Value};

/// The accessions this sitting creates. `list` returns them in accession
/// order, which is how the store orders a shelf, not by the time they were
/// written.
const ACCESSIONS: &[&str] = &[
    "deed-file-one",
    "deed-file-solo",
    "deed-file-two",
    "deed-quote-made",
    "deed-quote-src",
    "deed-quote-v1",
    "deed-quote-v2",
];

/// Tools the server publishes, in list order once sorted. A new tool has to
/// be called below or this fails.
const TOOLS: &[&str] = &[
    "deedar_check",
    "deedar_current",
    "deedar_evidence",
    "deedar_export",
    "deedar_get",
    "deedar_list",
    "deedar_log_audit",
    "deedar_log_bridge",
    "deedar_log_head",
    "deedar_trail",
];

const EDITION: &str = "https://example.invalid/edition";
const EXCERPT: &str = "what the seat quoted";
const MISSING: &str = "deed-quote-missing";

/// Envelope of one deed, as `deedar_get` publishes it.
const DEED_FIELDS: &[&str] = &[
    "agent", "body", "digest", "id", "kind", "name", "paths", "sources",
];

const HEAD_FIELDS: &[&str] = &["root", "size"];

const AUDIT_FIELDS: &[&str] = &[
    "backfill_settles_it",
    "clean",
    "logged",
    "missing",
    "unlogged",
];

const HANDOVER_FIELDS: &[&str] = &[
    "bridged_from",
    "head",
    "head_accepted",
    "head_signer",
    "proven",
];

struct Scratch {
    home: tempfile::TempDir,
    url: String,
}

impl Scratch {
    fn path(&self, name: &str) -> PathBuf {
        self.home.path().join(name)
    }
}

fn signing_off() {
    // The host key is process-wide. This binary has one test, and the
    // handover below is unsigned, which is the answer `deedar_check` gives
    // when the bag carries no signature.
    unsafe {
        std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off");
        std::env::remove_var("DEEDAR_HOST_KEY");
    }
}

fn scratch() -> Scratch {
    signing_off();
    let home = tempfile::tempdir().expect("scratch");
    let root = home.path().join("store");
    let url = format!("file://{}", root.display());
    let mut client = Client::open(&url).expect("open");

    // Every quote covers its whole excerpt, from the first byte.
    quote(
        &mut client,
        "deed-quote-src",
        "the edition",
        "the edition's words",
        Vec::new(),
        None,
    );
    quote(
        &mut client,
        "deed-quote-made",
        "the note",
        EXCERPT,
        vec![Source::Deed(DeedId::parse("deed-quote-src").expect("src"))],
        None,
    );
    quote(
        &mut client,
        "deed-quote-v1",
        "first take",
        "the first take",
        Vec::new(),
        None,
    );
    quote(
        &mut client,
        "deed-quote-v2",
        "second take",
        "the second take",
        Vec::new(),
        Some("deed-quote-v1"),
    );

    let pages = home.path().join("pages");
    std::fs::create_dir(&pages).expect("pages");
    let shared_a = pages.join("a.txt");
    let shared_b = pages.join("b.txt");
    let solo = pages.join("solo.txt");
    // Two files, one hash: a `sha256:` of the product then names both deeds.
    std::fs::write(&shared_a, "the same page\n").expect("write");
    std::fs::write(&shared_b, "the same page\n").expect("write");
    std::fs::write(&solo, "one page\n").expect("write");
    file(&mut client, "deed-file-one", &shared_a);
    file(&mut client, "deed-file-two", &shared_b);
    file(&mut client, "deed-file-solo", &solo);

    Scratch { home, url }
}

fn quote(
    client: &mut Client,
    id: &str,
    name: &str,
    excerpt: &str,
    sources: Vec<Source>,
    supersedes: Option<&str>,
) {
    client
        .create(CreateRequest {
            id: Some(DeedId::parse(id).expect(id)),
            name: name.into(),
            sources,
            agent_id: "seat".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::Quote {
                edition: EDITION.into(),
                start: 0,
                end: excerpt.len() as u64,
                excerpt: excerpt.into(),
                urls: vec![EDITION.into()],
            },
            supersedes: supersedes.map(|prior| DeedId::parse(prior).expect(prior)),
        })
        .unwrap_or_else(|e| panic!("create {id}: {e}"));
}

fn file(client: &mut Client, id: &str, path: &Path) {
    client
        .create(CreateRequest {
            id: Some(DeedId::parse(id).expect(id)),
            name: id.into(),
            sources: Vec::new(),
            agent_id: "seat".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::File {
                path: path.to_path_buf(),
                media_type: Some("text/plain".into()),
            },
            supersedes: None,
        })
        .unwrap_or_else(|e| panic!("create {id}: {e}"));
}

fn store(url: &str) -> Client {
    Client::open(url).expect("store")
}

fn deed(client: &mut Client, id: &str) -> Deed {
    let id = DeedId::parse(id).expect(id);
    client.get(&id).unwrap_or_else(|e| panic!("get {id}: {e}"))
}

fn source_line(source: &Source) -> String {
    match source {
        Source::Deed(id) => format!("deed:{id}"),
        Source::Path(path) => format!("path:{}", path.display()),
        Source::Url(url) => format!("url:{url}"),
    }
}

async fn serve(url: &str) -> rmcp::service::RunningService<RoleClient, ()> {
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_deedar-mcp")).configure(|cmd| {
            cmd.env("DEEDAR_URL", url)
                .env("DEEDAR_HOST_SIGNING_KEY", "off")
                .env_remove("DEEDAR_HOST_KEY");
        }),
    )
    .expect("spawn deedar-mcp");
    ().serve(transport).await.expect("deedar-mcp speaks")
}

async fn call(
    peer: &Peer<RoleClient>,
    name: &str,
    arguments: Option<Value>,
) -> Result<CallToolResult, ServiceError> {
    let mut params = CallToolRequestParams::new(name.to_string());
    if let Some(arguments) = arguments {
        match arguments {
            Value::Object(map) => params = params.with_arguments(map),
            other => panic!("{name}: arguments are an object, not {other}"),
        }
    }
    peer.call_tool(params).await
}

fn text_of(result: &CallToolResult) -> &str {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0]
        .as_text()
        .unwrap_or_else(|| panic!("{result:?}"))
        .text
        .as_str()
}

/// The structured answer, which is also the text a seat reads.
fn payload(result: CallToolResult) -> Value {
    assert_eq!(result.is_error, Some(false), "{result:?}");
    let value = result
        .structured_content
        .clone()
        .unwrap_or_else(|| panic!("no structured content: {result:?}"));
    let from_text: Value =
        serde_json::from_str(text_of(&result)).unwrap_or_else(|e| panic!("{e}: {result:?}"));
    assert_eq!(
        from_text, value,
        "the text a seat reads differs from structuredContent"
    );
    value
}

async fn ask(peer: &Peer<RoleClient>, name: &str, arguments: Option<Value>) -> Value {
    let result = call(peer, name, arguments)
        .await
        .unwrap_or_else(|e| panic!("{name} failed: {e}"));
    payload(result)
}

async fn told(peer: &Peer<RoleClient>, name: &str, arguments: Value) -> String {
    match call(peer, name, Some(arguments)).await {
        Ok(result) => {
            assert_eq!(result.is_error, Some(true), "{name} succeeded: {result:?}");
            text_of(&result).to_string()
        }
        Err(ServiceError::McpError(data)) => data.message.into_owned(),
        Err(other) => panic!("{name}: {other}"),
    }
}

fn fields<'a>(value: &'a Value, expected: &[&str]) -> &'a Map<String, Value> {
    let map = value
        .as_object()
        .unwrap_or_else(|| panic!("a row is an object, got {value}"));
    let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, expected, "{value}");
    map
}

fn string_at<'a>(row: &'a Map<String, Value>, key: &str) -> &'a str {
    row[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} is not a string: {}", row[key]))
}

fn number_at(row: &Map<String, Value>, key: &str) -> u64 {
    row[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} is not a number: {}", row[key]))
}

fn flag_at(row: &Map<String, Value>, key: &str) -> bool {
    row[key]
        .as_bool()
        .unwrap_or_else(|| panic!("{key} is not a bool: {}", row[key]))
}

fn assert_deed(value: &Value, deed: &Deed) {
    let row = fields(value, DEED_FIELDS);
    assert_eq!(string_at(row, "id"), deed.id.as_str());
    assert_eq!(string_at(row, "kind"), deed.kind.token());
    assert_eq!(string_at(row, "name"), deed.name);
    assert_eq!(string_at(row, "agent"), deed.produced_by.agent_id);
    assert_eq!(string_at(row, "digest"), deed_digest(deed));
    let paths: Vec<String> = deed.paths.iter().map(|p| p.display().to_string()).collect();
    assert_eq!(row["paths"], json!(paths));
    let sources: Vec<String> = deed.sources.iter().map(source_line).collect();
    assert_eq!(row["sources"], json!(sources));
    assert_eq!(row["body"], serde_json::to_value(&deed.body).expect("body"));
}

fn assert_quote(body: &Value) {
    let root = fields(body, &["quote"]);
    let quote = fields(
        &root["quote"],
        &["edition", "end", "excerpt", "start", "urls"],
    );
    assert_eq!(string_at(quote, "edition"), EDITION);
    assert_eq!(string_at(quote, "excerpt"), EXCERPT);
    assert_eq!(number_at(quote, "start"), 0);
    assert_eq!(number_at(quote, "end"), EXCERPT.len() as u64);
    assert_eq!(quote["urls"], json!([EDITION]));
}

fn assert_file(body: &Value, path: &str) {
    let root = fields(body, &["file"]);
    // `rename_all` on the enum renames variants, not these fields, so a seat
    // sees `media_type` rather than `mediaType`.
    let file = fields(&root["file"], &["media_type", "path"]);
    assert_eq!(string_at(file, "media_type"), "text/plain");
    assert_eq!(string_at(file, "path"), path);
}

fn file_count(dir: &Path) -> usize {
    let mut n = 0usize;
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            n += file_count(&path);
        } else if path.is_file() {
            n += 1;
        }
    }
    n
}

fn id_args(id: &str) -> Value {
    json!({ "id": id })
}

/// A seat lists the tools and calls each one: the row it publishes, and the
/// error for an accession that is not in the store or a `sha256:` that names
/// two deeds.
#[tokio::test]
async fn every_tool_answers_with_its_fields_and_names_a_bad_accession() {
    let scratch = scratch();
    let mut oracle = store(&scratch.url);
    let deeds = oracle.list().expect("list");
    assert_eq!(
        deeds.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ACCESSIONS
    );
    let made = deed(&mut oracle, "deed-quote-made");
    let solo = deed(&mut oracle, "deed-file-solo");
    let shared = deed(&mut oracle, "deed-file-one").paths[0]
        .display()
        .to_string();
    let shared_by = deeds
        .iter()
        .filter(|d| d.paths.iter().any(|p| p.display().to_string() == shared))
        .count();
    assert!(shared_by > 1, "the shared page names one deed");
    let solo_path = solo.paths[0].display().to_string();
    let evidence = {
        let ev = oracle.evidence(&made.id).expect("evidence");
        format!(
            "id={} producedBy={} {} time={}\n",
            ev.deed_id,
            ev.produced_by.agent_id,
            ev.produced_by.activity_id.as_deref().unwrap_or("-"),
            ev.unix_time
        )
    };
    // One entry is the smallest head a consistency proof can start from.
    // Zero is not a tree, so `bridge` refuses it; one is a head this log had.
    let earlier = 1usize;
    let bridge = oracle.bridge(earlier).expect("bridge").render();
    let head = oracle.log_head().expect("head");

    let client = serve(&scratch.url).await;
    let peer = client.peer();

    let tools = peer.list_all_tools().await.expect("tools/list");
    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, TOOLS);
    for tool in &tools {
        assert!(
            tool.description.as_ref().is_some_and(|d| !d.is_empty()),
            "{} carries no description",
            tool.name
        );
    }

    let got = ask(peer, "deedar_get", Some(id_args(made.id.as_str()))).await;
    assert_deed(&got, &made);
    assert_quote(&got["body"]);
    let by_digest = ask(peer, "deedar_get", Some(id_args(&deed_digest(&made)))).await;
    assert_eq!(
        string_at(fields(&by_digest, DEED_FIELDS), "id"),
        made.id.as_str()
    );
    let by_path = ask(peer, "deedar_get", Some(id_args(&solo_path))).await;
    assert_deed(&by_path, &solo);
    assert_file(&by_path["body"], &solo_path);

    let listed = ask(peer, "deedar_list", None).await;
    let listed = listed
        .as_array()
        .unwrap_or_else(|| panic!("list is an array, got {listed}"));
    assert_eq!(listed.len(), deeds.len());
    for (row, deed) in listed.iter().zip(&deeds) {
        assert_deed(row, deed);
    }

    let trail = ask(peer, "deedar_trail", Some(id_args("deed-quote-made"))).await;
    let trail = trail
        .as_array()
        .unwrap_or_else(|| panic!("trail is an array, got {trail}"));
    let trail_ids: Vec<&str> = trail
        .iter()
        .map(|row| {
            let row = fields(row, DEED_FIELDS);
            row["id"].as_str().expect("id")
        })
        .collect();
    assert_eq!(trail_ids, ["deed-quote-made", "deed-quote-src"]);
    assert_deed(&trail[0], &made);
    assert_deed(&trail[1], &deed(&mut oracle, "deed-quote-src"));

    let tip = deed(&mut oracle, "deed-quote-v2");
    let current = ask(peer, "deedar_current", Some(id_args("deed-quote-v1"))).await;
    assert_deed(&current, &tip);
    let still = ask(peer, "deedar_current", Some(id_args("deed-quote-v2"))).await;
    assert_eq!(
        string_at(fields(&still, DEED_FIELDS), "id"),
        "deed-quote-v2"
    );

    let said = call(peer, "deedar_evidence", Some(id_args(made.id.as_str())))
        .await
        .expect("evidence");
    assert_eq!(said.is_error, Some(false), "{said:?}");
    assert!(said.structured_content.is_none(), "{said:?}");
    assert_eq!(text_of(&said), evidence);

    let logged = ask(peer, "deedar_log_head", None).await;
    let logged_row = fields(&logged, HEAD_FIELDS);
    assert_eq!(number_at(logged_row, "size"), head.size as u64);
    assert_eq!(string_at(logged_row, "root"), head.root);

    let audit = ask(peer, "deedar_log_audit", None).await;
    let audit = fields(&audit, AUDIT_FIELDS);
    assert!(flag_at(audit, "clean"));
    assert_eq!(number_at(audit, "logged"), head.size as u64);
    assert_eq!(audit["missing"], json!([]));
    assert_eq!(audit["unlogged"], json!([]));
    assert!(!flag_at(audit, "backfill_settles_it"));

    let bag = scratch.path("bag");
    let exported = call(
        peer,
        "deedar_export",
        Some(json!({
            "into": bag.display().to_string(),
            "accessions": ["deed-quote-src"],
        })),
    )
    .await
    .expect("export");
    assert_eq!(exported.is_error, Some(false), "{exported:?}");
    assert!(exported.structured_content.is_none(), "{exported:?}");
    let wrote = file_count(&bag);
    assert_eq!(
        text_of(&exported),
        format!("exported 1 deeds, {wrote} files\n")
    );
    assert!(bag.join("deed-quote-src").join("deed.bin").is_file());

    let checked = ask(
        peer,
        "deedar_check",
        Some(json!({ "dir": bag.display().to_string() })),
    )
    .await;
    let checked_row = fields(&checked, HANDOVER_FIELDS);
    assert_eq!(checked_row["proven"], json!(["deed-quote-src"]));
    let checked_head = fields(&checked_row["head"], HEAD_FIELDS);
    assert_eq!(number_at(checked_head, "size"), head.size as u64);
    assert_eq!(string_at(checked_head, "root"), head.root);
    assert_eq!(checked_row["head_signer"], Value::Null);
    assert!(!flag_at(checked_row, "head_accepted"));
    assert_eq!(checked_row["bridged_from"], Value::Null);

    let bridged = call(peer, "deedar_log_bridge", Some(json!({ "from": earlier })))
        .await
        .expect("bridge");
    assert_eq!(bridged.is_error, Some(false), "{bridged:?}");
    assert!(bridged.structured_content.is_none(), "{bridged:?}");
    let bridged = text_of(&bridged).to_string();
    assert_eq!(bridged, bridge);
    let mut lines = bridged.lines();
    let from = lines.next().expect("from").to_string();
    let to = lines.next().expect("to").to_string();
    let rest: Vec<&str> = lines.collect();
    let from = from.strip_prefix("from=").expect("from=");
    let to = to.strip_prefix("to=").expect("to=");
    let (from_size, _) = from.split_once(' ').expect("from size");
    let (to_size, to_root) = to.split_once(' ').expect("to size");
    assert_eq!(from_size, earlier.to_string());
    assert_eq!(to_size, head.size.to_string());
    assert_eq!(to_root, head.root);
    assert!(head.size > earlier);
    assert!(
        !rest.is_empty() && rest.iter().all(|line| line.starts_with("path=")),
        "{bridged}"
    );

    let unknown = format!("deed not found: {MISSING}");
    let ambiguous = format!("invalid deed id: {shared} names {shared_by} deeds");
    for name in [
        "deedar_get",
        "deedar_trail",
        "deedar_current",
        "deedar_evidence",
    ] {
        assert_eq!(told(peer, name, id_args(MISSING)).await, unknown, "{name}");
        assert_eq!(
            told(peer, name, id_args(&shared)).await,
            ambiguous,
            "{name}"
        );
    }

    let refused = scratch.path("refused");
    assert_eq!(
        told(
            peer,
            "deedar_export",
            json!({
                "into": refused.display().to_string(),
                "accessions": [MISSING],
            })
        )
        .await,
        format!("{MISSING}: {unknown}")
    );
    assert_eq!(
        told(
            peer,
            "deedar_export",
            json!({
                "into": refused.display().to_string(),
                "accessions": [shared],
            })
        )
        .await,
        format!("{shared}: {ambiguous}")
    );

    // A path that is not a directory is not a handover. An empty directory
    // is a handover that carried nothing, which is a different answer.
    let absent = scratch.path("no-such-bag");
    assert_eq!(
        told(
            peer,
            "deedar_check",
            json!({ "dir": absent.display().to_string() })
        )
        .await,
        format!(
            "evidence: {}: no deeds here and none under data/deeds",
            absent.display()
        )
    );

    // One past the head is not a size this log ever published.
    let past = head.size + 1;
    assert_eq!(
        told(peer, "deedar_log_bridge", json!({ "from": past })).await,
        format!(
            "evidence: a log of {} entries cannot bridge from {past}",
            head.size
        )
    );

    client.cancel().await.expect("stop");
}
