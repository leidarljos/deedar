//! Which store `deedar-mcp` opens when `DEEDAR_URL` does not name one.
//!
//! The command line uses `$XDG_DATA_HOME/deedar/store` before
//! `~/.local/share/deedar/store`. A deed the command line stored there has to
//! come back from `deedar_list`.

use std::path::Path;
use std::process::Stdio;

use deed::{Body, DeedId};
use deedar::{Client, CreateRequest};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{ServiceError, ServiceExt};
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::ChildStderr;

const ACCESSION: &str = "deed-quote-seat";

/// A scratch home whose data directory holds a store with one deed.
fn seat_home() -> tempfile::TempDir {
    std::env::set_var("DEEDAR_HOST_SIGNING_KEY", "off");
    std::env::remove_var("DEEDAR_HOST_KEY");
    let home = tempfile::tempdir().expect("scratch");
    let store = home.path().join("xdg/deedar/store");
    let mut client = Client::open(&format!("file://{}", store.display())).expect("open");
    client
        .create(CreateRequest {
            id: Some(DeedId::parse(ACCESSION).expect("id")),
            name: "seat".into(),
            sources: Vec::new(),
            agent_id: "seat".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::Quote {
                edition: "https://example.invalid/edition".into(),
                start: 0,
                end: 4,
                excerpt: "said".into(),
                urls: vec!["https://example.invalid/edition".into()],
            },
            supersedes: None,
        })
        .expect("create");
    home
}

/// The accessions `deedar_list` returns from a server started in `home`.
async fn listed(home: &Path, deedar_url: Option<&str>) -> Vec<String> {
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_deedar-mcp")).configure(|cmd| {
            cmd.env("HOME", home.join("home"))
                .env("XDG_DATA_HOME", home.join("xdg"))
                .env("DEEDAR_HOST_SIGNING_KEY", "off")
                .env_remove("DEEDAR_HOST_KEY")
                .env_remove("DEEDAR_URL");
            if let Some(url) = deedar_url {
                cmd.env("DEEDAR_URL", url);
            }
        }),
    )
    .expect("spawn deedar-mcp");
    let client = ().serve(transport).await.expect("deedar-mcp speaks");
    let result = client
        .peer()
        .call_tool(CallToolRequestParams::new("deedar_list".to_string()))
        .await
        .expect("deedar_list");
    client.cancel().await.expect("stop");
    let rows = result.structured_content.expect("structured rows");
    rows.as_array()
        .expect("a list")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_string())
        .collect()
}

#[tokio::test]
async fn the_seat_store_under_xdg_data_home_is_the_one_listed() {
    let home = seat_home();
    assert_eq!(listed(home.path(), None).await, vec![ACCESSION.to_string()]);
    assert!(
        !home.path().join("home/.local/share/deedar").exists(),
        "a second store was made under HOME"
    );
}

#[tokio::test]
async fn an_empty_deedar_url_is_unset() {
    let home = seat_home();
    assert_eq!(
        listed(home.path(), Some("")).await,
        vec![ACCESSION.to_string()]
    );
}

/// The command line's refusal, without the `deedar:` prefix `main` adds.
fn seat_refusal(path: &Path) -> String {
    let path = path.display();
    format!(
        "DEEDAR_URL is unset and there is no store at {path}; \
         `export DEEDAR_URL=file://{path}` or `deedar --url file://{path}`"
    )
}

fn created_line(path: &Path) -> String {
    format!("deedar: created the store at {}", path.display())
}

struct Running {
    client: rmcp::service::RunningService<rmcp::RoleClient, ()>,
    stderr: ChildStderr,
}

/// `deedar-mcp` on an empty scratch seat: `XDG_DATA_HOME` set, no `DEEDAR_URL`.
async fn seat_server(xdg: &Path, home: &Path) -> Running {
    let cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_deedar-mcp")).configure(|cmd| {
        cmd.env("HOME", home)
            .env("XDG_DATA_HOME", xdg)
            .env("DEEDAR_HOST_SIGNING_KEY", "off")
            .env_remove("DEEDAR_HOST_KEY")
            .env_remove("DEEDAR_URL");
    });
    let (transport, stderr) = TokioChildProcess::builder(cmd)
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn deedar-mcp");
    let client = ().serve(transport).await.expect("deedar-mcp speaks");
    Running {
        client,
        stderr: stderr.expect("stderr is piped"),
    }
}

