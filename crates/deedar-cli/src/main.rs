//! Operator path: create, get, list, trail, evidence, delete, leave, timestamp, current, migrate.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use deed::{Body, Deed, DeedId, FormField, Grant, Kind, MailMessageId, Measure, Source, Step};
use deedar::{Client, CreateRequest, StoreUrl};

fn main() -> ExitCode {
    match dispatch(env::args().skip(1).collect()) {
        Ok(report) => {
            print!("{}", report.text);
            if report.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("deedar: {e}");
            ExitCode::from(1)
        }
    }
}

/// What a verb produced: text to print, and whether the run succeeded.
///
/// Every other verb fails by returning `Err`, which prints one message and
/// nothing else. Checking a list of ids cannot work that way: the point is to
/// say which ones failed, so it has to print a report *and* exit non-zero.
struct Report {
    text: String,
    ok: bool,
}

/// Route the verbs that report per-id, and hand the rest to [`run`].
fn dispatch(args: Vec<String>) -> Result<Report, String> {
    // Before the store is resolved: asking which build this is must work on a
    // machine that has no store yet, and it is how anything checking for drift
    // finds out.
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return Ok(Report {
            text: format!("deedar {}
", env!("CARGO_PKG_VERSION")),
            ok: true,
        });
    }
    let (url, rest) = take_url(&args)?;
    if rest.first().map(String::as_str) == Some("evidence") && is_many(&rest[1..]) {
        return evidence_many(&url, &rest[1..]);
    }
    if rest.first().map(String::as_str) == Some("current") && is_many(&rest[1..]) {
        return current_many(&url, &rest[1..]);
    }
    run(args).map(|text| Report { text, ok: true })
}

/// Whether an `evidence` call names more than one deed.
///
/// A single id keeps the single-deed report it always had. `-` reads the ids
/// from standard input, one per line, which is the form a working set arrives
/// in from whatever tool holds the citations. A bare `evidence` stays an error
/// rather than blocking on a terminal nobody meant to type into.
fn is_many(args: &[String]) -> bool {
    args.len() > 1 || args.first().is_some_and(|a| a == "-")
}

/// Check every named deed, reporting one line each.
///
/// Fails closed on the whole list: one unverifiable deed in a working set is
/// enough to make the set untrustworthy, and a caller that has to parse the
/// text to find that out will not.
/// The ids a set-shaped call names, from the arguments or from stdin.
fn ids_from(args: &[String], verb: &str) -> Result<Vec<String>, String> {
    let ids: Vec<String> = if args.first().is_some_and(|a| a == "-") {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .map_err(|e| format!("read ids from stdin: {e}"))?;
        buf.split_whitespace().map(str::to_string).collect()
    } else {
        args.to_vec()
    };
    if ids.is_empty() {
        return Err(format!("{verb} - read no ids"));
    }
    Ok(ids)
}

/// Follow every named deed to the tip of its supersede chain.
///
/// The set form of `current`, and the reason it exists: a citation is written
/// once and the thing it names can be superseded afterwards. Whatever holds the
/// citations, a tracker or a pack, has no way to notice that on its own, so it
/// hands the list over and this says which of them have moved on.
///
/// Exits non-zero when any citation is stale, so a hook can gate on it. A
/// missing deed is stale too: a citation that resolves to nothing is not a
/// citation anybody should keep.
fn current_many(url: &str, args: &[String]) -> Result<Report, String> {
    let ids = ids_from(args, "current")?;
    let mut client = Client::open(url).map_err(|e| e.to_string())?;
    let mut text = String::new();
    let mut stale = 0usize;
    for raw in &ids {
        match client
            .resolve(raw)
            .and_then(|id| client.current(&id).map(|d| (id, d)))
        {
            Ok((id, tip)) => {
                if tip.id.to_string() == id.to_string() {
                    text.push_str(&format!("{id} current\n"));
                } else {
                    stale += 1;
                    text.push_str(&format!("{id} SUPERSEDED by {}\n", tip.id));
                }
            }
            Err(e) => {
                stale += 1;
                text.push_str(&format!("{raw} FAILED: {e}\n"));
            }
        }
    }
    text.push_str(&format!("{} of {} current\n", ids.len() - stale, ids.len()));
    Ok(Report {
        ok: stale == 0,
        text,
    })
}

