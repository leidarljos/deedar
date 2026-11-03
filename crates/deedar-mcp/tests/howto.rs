//! The how-to names the tools the running server registers.
//!
//! A seat reads `docs/orgmode/howto.org` and `docs/source/howto.rst`. Each
//! registered tool has one line there: the first sentence of the description
//! the server publishes, and whether that tool reads or writes.

use std::path::{Path, PathBuf};

use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::ServiceExt;

fn repo_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name)
}

fn first_sentence(description: &str) -> &str {
    match description.find('.') {
        Some(end) => &description[..=end],
        None => description,
    }
}

/// Markup differs between the org source and the rst copy. The words do not.
fn plain(text: &str) -> String {
    text.chars().filter(|c| *c != '`' && *c != '=').collect()
}

fn tool_tokens(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut rest = 0;
    while let Some(found) = text[rest..].find("deedar_") {
        let start = rest + found;
        let mut end = start + "deedar_".len();
        while end < bytes.len() && (bytes[end].is_ascii_lowercase() || bytes[end] == b'_') {
            end += 1;
        }
        out.push(text[start..end].to_string());
        rest = end;
    }
    out
}

fn page(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[tokio::test]
async fn the_howto_lists_the_tools_the_server_registers() {
    let scratch = tempfile::tempdir().expect("scratch");
    let missing = scratch.path().join("no-store");
    assert!(!missing.exists(), "the probe store was already there");
    let url = format!("file://{}", missing.display());
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_deedar-mcp")).configure(|cmd| {
            cmd.env("DEEDAR_URL", &url)
                .env("DEEDAR_HOST_SIGNING_KEY", "off")
                .env_remove("DEEDAR_HOST_KEY");
        }),
    )
    .expect("spawn deedar-mcp");
    let client = ().serve(transport).await.expect("deedar-mcp speaks");
    let tools = client.peer().list_all_tools().await.expect("tools/list");
    client.cancel().await.expect("stop");
    assert!(
        !missing.exists(),
        "listing tools created {}",
        missing.display()
    );

    let org = page(&repo_file("docs/orgmode/howto.org"));
    let rst = page(&repo_file("docs/source/howto.rst"));
    for page in [&org, &rst] {
        assert!(
            page.contains("\"command\": \"deedar-mcp\""),
            "the client entry does not start deedar-mcp:\n{page}"
        );
        assert!(
            page.contains("$ deedar-mcp"),
            "the sample does not run deedar-mcp"
        );
        assert!(
            page.contains("unknown command mcp"),
            "the page does not record what `deedar mcp` prints"
        );
    }

    let mut registered = Vec::new();
    for tool in &tools {
        let name = tool.name.as_ref();
        registered.push(name.to_string());
        let description = tool
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("{name} carries no description"));
        let sentence = plain(first_sentence(description));
        let reads = match tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("{name} carries no annotations"))
            .read_only_hint
        {
            Some(true) => true,
            Some(false) => false,
            None => panic!("{name} does not say whether it writes"),
        };
        let marker = if reads { " reads. " } else { " writes. " };
        for (which, text) in [("org", &org), ("rst", &rst)] {
            let hits: Vec<&str> = text
                .lines()
                .filter(|line| {
                    let line = plain(line);
                    line.contains(name) && line.contains(sentence.as_str())
                })
                .collect();
            assert_eq!(
                hits.len(),
                1,
                "{which} should name {name} with its description once, found {hits:?}"
            );
            let line = plain(hits[0]);
            assert!(
                line.contains(marker),
                "{which} {name} should say{marker}in: {line}"
            );
        }
    }
    registered.sort();
    registered.dedup();
    for (which, text) in [("org", &org), ("rst", &rst)] {
        let mut tokens = tool_tokens(text);
        tokens.sort();
        tokens.dedup();
        assert_eq!(tokens, registered, "{which} tool names drifted");
    }
}
