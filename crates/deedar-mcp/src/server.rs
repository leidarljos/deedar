//! The deed store over MCP. Every tool reads; `delete` is not offered.
//! The `log` verbs answer whether the store still answers for what it published.

use std::path::PathBuf;

use rmcp::{
    handler::server::wrapper::Json, handler::server::wrapper::Parameters,
    handler::server::ServerHandler, model::*, prompt_handler, tool, tool_handler, tool_router,
    ErrorData as McpError,
};
use serde::Serialize;

use deedar::Client;

use crate::args::*;

/// The scheme a deed is addressable under.
const SCHEME: &str = "deed";

/// A deed's canonical rendering is text, and its bytes are Cap'n.
const TEXT: &str = "text/plain";

#[derive(Clone)]
pub struct DeedarServer {
    url: String,
}

/// One deed, flattened: the fields every deed has, and the body rendered.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct DeedRow {
    /// The accession, which is what crosses the three stores.
    pub id: String,
    /// `file`, `patch`, `quote` and the rest.
    pub kind: String,
    /// What it was called when it was frozen.
    pub name: String,
    /// Content addresses of the products, when it has any.
    pub paths: Vec<String>,
    /// What this deed was made from: deeds, paths, or urls.
    pub sources: Vec<String>,
    /// The agent that produced it.
    pub agent: String,
    /// `sha256:` of the canonical bytes, which `get` also accepts.
    pub digest: String,
}

/// What the log says about itself.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct HeadRow {
    /// Entries the root covers.
    pub size: usize,
    /// Merkle root, hex.
    pub root: String,
}

/// What an audit found, in both directions.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct AuditRow {
    /// Whether the store answers for everything it logged and logged
    /// everything it serves.
    pub clean: bool,
    /// Entries the log holds.
    pub logged: usize,
    /// Logged and no longer served as logged, which is the tampering the log
    /// exists to catch.
    pub missing: Vec<String>,
    /// Served and never logged, which is a store behind the log rather than
    /// one that lost something.
    pub unlogged: Vec<String>,
    /// Whether backfilling would settle it.
    pub backfill_settles_it: bool,
}

/// What checking a handover established.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct HandoverRow {
    /// The accessions whose bytes match their digest and whose path reaches
    /// the head, both, since neither implies the other.
    pub proven: Vec<String>,
    /// The one head every deed in the bag was against. Record it: the next
    /// handover from the same sender is checked against this.
    pub head: Option<HeadRow>,
    /// Who signed that head, when the bag carried a signature over it. An
    /// unsigned head means every proof above is against a head the bag
    /// asserted about itself.
    pub head_signer: Option<String>,
    /// Whether that signer was on this reader's list, as opposed to merely
    /// being the one the file named.
    pub head_accepted: bool,
    /// Whether a bridge was checked, so the head above is the head this reader
    /// already held, grown rather than replaced.
    pub bridged_from: Option<usize>,
}

fn row(deed: &deed::Deed) -> DeedRow {
    DeedRow {
        id: deed.id.to_string(),
        kind: deed.kind.token().to_string(),
        name: deed.name.clone(),
        paths: deed.paths.iter().map(|p| p.display().to_string()).collect(),
        sources: deed.sources.iter().map(source_text).collect(),
        agent: deed.produced_by.agent_id.clone(),
        digest: deedar::deed_digest(deed),
    }
}

fn source_text(source: &deed::Source) -> String {
    match source {
        deed::Source::Deed(id) => format!("deed:{id}"),
        deed::Source::Path(path) => format!("path:{}", path.display()),
        deed::Source::Url(url) => format!("url:{url}"),
    }
}

fn open(url: &str) -> Result<Client, McpError> {
    Client::open(url).map_err(|e| McpError::internal_error(format!("{e}"), None))
}

fn bad(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(format!("{e}"), None)
}

