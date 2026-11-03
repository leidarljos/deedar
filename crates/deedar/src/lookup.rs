//! What to say when an accession does not name one deed.
//!
//! The command line and the MCP server print the same sentence. Each one
//! supplies the step a seat runs next, because one is a shell command and
//! the other is a tool name.

use deed::Result;

use crate::Client;

/// Accessions an ambiguous-prefix line prints.
///
/// The line names every match up to this many, in byte order, and always
/// says how many matched. Eight is enough to tell a shared revision prefix
/// from a shared kind without the line becoming the whole store; anything
/// past it stays a count, and the list step is what a seat runs next
/// (`deedar list`, or `deedar_list` over MCP). The same bound is used for
/// every store.
pub const PREFIX_CANDIDATES_SHOWN: usize = 8;

/// Accessions whose id, deed digest, or product path starts with `raw`.
///
/// `proper_prefix` is set when one of those is longer than `raw`, which is
/// a prefix the seat can extend. An exact product path shared by two deeds
/// is not a prefix; it still names both accessions.
///
/// # Errors
///
/// Fails when the store cannot be listed.
pub fn prefix_matches(client: &mut Client, raw: &str) -> Result<(Vec<String>, bool)> {
    let mut ids = Vec::new();
    let mut proper_prefix = false;
    for deed in client.list()? {
        let id = deed.id.to_string();
        let digest = crate::deed_digest(&deed);
        let mut hit = starts_with(&id, raw) || starts_with(&digest, raw);
        let mut extends = extends_prefix(&id, raw) || extends_prefix(&digest, raw);
        for path in &deed.paths {
            let Some(text) = path.to_str() else {
                continue;
            };
            hit |= starts_with(text, raw);
            extends |= extends_prefix(text, raw);
        }
        if hit {
            proper_prefix |= extends;
            ids.push(id);
        }
    }
    // Accessions are ASCII, so this char order is byte order, and it is the
    // order `list` prints. Sorting here keeps the line on that rule if `list`
    // ever changes order.
    ids.sort();
    Ok((ids, proper_prefix))
}

/// The sentence for an accession that did not resolve to one deed.
///
/// `next_list` is the step when nothing matches or several do.
/// `next_get` is the step for the one match, without that accession, which
/// is appended. Both are wrapped in backticks, the way the line names a
/// command.
///
/// # Errors
///
/// Fails when the store cannot be listed.
pub fn explain_accession(
    client: &mut Client,
    raw: &str,
    next_list: &str,
    next_get: &str,
) -> Result<String> {
    let (ids, proper_prefix) = prefix_matches(client, raw)?;
    let line = match ids.as_slice() {
        [] => format!("unknown accession {raw}; `{next_list}`"),
        [only] if proper_prefix => {
            format!("prefix {raw} matches {only}; `{next_get} {only}`")
        }
        [only] => format!("unknown accession {raw}; `{next_get} {only}`"),
        many => {
            let shown = many.len().min(PREFIX_CANDIDATES_SHOWN);
            let listed = many[..shown].join(", ");
            let kind = if proper_prefix {
                "ambiguous prefix"
            } else {
                "ambiguous"
            };
            let n = many.len();
            format!(
                "{kind} {raw} matches {n} deeds, at most {PREFIX_CANDIDATES_SHOWN} in byte order: \
                 {listed}; `{next_list}`"
            )
        }
    };
    Ok(line)
}

fn starts_with(text: &str, raw: &str) -> bool {
    text.starts_with(raw)
}

fn extends_prefix(text: &str, raw: &str) -> bool {
    text.starts_with(raw) && text != raw
}