fn evidence_many(url: &str, args: &[String]) -> Result<Report, String> {
    let ids = ids_from(args, "evidence")?;
    let mut client = Client::open(url).map_err(|e| e.to_string())?;
    let mut text = String::new();
    let mut verified = 0usize;
    for raw in &ids {
        match client
            .resolve(raw)
            .and_then(|id| client.evidence(&id).map(|ev| ev.deed_id))
        {
            Ok(id) => {
                verified += 1;
                text.push_str(&format!("{id} ok\n"));
            }
            // The id as the caller wrote it, not as the store would spell it:
            // an id that does not resolve has no other spelling, and the
            // caller has to find this one in whatever cited it.
            Err(e) => text.push_str(&format!("{raw} FAILED: {e}\n")),
        }
    }
    text.push_str(&format!("{verified} of {} verified\n", ids.len()));
    Ok(Report {
        ok: verified == ids.len(),
        text,
    })
}

fn run(args: Vec<String>) -> Result<String, String> {
    let (url, rest) = take_url(&args)?;
    let mut client = Client::open(&url).map_err(|e| e.to_string())?;
    match rest.first().map(String::as_str) {
        Some("create") => cmd_create(&mut client, &rest[1..]),
        Some("get") => {
            let id = client
                .resolve(rest.get(1).ok_or("get needs an id")?)
                .map_err(|e| e.to_string())?;
            let d = client.get(&id).map_err(|e| e.to_string())?;
            Ok(format_one(&d))
        }
        Some("list") => {
            let rows = client.list().map_err(|e| e.to_string())?;
            Ok(format_list(&rows))
        }
        Some("trail") => {
            let id = client
                .resolve(rest.get(1).ok_or("trail needs an id")?)
                .map_err(|e| e.to_string())?;
            let rows = client.trail(&id).map_err(|e| e.to_string())?;
            Ok(format_list(&rows))
        }
        Some("evidence") => {
            let id = client
                .resolve(rest.get(1).ok_or("evidence needs an id")?)
                .map_err(|e| e.to_string())?;
            let ev = client.evidence(&id).map_err(|e| e.to_string())?;
            Ok(format!(
                "id={} ok\nproducedBy={} {}\ntime={}\n",
                ev.deed_id,
                ev.produced_by.agent_id,
                ev.produced_by.activity_id.as_deref().unwrap_or("-"),
                ev.unix_time,
            ))
        }
        Some("delete") => {
            let id = client
                .resolve(rest.get(1).ok_or("delete needs an id")?)
                .map_err(|e| e.to_string())?;
            client.delete(&id).map_err(|e| e.to_string())?;
            Ok(format!("deleted {id}\n"))
        }
        Some("leave") => {
            let id = client
                .resolve(rest.get(1).ok_or("leave needs an id")?)
                .map_err(|e| e.to_string())?;
            let dest = PathBuf::from(rest.get(2).ok_or("leave needs a dest dir")?);
            let out = client.leave(&id, &dest).map_err(|e| e.to_string())?;
            Ok(format!("{}\n", out.display()))
        }
        Some("timestamp") => {
            let id = client
                .resolve(rest.get(1).ok_or("timestamp needs an id")?)
                .map_err(|e| e.to_string())?;
            let path = client.timestamp(&id).map_err(|e| e.to_string())?;
            Ok(format!("timestamp {}\n", path.display()))
        }
        Some("current") => {
            let id = client
                .resolve(rest.get(1).ok_or("current needs an id")?)
                .map_err(|e| e.to_string())?;
            let d = client.current(&id).map_err(|e| e.to_string())?;
            Ok(format_one(&d))
        }
        Some("migrate") => {
            client.migrate().map_err(|e| e.to_string())?;
            Ok("layout=1\n".into())
        }
        Some("url") => Ok(format!(
            "{}\n",
            StoreUrl::parse(&url).map_err(|e| e.to_string())?.as_url()
        )),
        Some(other) => Err(format!("unknown command {other}")),
        None => Err(
            "usage: deedar [--url FILE] create|get|list|trail|evidence|delete|leave|timestamp|current|migrate\n\
             evidence and current take one id, several ids, or - to read them from stdin"
                .into(),
        ),
    }
}

