//! Which store `deedar-mcp` opens when `DEEDAR_URL` does not name one.
//!
//! The command line uses `$XDG_DATA_HOME/deedar/store` before
//! `~/.local/share/deedar/store`. A deed the command line stored there has to
//! come back from `deedar_list`.

use std::path::Path;

use deed::{Body, DeedId};
use deedar::{Client, CreateRequest};
use rmcp::model::CallToolRequestParams;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::ServiceExt;

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