#[tool_router]
impl DeedarServer {
    /// Open on the store `DEEDAR_URL` names.
    ///
    /// # Errors
    ///
    /// Fails when no store is configured and none can be guessed.
    pub fn from_env() -> anyhow::Result<Self> {
        let url = std::env::var("DEEDAR_URL").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("file://{home}/.local/share/deedar/store")
        });
        Ok(Self { url })
    }

    /// Open on a named store, for a test.
    #[cfg(test)]
    #[must_use]
    pub fn at(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    #[tool(
        description = "One deed by accession, or by a `sha256:` of the deed or of one of its product paths. A deed is frozen: what this returns for an accession does not change.",
        annotations(title = "Read a deed", read_only_hint = true, open_world_hint = false)
    )]
    async fn deedar_get(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<Json<DeedRow>, McpError> {
        let mut client = open(&self.url)?;
        let id = client.resolve(&args.id).map_err(bad)?;
        let deed = client.get(&id).map_err(bad)?;
        Ok(Json(row(&deed)))
    }

    #[tool(
        description = "Every deed the store serves, oldest first.",
        annotations(title = "List deeds", read_only_hint = true, open_world_hint = false)
    )]
    async fn deedar_list(&self) -> Result<Json<Vec<DeedRow>>, McpError> {
        let mut client = open(&self.url)?;
        Ok(Json(client.list().map_err(bad)?.iter().map(row).collect()))
    }

    #[tool(
        description = "The supersede chain behind a deed: what it replaced, and what replaced that. Use this when an accession is cited somewhere and you need to know whether it is still the tip.",
        annotations(
            title = "Follow a supersede chain",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_trail(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<Json<Vec<DeedRow>>, McpError> {
        let mut client = open(&self.url)?;
        let id = client.resolve(&args.id).map_err(bad)?;
        Ok(Json(
            client.trail(&id).map_err(bad)?.iter().map(row).collect(),
        ))
    }

    #[tool(
        description = "The deed that supersedes this one, or this one when nothing does. A citation that is not the tip is stale, and this is how a caller finds out without reading the chain.",
        annotations(
            title = "The current deed",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_current(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<Json<DeedRow>, McpError> {
        let mut client = open(&self.url)?;
        let id = client.resolve(&args.id).map_err(bad)?;
        let deed = client.current(&id).map_err(bad)?;
        Ok(Json(row(&deed)))
    }

    #[tool(
        description = "Whether the store still holds evidence for a deed, and what it says: who produced it and when.",
        annotations(
            title = "Evidence for a deed",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_evidence(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut client = open(&self.url)?;
        let id = client.resolve(&args.id).map_err(bad)?;
        let ev = client.evidence(&id).map_err(bad)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "id={} producedBy={} {} time={}\n",
            ev.deed_id,
            ev.produced_by.agent_id,
            ev.produced_by.activity_id.as_deref().unwrap_or("-"),
            ev.unix_time
        ))]))
    }

    #[tool(
        description = "The append-only log's head: how many deeds it covers and the Merkle root over them. Keep this between visits; a later head that does not extend it means the store rewrote what it already published.",
        annotations(title = "The log head", read_only_hint = true, open_world_hint = false)
    )]
    async fn deedar_log_head(&self) -> Result<Json<HeadRow>, McpError> {
        let client = open(&self.url)?;
        let head = client.log_head().map_err(bad)?;
        Ok(Json(HeadRow {
            size: head.size,
            root: head.root,
        }))
    }

    #[tool(
        description = "Whether the store answers for what it logged, and logged what it serves. A deed that is logged and no longer served is the tampering the log exists to catch; a deed served and never logged is a store behind its log, which backfilling settles.",
        annotations(
            title = "Audit the log",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_log_audit(&self) -> Result<Json<AuditRow>, McpError> {
        let client = open(&self.url)?;
        let audit = client.log_audit().map_err(bad)?;
        Ok(Json(AuditRow {
            clean: audit.is_clean(),
            logged: audit.logged,
            missing: audit
                .missing
                .iter()
                .map(|m| format!("{}: {}", m.id, m.why))
                .collect(),
            unlogged: audit.unlogged.clone(),
            backfill_settles_it: audit.backfill_settles_it(),
        }))
    }

    #[tool(
        description = "Write deeds into a directory with the proof they were logged here first, which is what a handover carries. Takes the accessions a satchel names.",
        annotations(
            title = "Export deeds",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_export(
        &self,
        Parameters(args): Parameters<ExportArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut client = open(&self.url)?;
        let into = PathBuf::from(&args.into);
        // Resolve first, then read the log once for the whole handover; see
        // the command line's export for why the order is the order.
        let resolved: Vec<(String, deed::Result<deed::DeedId>)> = args
            .accessions
            .iter()
            .map(|raw| (raw.clone(), client.resolve(raw)))
            .collect();
        let exporter = client
            .exporter()
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let mut wrote = 0usize;
        let mut refused = Vec::new();
        for (raw, id) in resolved {
            match id.and_then(|id| exporter.export(&id, &into)) {
                Ok(files) => wrote += files.len(),
                Err(e) => refused.push(format!("{raw}: {e}")),
            }
        }
        if refused.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "exported {} deeds, {wrote} files\n",
                args.accessions.len()
            ))]));
        }
        // Asked for and not supplied is the caller's problem to know about.
        Err(McpError::internal_error(refused.join("\n"), None))
    }

    #[tool(
        description = "Check deeds somebody else handed over: that each one's bytes match the digest its log entry recorded, and that its path reaches the head the bag names. Needs no store, because a receiver holds a bag and no log. Answers with the head to record for next time; pass that back as a bridge file in `since` to also check the sender's log only grew.",
        annotations(
            title = "Check a handover",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_check(
        &self,
        Parameters(args): Parameters<CheckArgs>,
    ) -> Result<Json<HandoverRow>, McpError> {
        let mut bridged_from = None;
        if let Some(path) = &args.since {
            let text = std::fs::read_to_string(path)
                .map_err(|_| McpError::invalid_params(format!("no bridge at {path}"), None))?;
            let bridge = deedar::Bridge::parse(&text)
                .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
            // A bag whose deeds check out against a head nobody has seen
            // before is a bag from a log that may have been rewritten, so this
            // failing is a refusal and not a note.
            bridge
                .check()
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            bridged_from = Some(bridge.from.size);
        }
        // The reader's signer list, from their own store's layout. A surface
        // that checked against no list would report the weaker of the two
        // answers as though it were the stronger.
        let accept = deedar::store_dir(&self.url)
            .ok()
            .and_then(|dir| deedar::Policy::read(&dir).ok())
            .map(|policy| policy.signers)
            .unwrap_or_default();
        let done = deedar::check_handover(&PathBuf::from(&args.dir), &accept)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(Json(HandoverRow {
            proven: done.proven,
            head: done.head.map(|head| HeadRow {
                size: head.size,
                root: head.root,
            }),
            head_signer: done.head_signer.map(|signer| deedar::log::hex(&signer)),
            head_accepted: done.head_accepted,
            bridged_from,
        }))
    }

    #[tool(
        description = "The record joining a head a reader already holds to this log's own, so they can tell a log that grew from one that was rewritten. Give it the size the reader recorded from an earlier handover.",
        annotations(
            title = "Bridge to an earlier head",
            read_only_hint = true,
            open_world_hint = false
        )
    )]
    async fn deedar_log_bridge(
        &self,
        Parameters(args): Parameters<BridgeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bridge = open(&self.url)?
            .bridge(args.from)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            bridge.render(),
        )]))
    }
}

