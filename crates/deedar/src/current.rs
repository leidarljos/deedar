use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use deed::{Deed, DeedId, Error, Result};

use crate::fs::{atomic_write, FsStore};

impl FsStore {
    /// Walk supersede edges from `id` and say which deed is current.
    ///
    /// An edge is the accession in `{prior}.successor`, or in
    /// `{next}.supersedes` when that file names `prior`. Create records
    /// one successor. A second deed can still name the same prior, and
    /// that is a fork: every deed with no live edge out is current.
    ///
    /// One current deed is returned, and `deedar current` prints it.
    /// Several are all current. Their accessions are listed in accession
    /// order, which does not follow directory order. The command prints
    /// one `id=` line for each, exits 0, and writes one stderr line
    /// `deedar: warning: fork {id} {id}`.
    /// A walk that reaches a deed already on its path is a cycle. Those
    /// accessions are listed once, in that same order. The command prints
    /// one `id=` line for each, exits 0, and writes one stderr line
    /// `deedar: warning: cycle {id} {id}`. The walk is not repeated.
    ///
    /// An edge to a missing or tombstoned deed is not an edge. The chain
    /// ends at the last deed that is here. When a cycle is reachable,
    /// the cycle is the answer: a tip on another branch would hide it.
    ///
    /// # Errors
    ///
    /// `NotFound` when the deed the walk ends on is missing, and
    /// `Tombstoned` when that deed is tombstoned. A fork or a cycle
    /// returns `InvalidBody` carrying `fork {id}...` or `cycle {id}...`.
    /// The command prints those as above and still exits 0. A sidecar
    /// that does not parse, or a deeds directory that cannot be read,
    /// returns that error.
    pub fn current(&self, id: &DeedId) -> Result<Deed> {
        let superseded_by = self.superseded_by()?;
        match walk(self, id, &superseded_by)? {
            Walk::Tip(tip) => self.get(&tip),
            Walk::Fork(ids) => Err(Error::InvalidBody(format!("fork {}", join_ids(&ids)))),
            Walk::Cycle(ids) => Err(Error::InvalidBody(format!("cycle {}", join_ids(&ids)))),
        }
    }

    /// The successor `current` steps to from `id`: named by `{id}.successor`,
    /// held here, and not tombstoned.
    pub(crate) fn next_tip(&self, id: &DeedId) -> Result<Option<DeedId>> {
        let Some(next) = read_successor(&self.successor_path(id))? else {
            return Ok(None);
        };
        if self.tomb_path(&next).exists() || !self.deed_path(&next).exists() {
            return Ok(None);
        }
        Ok(Some(next))
    }

    pub(crate) fn record_supersedes(&self, new: &DeedId, prior: &DeedId) -> Result<()> {
        if new == prior {
            return Err(Error::InvalidBody("supersedes self".into()));
        }
        if !self.deed_path(prior).exists() {
            return Err(Error::NotFound(prior.to_string()));
        }
        let successor = self.successor_path(prior);
        if successor.exists() {
            return Err(Error::Frozen(prior.to_string()));
        }
        atomic_write(&self.supersedes_path(new), format!("{prior}\n").as_bytes())?;
        atomic_write(&successor, format!("{new}\n").as_bytes())?;
        Ok(())
    }

    pub(crate) fn successor_path(&self, id: &DeedId) -> PathBuf {
        self.dir().join("deeds").join(format!("{id}.successor"))
    }

    pub(crate) fn supersedes_path(&self, id: &DeedId) -> PathBuf {
        self.dir().join("deeds").join(format!("{id}.supersedes"))
    }

    /// Deeds that name `prior` in `{next}.supersedes` and are still here.
    ///
    /// The successor file holds one accession. A fork is a second
    /// `.supersedes` naming the same prior, so the edges have to be read
    /// from those files. A missing or tombstoned deed is left out: `current`
    /// does not step to it, the same rule as [`Self::next_tip`].
    fn superseded_by(&self) -> Result<BTreeMap<String, BTreeSet<String>>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let entries =
            fs::read_dir(self.dir().join("deeds")).map_err(|e| Error::Io(e.to_string()))?;
        for entry in entries {
            let path = entry.map_err(|e| Error::Io(e.to_string()))?.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(raw) = name.strip_suffix(".supersedes") else {
                continue;
            };
            // A name that is not an accession is not an edge.
            let Ok(next) = DeedId::parse(raw) else {
                continue;
            };
            if self.tomb_path(&next).exists() || !self.deed_path(&next).exists() {
                continue;
            }
            let Some(link) = crate::fs::read_supersedes(&path)? else {
                continue;
            };
            out.entry(link.id.to_string())
                .or_default()
                .insert(next.to_string());
        }
        Ok(out)
    }
}

