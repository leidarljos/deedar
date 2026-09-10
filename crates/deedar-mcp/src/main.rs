//! `deedar-mcp`: the deed store over the Model Context Protocol.
//!
//! One store per process, named by `DEEDAR_URL`, the same as the command line.
//! Stdio, because a seat runs this beside the agent rather than as a service.

mod args;
mod prompts;
mod server;

use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = server::DeedarServer::from_env()?;
    let running = service.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