fn text_of(result: &CallToolResult) -> String {
    assert_eq!(result.content.len(), 1, "{result:?}");
    result.content[0]
        .as_text()
        .unwrap_or_else(|| panic!("{result:?}"))
        .text
        .clone()
}

async fn answer(running: &Running, name: &str, arguments: Option<Value>) -> Result<String, String> {
    let mut params = CallToolRequestParams::new(name.to_string());
    if let Some(Value::Object(map)) = arguments {
        params = params.with_arguments(map);
    }
    match running.client.peer().call_tool(params).await {
        Ok(result) if result.is_error == Some(true) => Err(text_of(&result)),
        Ok(result) => Ok(text_of(&result)),
        Err(ServiceError::McpError(data)) => Err(data.message.into_owned()),
        Err(other) => panic!("{name}: {other}"),
    }
}

async fn stderr_of(mut running: Running) -> String {
    running.client.cancel().await.expect("stop");
    let mut logged = String::new();
    running
        .stderr
        .read_to_string(&mut logged)
        .await
        .expect("stderr");
    logged
}

#[tokio::test]
async fn a_read_of_a_missing_seat_store_names_the_path_and_creates_nothing() {
    let scratch = tempfile::tempdir().expect("scratch");
    let xdg = scratch.path().join("xdg");
    let home = scratch.path().join("home");
    let store = xdg.join("deedar/store");
    let running = seat_server(&xdg, &home).await;
    let err = answer(&running, "deedar_list", None)
        .await
        .expect_err("list");
    assert_eq!(err, seat_refusal(&store));
    assert_eq!(stderr_of(running).await, "");
    assert!(!store.exists(), "a read created the seat store");
    assert!(!home.join(".local").exists(), "a store was made under HOME");
}

#[tokio::test]
async fn an_export_creates_the_missing_seat_store_and_names_it() {
    let scratch = tempfile::tempdir().expect("scratch");
    let xdg = scratch.path().join("xdg");
    let home = scratch.path().join("home");
    let store = xdg.join("deedar/store");
    let export = json!({
        "into": scratch.path().join("bag").display().to_string(),
        "accessions": [],
    });
    let running = seat_server(&xdg, &home).await;
    let first = answer(&running, "deedar_export", Some(export.clone()))
        .await
        .expect("export");
    let line = created_line(&store);
    assert_eq!(first, format!("{line}\nexported 0 deeds, 0 files\n"));
    assert!(store.is_dir(), "the write did not create the seat store");
    assert!(
        !home.join(".local").exists(),
        "a second store was made under HOME"
    );
    let second = answer(&running, "deedar_export", Some(export))
        .await
        .expect("export again");
    assert_eq!(second, "exported 0 deeds, 0 files\n");
    assert_eq!(stderr_of(running).await, format!("{line}\n"));
}

#[tokio::test]
async fn a_file_at_the_seat_path_is_not_a_store_a_write_may_create() {
    let scratch = tempfile::tempdir().expect("scratch");
    let xdg = scratch.path().join("xdg");
    let home = scratch.path().join("home");
    let store = xdg.join("deedar/store");
    std::fs::create_dir_all(store.parent().expect("parent")).expect("parent");
    std::fs::write(&store, b"not a store\n").expect("file");
    let running = seat_server(&xdg, &home).await;
    let err = answer(
        &running,
        "deedar_export",
        Some(json!({
            "into": scratch.path().join("bag").display().to_string(),
            "accessions": [],
        })),
    )
    .await
    .expect_err("export");
    assert_eq!(err, seat_refusal(&store));
    assert_eq!(stderr_of(running).await, "");
    assert_eq!(std::fs::read(&store).expect("file"), b"not a store\n");
}
