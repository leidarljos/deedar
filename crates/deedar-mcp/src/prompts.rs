//! The sequences this store is the authority for.
//!
//! A prompt is the primitive a person picks, and what it carries that a tool
//! description cannot is an order and the reason for it. Both sequences here
//! are orders that matter: each step answers a question the step before it
//! does not, and stopping early means believing an answer to a question nobody
//! asked.
//!
//! What belongs here is what this store can answer. Packing a slice is the
//! tracker's sequence and lives there; checking one that arrived is this
//! store's, because the proofs are its format.

use rmcp::{
    handler::server::wrapper::Parameters, model::*, prompt, prompt_router, ErrorData as McpError,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::server::DeedarServer;

/// A bag that arrived, and what the reader already holds.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArrivalArgs {
    /// The satchel directory somebody handed over.
    pub dir: String,
    /// A bridge file from an earlier handover by the same sender, if this
    /// reader has taken one before.
    pub since: Option<String>,
}

/// One accession, and how far back to look.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StandingArgs {
    /// `deed-<kind>-<slug>`, or a `sha256:` of the deed or of a product path.
    pub accession: String,
}

fn asked(text: String) -> Vec<PromptMessage> {
    vec![PromptMessage::new_text(Role::User, text)]
}

#[prompt_router(vis = "pub(crate)")]
impl DeedarServer {
    /// Check a handover somebody sent: the payload, who packed it, and whether
    /// the deeds inside predate the asking.
    #[prompt(name = "check_a_handover")]
    pub async fn check_a_handover_prompt(
        &self,
        Parameters(args): Parameters<ArrivalArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let dir = args.dir;
        let since = args.since.map_or_else(
            || {
                "No earlier head was named, so nothing here can say this log is one \
                 the reader has seen before. Record the head the check reports; the \
                 next handover from this sender is checked against it."
                    .to_string()
            },
            |path| {
                format!(
                    "An earlier head is at {path}. Check that bridge first: a bag whose \
                     deeds all check out against a head nobody has seen before is a bag \
                     from a log that may have been rewritten, and the receipts inside \
                     it would be perfectly good either way."
                )
            },
        );
        Ok(asked(format!(
            "Check the handover at {dir}.\n\
             \n\
             Three questions, in this order, because each one is unanswered by the\n\
             one before it:\n\
             \n\
             1. Did the bag arrive as written. The manifest against the payload. This\n\
                catches truncation and corruption and nothing else: a receiver who\n\
                recomputes digests from the bag they were handed is checking the bag\n\
                against itself.\n\
             2. Who wrote it. `deedar vouch check` on the manifest, against the keys\n\
                this reader accepts. Without a signer list the answer is only that the\n\
                bytes and the signature go together, which is worth saying out loud\n\
                rather than reading as a pass.\n\
             3. Do the deeds predate the asking. `deedar_check` with dir {dir}. A\n\
                sender who mints a deed the morning they are asked for it produces\n\
                bytes that are just as intact and a signature that is just as good;\n\
                only the inclusion proof against a head separates the two.\n\
             \n\
             {since}\n\
             \n\
             Report which of the three passed. A bag that passes the first two and\n\
             fails the third is not a bag that mostly checked out."
        )))
    }

    /// Follow one accession as far as it goes: what it says, what it stands
    /// on, whether it is still the tip, and whether the store still answers.
    #[prompt(name = "stand_behind_a_deed")]
    pub async fn stand_behind_a_deed_prompt(
        &self,
        Parameters(args): Parameters<StandingArgs>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let accession = args.accession;
        Ok(asked(format!(
            "Say what {accession} stands on and whether it still holds.\n\
             \n\
             `deedar_get` reads the deed. `deedar_trail` walks its inputs, which is\n\
             what it was made from rather than what it says.\n\
             \n\
             Then the two questions that are not about this deed's contents at all:\n\
             \n\
             - `deedar_evidence` says the bytes are intact and so are its sources'.\n\
             - `deedar_current` says whether a later take has superseded it. A\n\
               citation that resolves and is stale is worse than one that fails,\n\
               because nothing complains about it.\n\
             \n\
             And once, for the store rather than the deed: `deedar_log_audit`. Every\n\
             signature over a deleted deed stays perfectly good, so a pile of intact\n\
             signatures is not an answer about which deeds exist. The log is.\n\
             \n\
             Report the deed, its trail, and which of those checks it failed."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every declared prompt renders, from the arguments it says it takes.
    #[tokio::test]
    async fn every_prompt_renders_from_what_it_declares() {
        let declared = DeedarServer::prompt_router().list_all();
        let names: Vec<&str> = declared.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["check_a_handover", "stand_behind_a_deed"]);
        for prompt in &declared {
            assert!(
                prompt.description.as_ref().is_some_and(|d| !d.is_empty()),
                "{} carries no description",
                prompt.name
            );
            assert!(
                prompt.arguments.as_ref().is_some_and(|a| !a.is_empty()),
                "{} declares no arguments",
                prompt.name
            );
        }

        let server = DeedarServer::at("file:///tmp/deedar-prompts");
        let checked = server
            .check_a_handover_prompt(Parameters(ArrivalArgs {
                dir: "/tmp/bag".into(),
                since: Some("/tmp/bag/bridge.txt".into()),
            }))
            .await
            .expect("renders");
        let said = format!("{:?}", checked[0].content);
        assert!(said.contains("/tmp/bag"), "{said}");
        assert!(said.contains("bridge.txt"), "{said}");
        assert!(said.contains("may have been rewritten"), "{said}");

        // With no earlier head, the text says what to do with the one this
        // check reports rather than leaving the reader with an unused answer.
        let first = server
            .check_a_handover_prompt(Parameters(ArrivalArgs {
                dir: "/tmp/bag".into(),
                since: None,
            }))
            .await
            .expect("renders");
        assert!(format!("{:?}", first[0].content).contains("Record the head"));

        let standing = server
            .stand_behind_a_deed_prompt(Parameters(StandingArgs {
                accession: "deed-file-note".into(),
            }))
            .await
            .expect("renders");
        assert!(format!("{:?}", standing[0].content).contains("deed-file-note"));
    }
}
