use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::id::DeedId;
use crate::record::Deed;
use crate::source::Source;

/// Walk `sources` that name other deeds.
pub fn walk_trail<F>(start: &Deed, mut get: F) -> Result<Vec<Deed>>
where
    F: FnMut(&DeedId) -> Result<Deed>,
{
    let mut out = vec![start.clone()];
    let mut seen = HashSet::new();
    seen.insert(start.id.clone());
    let mut stack: Vec<DeedId> = deed_sources(start);
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        match get(&id) {
            Ok(next) => {
                stack.extend(deed_sources(&next));
                out.push(next);
            }
            Err(Error::NotFound(_)) | Err(Error::Tombstoned(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

fn deed_sources(deed: &Deed) -> Vec<DeedId> {
    deed.sources
        .iter()
        .filter_map(|s| match s {
            Source::Deed(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}