fn read_successor(path: &std::path::Path) -> Result<Option<DeedId>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| Error::Io(e.to_string()))?;
    let id = raw.trim();
    if id.is_empty() {
        return Ok(None);
    }
    Ok(Some(DeedId::parse(id)?))
}

/// Where the walk from one accession ended.
enum Walk {
    /// One deed has no live edge out.
    Tip(DeedId),
    /// More than one does. Accession order.
    Fork(Vec<DeedId>),
    /// Deeds that sit on a cycle. Accession order, each once.
    Cycle(Vec<DeedId>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    /// On the path from the start. A step back here is a cycle.
    OnPath,
    /// Every edge out of it has been followed. It is not entered again.
    Done,
}

struct Frame {
    id: DeedId,
    succs: Vec<DeedId>,
    next: usize,
}

fn walk(
    store: &FsStore,
    start: &DeedId,
    superseded_by: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Walk> {
    let mut color: HashMap<DeedId, Color> = HashMap::new();
    let mut tips: BTreeSet<String> = BTreeSet::new();
    let mut cycle: BTreeSet<String> = BTreeSet::new();
    let mut path: Vec<DeedId> = Vec::new();
    let mut stack = vec![Frame {
        id: start.clone(),
        succs: successors(store, start, superseded_by)?,
        next: 0,
    }];
    color.insert(start.clone(), Color::OnPath);
    path.push(start.clone());

    // A deed is pushed once, when it is first entered. A cycle steps back
    // to one already on the path and does not push it again, so the walk
    // cannot spin.
    while !stack.is_empty() {
        let at = stack.len() - 1;
        if stack[at].next == stack[at].succs.len() {
            let frame = match stack.pop() {
                Some(frame) => frame,
                None => break,
            };
            if frame.succs.is_empty() {
                tips.insert(frame.id.to_string());
            }
            color.insert(frame.id, Color::Done);
            path.pop();
            continue;
        }
        let next = stack[at].succs[stack[at].next].clone();
        stack[at].next += 1;
        match color.get(&next).copied() {
            Some(Color::OnPath) => {
                if let Some(i) = path.iter().position(|id| id == &next) {
                    for id in &path[i..] {
                        cycle.insert(id.to_string());
                    }
                }
            }
            Some(Color::Done) => {}
            None => {
                let succs = successors(store, &next, superseded_by)?;
                color.insert(next.clone(), Color::OnPath);
                path.push(next.clone());
                stack.push(Frame {
                    id: next,
                    succs,
                    next: 0,
                });
            }
        }
    }

    if !cycle.is_empty() {
        return Ok(Walk::Cycle(ids_in_order(cycle)?));
    }
    let tips = ids_in_order(tips)?;
    match tips.as_slice() {
        [] => Ok(Walk::Tip(start.clone())),
        [one] => Ok(Walk::Tip(one.clone())),
        _ => Ok(Walk::Fork(tips)),
    }
}

/// Live deeds that supersede `id`, in accession order.
///
/// Accession text is the order a seat can recompute. Directory order is
/// not the same on every machine, so the list is not taken from `read_dir`.
fn successors(
    store: &FsStore,
    id: &DeedId,
    superseded_by: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<DeedId>> {
    let mut names = BTreeSet::new();
    if let Some(next) = store.next_tip(id)? {
        names.insert(next.to_string());
    }
    if let Some(more) = superseded_by.get(id.as_str()) {
        names.extend(more.iter().cloned());
    }
    ids_in_order(names)
}

fn ids_in_order(names: BTreeSet<String>) -> Result<Vec<DeedId>> {
    names.into_iter().map(|name| DeedId::parse(&name)).collect()
}

fn join_ids(ids: &[DeedId]) -> String {
    ids.iter()
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}