/// Where a seat keeps its store when nobody says otherwise.
///
/// `$XDG_DATA_HOME/deedar/store`, or `~/.local/share/deedar/store`.
fn default_store() -> Option<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(base.join("deedar").join("store"))
}

/// The store to act on: `--url`, then `DEEDAR_URL`, then the seat's own.
///
/// The fallback is taken only when that store is already there. Creating one
/// wherever a command happened to run is how a seat ends up with two, and a
/// citation that resolves against one and not the other is worse than a
/// command that refused. So an absent store still refuses, and names the path
/// it would have used.
fn take_url(args: &[String]) -> Result<(String, Vec<String>), String> {
    let mut url = env::var("DEEDAR_URL").unwrap_or_default();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--url" {
            url = args.get(i + 1).ok_or("--url needs a value")?.clone();
            i += 2;
            continue;
        }
        rest.push(args[i].clone());
        i += 1;
    }
    if url.is_empty() {
        let seat = default_store();
        match seat {
            Some(dir) if dir.is_dir() => url = format!("file://{}", dir.display()),
            Some(dir) => {
                return Err(format!(
                    "set DEEDAR_URL or pass --url; no store at {}",
                    dir.display()
                ))
            }
            None => return Err("set DEEDAR_URL or pass --url".into()),
        }
    }
    Ok((url, rest))
}

