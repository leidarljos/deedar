//! Operator path: create, get, list, trail, evidence, delete, leave, timestamp, current, migrate.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use deed::{Body, Deed, DeedId, FormField, Grant, Kind, MailMessageId, Measure, Source, Step};
use deeder::{Client, CreateRequest, StoreUrl};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("deeder: {e}");
            ExitCode::from(1)
        }
    }
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
            "usage: deeder [--url FILE] create|get|list|trail|evidence|delete|leave|timestamp|current|migrate"
                .into(),
        ),
    }
}

fn take_url(args: &[String]) -> Result<(String, Vec<String>), String> {
    let mut url = env::var("DEEDER_URL").unwrap_or_default();
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
        return Err("set DEEDER_URL or pass --url".into());
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
mod tests {
    use super::run;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_url() -> String {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("deeder-cli-{n}"));
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

        let mut client = deeder::Client::open(&url).expect("open");
        let deed = client
            .get(&deed::DeedId::parse("deed-quote-digest-alias").unwrap())
            .expect("load");
        let digest = deeder::deed_digest(&deed);
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
