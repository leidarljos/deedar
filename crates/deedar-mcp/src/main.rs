//! `deedar-mcp`: the deed store over the Model Context Protocol.
//!
//! One store per process, named by `DEEDAR_URL`, the same as the command line.
//! Stdio, because a seat runs this beside the agent rather than as a service.

mod args;
mod prompts;
mod server;

use rmcp::{transport::stdio, ServiceExt};

const HELP: &str = "\
deedar-mcp
speaks the Model Context Protocol on standard input and output
the store is DEEDAR_URL, else $XDG_DATA_HOME/deedar/store, else ~/.local/share/deedar/store
DEEDAR_HOST_SIGNING_KEY is the same variable the deedar command reads
deedar-mcp --help     this page
deedar-mcp --version  the version
with no arguments it waits for a client
";

/// `--help` and `--version` print and exit. Any other argument is refused.
/// `None` means start the server.
fn cli_page() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{HELP}");
        return Some(0);
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("deedar-mcp {}", env!("CARGO_PKG_VERSION"));
        return Some(0);
    }
    if args.is_empty() {
        return None;
    }
    eprintln!("deedar-mcp takes no arguments and speaks on standard input; `deedar-mcp --help`");
    Some(1)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(code) = cli_page() {
        std::process::exit(code);
    }
    let service = server::DeedarServer::from_env()?;
    let running = service.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