fn cmd_create(client: &mut Client, args: &[String]) -> Result<String, String> {
    let kind = args.first().ok_or("create needs a kind")?;
    let mut name = String::new();
    let mut id = None;
    let mut excerpt = String::new();
    let mut edition = String::new();
    let mut range_start = 0u64;
    let mut range_end = 0u64;
    let mut urls = Vec::new();
    let mut members = Vec::new();
    let mut tree = String::new();
    let mut diffs = Vec::new();
    let mut functionaries = Vec::new();
    let mut path = PathBuf::new();
    let mut message_id = String::new();
    let mut in_reply_to = None;
    let mut subject = String::new();
    let mut title = String::new();
    let mut agent = String::new();
    let mut activity = None;
    let mut grants = Vec::new();
    let mut sources = Vec::new();
    let mut media = None;
    let mut clip_sources = Vec::new();
    let mut in_point = 0.0f64;
    let mut out_point = 0.0f64;
    let mut duration = 0.0f64;
    let mut page_url = String::new();
    let mut snapshot = PathBuf::new();
    let mut blank = String::new();
    let mut fields = Vec::new();
    let mut measures = Vec::new();
    let mut steps = Vec::new();
    let mut when = String::new();
    let mut where_ = String::new();
    let mut who = String::new();
    let mut supersedes = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--name" => name = need(args, &mut i)?,
            "--id" => id = Some(DeedId::parse(&need(args, &mut i)?).map_err(|e| e.to_string())?),
            "--excerpt" => excerpt = need(args, &mut i)?,
            "--edition" => edition = need(args, &mut i)?,
            "--start" => range_start = need(args, &mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--end" => range_end = need(args, &mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--src-url" => urls.push(need(args, &mut i)?),
            "--member" => members.push(PathBuf::from(need(args, &mut i)?)),
            "--tree" => tree = need(args, &mut i)?,
            "--diff" => diffs.push(PathBuf::from(need(args, &mut i)?)),
            "--functionary" => functionaries.push(need(args, &mut i)?),
            "--path" => path = PathBuf::from(need(args, &mut i)?),
            "--media" => media = Some(need(args, &mut i)?),
            "--message-id" => message_id = need(args, &mut i)?,
            "--in-reply-to" => in_reply_to = Some(need(args, &mut i)?),
            "--subject" => subject = need(args, &mut i)?,
            "--title" => title = need(args, &mut i)?,
            "--agent" => agent = need(args, &mut i)?,
            "--activity" => activity = Some(need(args, &mut i)?),
            "--grant-host" => grants.push(Grant::Host(need(args, &mut i)?)),
            "--grant-path" => grants.push(Grant::Path(PathBuf::from(need(args, &mut i)?))),
            "--input" => {
                let mid = DeedId::parse(&need(args, &mut i)?).map_err(|e| e.to_string())?;
                sources.push(Source::deed(mid));
            }
            "--clip-src" => clip_sources.push(PathBuf::from(need(args, &mut i)?)),
            "--in" => in_point = need(args, &mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--out" => out_point = need(args, &mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--duration" => duration = need(args, &mut i)?.parse().map_err(|e| format!("{e}"))?,
            "--page-url" => page_url = need(args, &mut i)?,
            "--snapshot" => snapshot = PathBuf::from(need(args, &mut i)?),
            "--blank" => blank = need(args, &mut i)?,
            "--field" => {
                let raw = need(args, &mut i)?;
                let (n, v) = raw.split_once('=').ok_or("--field name=value")?;
                fields.push(FormField {
                    name: n.into(),
                    value: v.into(),
                });
            }
            "--measure" => {
                let raw = need(args, &mut i)?;
                let (n, v) = raw.split_once('=').ok_or("--measure name=value")?;
                measures.push(Measure {
                    name: n.into(),
                    value: v.into(),
                });
            }
            "--step" => steps.push(Step {
                text: need(args, &mut i)?,
                done: false,
            }),
            "--when" => when = need(args, &mut i)?,
            "--where" => where_ = need(args, &mut i)?,
            "--who" => who = need(args, &mut i)?,
            "--supersedes" => {
                supersedes = Some(DeedId::parse(&need(args, &mut i)?).map_err(|e| e.to_string())?)
            }
            other => return Err(format!("unknown create flag {other}")),
        }
        i += 1;
    }
    if agent.is_empty() {
        return Err("--agent is required".into());
    }
    if name.is_empty() {
        name = kind.clone();
    }
    if edition.is_empty() && !urls.is_empty() {
        edition = urls[0].clone();
    }
    if range_end == 0 && !excerpt.is_empty() {
        range_end = excerpt.len() as u64;
    }
    let kind = Kind::parse(kind).map_err(|e| e.to_string())?;
    let body = match kind {
        Kind::File => Body::File {
            path,
            media_type: media,
        },
        Kind::Set => Body::Set {
            title: if title.is_empty() {
                name.clone()
            } else {
                title
            },
            members,
        },
        Kind::Quote => Body::Quote {
            edition,
            start: range_start,
            end: range_end,
            excerpt,
            urls,
        },
        Kind::Patch => Body::Patch {
            tree,
            diffs,
            functionaries: if functionaries.is_empty() {
                vec![agent.clone()]
            } else {
                functionaries
            },
        },
        Kind::MailDraft => Body::MailDraft {
            message_id: MailMessageId::parse(&message_id).map_err(|e| e.to_string())?,
            in_reply_to: in_reply_to
                .as_deref()
                .map(MailMessageId::parse)
                .transpose()
                .map_err(|e| e.to_string())?,
            subject,
            path,
        },
        Kind::Clip => Body::Clip {
            sources: clip_sources,
            in_point,
            out_point,
            duration,
            path,
        },
        Kind::Page => Body::Page {
            url: page_url,
            snapshot,
        },
        Kind::Form => Body::Form {
            blank,
            fields,
            path,
        },
        Kind::Table => Body::Table { measures },
        Kind::Procedure => Body::Procedure { steps },
        Kind::Event => Body::Event { when, where_, who },
    };
    let (deed, _) = client
        .create(CreateRequest {
            id,
            name,
            sources,
            agent_id: agent,
            activity_id: activity,
            grants,
            body,
            supersedes,
        })
        .map_err(|e| e.to_string())?;
    Ok(format_one(&deed))
}