#[tool_handler]
#[prompt_handler(router = Self::prompt_router())]
impl ServerHandler for DeedarServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("deedar", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "A deed is what a unit of work produced, frozen, and its accession is the \
             identifier that crosses the tracker, the pack and this store. Read one at \
             deed://<accession>. Ask whether a citation is still the tip with the current \
             verb rather than assuming. Ask whether the store still answers for what it \
             published with the audit, which is a different question from whether a deed \
             reads.",
        )
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let mut template =
            ResourceTemplate::new(format!("{SCHEME}://{{accession}}"), "deed".to_string());
        template.title = Some("One deed".to_string());
        template.description =
            Some("A deed by accession, rendered as the store prints it.".to_string());
        template.mime_type = Some(TEXT.to_string());
        Ok(ListResourceTemplatesResult::with_all_items(vec![template]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri.clone();
        let accession = uri
            .strip_prefix(&format!("{SCHEME}://"))
            .filter(|rest| !rest.is_empty())
            .ok_or_else(|| {
                McpError::resource_not_found(format!("not a {SCHEME} uri: {uri}"), None)
            })?;
        let mut client = open(&self.url)?;
        let id = client
            .resolve(accession)
            .map_err(|e| McpError::resource_not_found(format!("{e}"), None))?;
        let deed = client
            .get(&id)
            .map_err(|e| McpError::resource_not_found(format!("{e}"), None))?;
        let text = serde_json::to_string_pretty(&row(&deed)).map_err(bad)?;
        Ok(
            ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                uri,
                mime_type: Some(TEXT.to_string()),
                text,
                meta: None,
            }])
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deed::{Body, DeedId};
    use deedar::CreateRequest;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A store of this test's own, named by pid and a counter: the clock alone
    /// collides across tests in one process.
    fn store() -> String {
        static NTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let nth = NTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("deedar-mcp-{}-{n}-{nth}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        format!("file://{}", dir.display())
    }

    fn quote(url: &str, id: &str, excerpt: &str) {
        let mut client = Client::open(url).expect("open");
        client
            .create(CreateRequest {
                id: Some(DeedId::parse(id).expect("id")),
                name: id.into(),
                sources: Vec::new(),
                agent_id: "reader".into(),
                activity_id: None,
                grants: Vec::new(),
                body: Body::Quote {
                    edition: "https://example.invalid/a".into(),
                    start: 0,
                    end: excerpt.len() as u64,
                    excerpt: excerpt.into(),
                    urls: vec!["https://example.invalid/a".into()],
                },
                supersedes: None,
            })
            .expect("create");
    }

    /// Every tool says whether it reads, and all but one of them do, because a
    /// deed is frozen and the surface offers no way to unfreeze it.
    #[test]
    fn the_surface_is_reads_and_one_export() {
        let tools = DeedarServer::tool_router().list_all();
        assert!(tools.len() >= 7, "{} tools", tools.len());
        let mut writers = Vec::new();
        for tool in &tools {
            let hints = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} carries no annotations", tool.name));
            assert_eq!(
                hints.open_world_hint,
                Some(false),
                "{} claims an open world",
                tool.name
            );
            match hints.read_only_hint {
                Some(true) => {}
                Some(false) => {
                    writers.push(tool.name.to_string());
                    assert!(hints.destructive_hint.is_some(), "{}", tool.name);
                    assert!(hints.idempotent_hint.is_some(), "{}", tool.name);
                }
                None => panic!("{} does not say whether it writes", tool.name),
            }
        }
        // Export writes somewhere else. Nothing here writes the store, which
        // is the property a caller wants to be able to see.
        assert_eq!(writers, vec!["deedar_export".to_string()], "{writers:?}");
    }

    /// A deed reads back as the fields a caller wants rather than as a union.
    #[tokio::test]
    async fn a_deed_reads_back_flattened() {
        let url = store();
        quote(&url, "deed-quote-one", "what was said");
        let server = DeedarServer::at(&url);

        let got = server
            .deedar_get(Parameters(IdArgs {
                id: "deed-quote-one".into(),
            }))
            .await
            .expect("reads");
        assert_eq!(got.0.id, "deed-quote-one");
        assert_eq!(got.0.kind, "quote");
        assert!(got.0.digest.starts_with("sha256:"), "{}", got.0.digest);

        // The digest is an accession too, which is what makes a citation by
        // content work the same as one by name.
        let by_digest = server
            .deedar_get(Parameters(IdArgs {
                id: got.0.digest.clone(),
            }))
            .await
            .expect("reads by digest");
        assert_eq!(by_digest.0.id, got.0.id);
    }

    /// The audit answers the other question, and says which of the two states
    /// it found rather than a count.
    #[tokio::test]
    async fn the_audit_tells_a_clean_store_from_one_behind_its_log() {
        let url = store();
        quote(&url, "deed-quote-two", "logged as it was written");
        let server = DeedarServer::at(&url);

        let clean = server.deedar_log_audit().await.expect("audits");
        assert!(clean.0.clean, "{:?}", clean.0);
        assert_eq!(clean.0.logged, 1);

        // A store whose log was never written is behind, not tampered with.
        let dir = deedar::store_dir(&url).expect("dir");
        std::fs::remove_file(dir.join("log")).expect("remove");
        let behind = server.deedar_log_audit().await.expect("audits");
        assert!(!behind.0.clean, "{:?}", behind.0);
        assert!(behind.0.missing.is_empty(), "{:?}", behind.0);
        assert_eq!(behind.0.unlogged.len(), 1, "{:?}", behind.0);
        assert!(behind.0.backfill_settles_it, "{:?}", behind.0);
    }
}