fn need(args: &[String], i: &mut usize) -> Result<String, String> {
    let v = args.get(*i + 1).ok_or("flag needs a value")?;
    *i += 1;
    Ok(v.clone())
}

fn format_one(d: &Deed) -> String {
    let mut out = format!(
        "id={} kind={} name={}\npaths={}\nsources={}\nproducedBy={} {}\ngrants={}\nface={}\n",
        d.id,
        d.kind.token(),
        d.name,
        join_paths(&d.paths),
        join_sources(&d.sources),
        d.produced_by.agent_id,
        d.produced_by.activity_id.as_deref().unwrap_or("-"),
        join_grants(&d.grants),
        d.face.as_str(),
    );
    match &d.body {
        Body::Quote {
            excerpt,
            urls,
            edition,
            start,
            end,
        } => {
            out.push_str(&format!(
                "edition={edition} range={start}-{end}\nexcerpt={excerpt}\nquoteUrls={}\n",
                urls.join(",")
            ));
        }
        Body::Set { title, members } => {
            out.push_str(&format!("title={title}\nmembers={}\n", join_paths(members)));
        }
        Body::Patch {
            tree,
            diffs,
            functionaries,
        } => {
            out.push_str(&format!(
                "tree={tree}\ndiffs={}\nfunctionaries={}\n",
                join_paths(diffs),
                functionaries.join(",")
            ));
        }
        Body::MailDraft {
            message_id,
            in_reply_to,
            subject,
            ..
        } => {
            out.push_str(&format!(
                "messageId={}\ninReplyTo={}\nsubject={subject}\n",
                message_id.as_str(),
                in_reply_to
                    .as_ref()
                    .map(MailMessageId::as_str)
                    .unwrap_or("-"),
            ));
        }
        Body::File { path, .. } => {
            out.push_str(&format!("file={}\n", path.display()));
        }
        Body::Clip {
            in_point,
            out_point,
            duration,
            path,
            ..
        } => {
            out.push_str(&format!(
                "clip={} in={in_point} out={out_point} duration={duration}\n",
                path.display()
            ));
        }
        Body::Page { url, snapshot } => {
            out.push_str(&format!("page={url} snapshot={}\n", snapshot.display()));
        }
        Body::Form { blank, path, .. } => {
            out.push_str(&format!("form={blank} path={}\n", path.display()));
        }
        Body::Table { measures } => {
            out.push_str(&format!(
                "measures={}\n",
                measures
                    .iter()
                    .map(|m| format!("{}={}", m.name, m.value))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        Body::Procedure { steps } => {
            out.push_str(&format!("steps={}\n", steps.len()));
        }
        Body::Event { when, where_, who } => {
            out.push_str(&format!("when={when} where={where_} who={who}\n"));
        }
    }
    out
}

fn format_list(rows: &[Deed]) -> String {
    let mut out = String::new();
    for d in rows {
        out.push_str(&format!("{}\n", d.label()));
        out.push_str(&format_one(d));
        out.push('\n');
    }
    out
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn join_sources(sources: &[Source]) -> String {
    sources
        .iter()
        .map(|s| match s {
            Source::Deed(id) => format!("deed:{id}"),
            Source::Path(p) => format!("path:{}", p.display()),
            Source::Url(u) => format!("url:{u}"),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn join_grants(grants: &[Grant]) -> String {
    grants
        .iter()
        .map(|g| match g {
            Grant::Host(h) => format!("host:{h}"),
            Grant::Path(p) => format!("path:{}", p.display()),
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod url_tests {
    use super::*;

    /// Point HOME and XDG at a scratch directory for one closure.
    fn with_home<T>(
        dir: &std::path::Path,
        xdg: Option<&std::path::Path>,
        f: impl FnOnce() -> T,
    ) -> T {
        let old_home = env::var_os("HOME");
        let old_xdg = env::var_os("XDG_DATA_HOME");
        let old_url = env::var_os("DEEDAR_URL");
        env::remove_var("DEEDAR_URL");
        env::set_var("HOME", dir);
        match xdg {
            Some(path) => env::set_var("XDG_DATA_HOME", path),
            None => env::remove_var("XDG_DATA_HOME"),
        }
        let out = f();
        match old_home {
            Some(v) => env::set_var("HOME", v),
            None => env::remove_var("HOME"),
        }
        match old_xdg {
            Some(v) => env::set_var("XDG_DATA_HOME", v),
            None => env::remove_var("XDG_DATA_HOME"),
        }
        if let Some(v) = old_url {
            env::set_var("DEEDAR_URL", v);
        }
        out
    }

    #[test]
    fn an_explicit_url_wins_over_everything() {
        let dir = tempfile::tempdir().unwrap();
        with_home(dir.path(), None, || {
            let args = vec![
                "--url".to_string(),
                "file:///somewhere".to_string(),
                "list".into(),
            ];
            let (url, rest) = take_url(&args).unwrap();
            assert_eq!(url, "file:///somewhere");
            assert_eq!(rest, vec!["list".to_string()]);
        });
    }

    #[test]
    fn the_seat_store_is_used_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join(".local/share/deedar/store");
        std::fs::create_dir_all(&store).unwrap();
        with_home(dir.path(), None, || {
            let (url, _) = take_url(&["list".to_string()]).unwrap();
            assert_eq!(url, format!("file://{}", store.display()));
        });
    }

    #[test]
    fn xdg_moves_the_seat_store() {
        let dir = tempfile::tempdir().unwrap();
        let xdg = dir.path().join("data");
        let store = xdg.join("deedar/store");
        std::fs::create_dir_all(&store).unwrap();
        with_home(dir.path(), Some(&xdg), || {
            let (url, _) = take_url(&["list".to_string()]).unwrap();
            assert_eq!(url, format!("file://{}", store.display()));
        });
    }

    #[test]
    fn no_store_refuses_and_names_the_path_it_wanted() {
        // Creating one wherever a command happened to run is how a seat ends
        // up with two stores, and a citation resolving against one and not the
        // other is worse than a command that refused.
        let dir = tempfile::tempdir().unwrap();
        with_home(dir.path(), None, || {
            let err = take_url(&["list".to_string()]).unwrap_err();
            assert!(err.contains("no store at"), "{err}");
            assert!(err.contains(".local/share/deedar/store"), "{err}");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{dispatch, run};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_url() -> String {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("deedar-cli-{n}"));
        std::fs::create_dir_all(&dir).expect("dir");
        format!("file://{}", dir.display())
    }

    #[test]
    fn cli_creates_quote_and_gets_it() {
        let url = tmp_url();
        let quote = run(vec![
            "--url".into(),
            url.clone(),
            "create".into(),
            "quote".into(),
            "--id".into(),
            "deed-quote-rfc2094-nll".into(),
            "--name".into(),
            "NLL: lifetimes from the CFG".into(),
            "--excerpt".into(),
            "lifetimes that are based on the control-flow graph".into(),
            "--src-url".into(),
            "https://rust-lang.github.io/rfcs/2094-nll.html".into(),
            "--agent".into(),
            "reader".into(),
        ])
        .expect("create quote");
        assert!(quote.contains("deed-quote-rfc2094-nll"), "{quote}");
        assert!(quote.contains("kind=quote"), "{quote}");
        assert!(
            quote.contains("excerpt=lifetimes that are based on the control-flow graph"),
            "{quote}"
        );

        let got = run(vec![
            "--url".into(),
            url.clone(),
            "get".into(),
            "deed-quote-rfc2094-nll".into(),
        ])
        .expect("get");
        assert!(got.contains("deed-quote-rfc2094-nll"), "{got}");
        assert!(got.contains("kind=quote"), "{got}");
        assert!(
            got.contains("excerpt=lifetimes that are based on the control-flow graph"),
            "{got}"
        );

        let trail = run(vec![
            "--url".into(),
            url,
            "trail".into(),
            "deed-quote-rfc2094-nll".into(),
        ])
        .expect("trail");
        assert!(trail.contains("deed-quote-rfc2094-nll"), "{trail}");
    }

    /// A working set arrives as several ids at once, and the answer a caller
    /// needs is which of them failed rather than that one did.
    #[test]
    fn evidence_over_several_ids_reports_each_and_fails_on_any() {
        let url = tmp_url();
        for slug in ["deed-quote-first", "deed-quote-second"] {
            run(vec![
                "--url".into(),
                url.clone(),
                "create".into(),
                "quote".into(),
                "--id".into(),
                slug.into(),
                "--excerpt".into(),
                "a range in a frozen edition".into(),
                "--src-url".into(),
                "https://example.invalid/edition".into(),
                "--agent".into(),
                "reader".into(),
            ])
            .expect("create quote");
        }

        let both = dispatch(vec![
            "--url".into(),
            url.clone(),
            "evidence".into(),
            "deed-quote-first".into(),
            "deed-quote-second".into(),
        ])
        .expect("evidence over two ids");
        assert!(both.ok, "{}", both.text);
        assert!(both.text.contains("deed-quote-first ok"), "{}", both.text);
        assert!(both.text.contains("2 of 2 verified"), "{}", both.text);

        // One bad id in a set makes the set untrustworthy, and the report has
        // to name which one rather than stopping at it.
        let mixed = dispatch(vec![
            "--url".into(),
            url,
            "evidence".into(),
            "deed-quote-first".into(),
            "deed-quote-absent".into(),
            "deed-quote-second".into(),
        ])
        .expect("evidence over a set holding one bad id");
        assert!(!mixed.ok, "{}", mixed.text);
        assert!(
            mixed.text.contains("deed-quote-absent FAILED"),
            "{}",
            mixed.text
        );
        assert!(
            mixed.text.contains("deed-quote-second ok"),
            "a bad id must not stop the rest of the set: {}",
            mixed.text
        );
        assert!(mixed.text.contains("2 of 3 verified"), "{}", mixed.text);
    }

    /// A citation is written once and the thing it names can be superseded
    /// afterwards. Whatever holds the citations cannot notice that on its own,
    /// so the set form is what tells it which have moved on.
    #[test]
    fn current_over_a_set_names_the_ones_that_moved() {
        let url = tmp_url();
        let quote = |slug: &str, extra: Vec<String>| {
            let mut argv = vec![
                "--url".into(),
                url.clone(),
                "create".into(),
                "quote".into(),
                "--id".into(),
                slug.to_string(),
                "--excerpt".into(),
                "a range in a frozen edition".into(),
                "--src-url".into(),
                "https://example.invalid/edition".into(),
                "--agent".into(),
                "reader".into(),
            ];
            argv.extend(extra);
            run(argv).expect("create quote");
        };
        quote("deed-quote-stable", vec![]);
        quote("deed-quote-first", vec![]);
        quote(
            "deed-quote-second",
            vec!["--supersedes".into(), "deed-quote-first".into()],
        );

        let report = dispatch(vec![
            "--url".into(),
            url,
            "current".into(),
            "deed-quote-stable".into(),
            "deed-quote-first".into(),
        ])
        .expect("current over a set");
        assert!(!report.ok, "one of them moved: {}", report.text);
        assert!(
            report.text.contains("deed-quote-stable current"),
            "{}",
            report.text
        );
        assert!(
            report
                .text
                .contains("deed-quote-first SUPERSEDED by deed-quote-second"),
            "the citation has to name what to cite instead: {}",
            report.text
        );
        assert!(report.text.contains("1 of 2 current"), "{}", report.text);
    }

    /// One id is still the single-deed report it always was, so a caller that
    /// parses that output keeps working.
    #[test]
    fn evidence_over_one_id_keeps_its_own_shape() {
        let url = tmp_url();
        run(vec![
            "--url".into(),
            url.clone(),
            "create".into(),
            "quote".into(),
            "--id".into(),
            "deed-quote-alone".into(),
            "--excerpt".into(),
            "a range in a frozen edition".into(),
            "--src-url".into(),
            "https://example.invalid/edition".into(),
            "--agent".into(),
            "reader".into(),
        ])
        .expect("create quote");

        let one = dispatch(vec![
            "--url".into(),
            url,
            "evidence".into(),
            "deed-quote-alone".into(),
        ])
        .expect("evidence over one id");
        assert!(one.ok);
        assert!(one.text.contains("id=deed-quote-alone ok"), "{}", one.text);
        assert!(
            !one.text.contains("verified"),
            "the single-deed report has no set summary: {}",
            one.text
        );
    }

    #[test]
    fn cli_get_accepts_deed_digest() {
        let url = tmp_url();
        let created = run(vec![
            "--url".into(),
            url.clone(),
            "create".into(),
            "quote".into(),
            "--id".into(),
            "deed-quote-digest-alias".into(),
            "--name".into(),
            "digest alias".into(),
            "--excerpt".into(),
            "canonical bytes have a digest".into(),
            "--src-url".into(),
            "https://example.com/digest".into(),
            "--agent".into(),
            "reader".into(),
        ])
        .expect("create quote");
        assert!(created.contains("deed-quote-digest-alias"), "{created}");

        let mut client = deedar::Client::open(&url).expect("open");
        let deed = client
            .get(&deed::DeedId::parse("deed-quote-digest-alias").unwrap())
            .expect("load");
        let digest = deedar::deed_digest(&deed);
        let got = run(vec!["--url".into(), url, "get".into(), digest]).expect("get digest");
        assert!(got.contains("deed-quote-digest-alias"), "{got}");
        assert!(
            got.contains("excerpt=canonical bytes have a digest"),
            "{got}"
        );
    }

    #[test]
    fn cli_evidence_accepts_a_created_file() {
        let url = tmp_url();
        let dir = std::path::PathBuf::from(url.trim_start_matches("file://"));
        let letter = dir.join("letter.pdf");
        std::fs::write(&letter, b"%PDF-letter").expect("blob");
        run(vec![
            "--url".into(),
            url.clone(),
            "create".into(),
            "file".into(),
            "--id".into(),
            "deed-file-letter".into(),
            "--name".into(),
            "Hospital letter".into(),
            "--path".into(),
            letter.display().to_string(),
            "--media".into(),
            "application/pdf".into(),
            "--agent".into(),
            "reader".into(),
        ])
        .expect("create file");
        let ev = run(vec![
            "--url".into(),
            url,
            "evidence".into(),
            "deed-file-letter".into(),
        ])
        .expect("evidence");
        assert!(ev.contains("id=deed-file-letter"), "{ev}");
        assert!(ev.contains("ok"), "{ev}");
        assert!(ev.contains("producedBy=reader"), "{ev}");
    }
}
