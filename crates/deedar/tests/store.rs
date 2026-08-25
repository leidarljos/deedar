use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use deed::{Body, DeedId, Error, FormField, Grant, Kind, MailMessageId, Measure, Source, Step};
use deedar::{encode_response, is_write_once_addr, Client, CreateRequest, FsStore, Response};

fn tmp_url() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("deedar-store-{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    format!("file://{}", dir.display())
}

fn write_blob(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn req(id: &str, name: &str, body: Body) -> CreateRequest {
    CreateRequest {
        id: Some(DeedId::parse(id).unwrap()),
        name: name.into(),
        sources: Vec::new(),
        agent_id: "reader".into(),
        activity_id: Some("aa02".into()),
        grants: vec![Grant::Host("www.iea.org".into())],
        body,
        supersedes: None,
    }
}

#[test]
fn create_get_list_trail_delete_for_every_kind() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let letter = write_blob(&root, "letter.pdf", b"%PDF-letter");
    let shot = write_blob(&root, "museum.jpg", b"jpeg-bytes");
    let diff = write_blob(&root, "parser.rs", b"fn parse() {}");
    let eml = write_blob(&root, "maya.eml", b"From: desk");
    let wav = write_blob(&root, "sting.wav", b"RIFF");
    let snap = write_blob(&root, "year.html", b"<html>");
    let slip = write_blob(&root, "slip.pdf", b"%PDF-slip");

    let mut client = Client::open(&url).expect("open");

    let (file, _) = client
        .create(req(
            "deed-file-letter",
            "Hospital letter",
            Body::File {
                path: letter,
                media_type: Some("application/pdf".into()),
            },
        ))
        .expect("file");
    assert_eq!(file.kind, Kind::File);
    assert_eq!(file.face.as_str(), "document");
    assert!(
        file.paths.iter().all(is_write_once_addr),
        "{:?}",
        file.paths
    );

    let (set, _) = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-set-covers").unwrap()),
            name: "Cover shots".into(),
            sources: vec![Source::deed(file.id.clone())],
            agent_id: "reader".into(),
            activity_id: Some("aa02".into()),
            grants: vec![Grant::Path(PathBuf::from("/shots"))],
            body: Body::Set {
                title: "Cover shots".into(),
                members: vec![shot],
            },
            supersedes: None,
        })
        .expect("set");
    assert_eq!(set.kind, Kind::Set);
    assert_eq!(set.face.as_str(), "sheet");
    assert!(set.paths.iter().all(is_write_once_addr));

    let (quote, _) = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-quote-heatpump").unwrap()),
            name: "Heat-pump piece".into(),
            sources: vec![Source::deed(set.id.clone())],
            agent_id: "reader".into(),
            activity_id: Some("aa02".into()),
            grants: vec![Grant::Host("www.theatlantic.com".into())],
            body: Body::Quote {
                edition: "https://www.iea.org/energy-system/buildings/heat-pumps".into(),
                start: 0,
                end: 48,
                excerpt: "A heat pump moves heat; it does not make it.".into(),
                urls: vec!["https://www.iea.org/energy-system/buildings/heat-pumps".into()],
            },
            supersedes: None,
        })
        .expect("quote");
    assert_eq!(quote.kind, Kind::Quote);
    match &quote.body {
        Body::Quote {
            excerpt,
            start,
            end,
            ..
        } => {
            assert_eq!(excerpt, "A heat pump moves heat; it does not make it.");
            assert_eq!((*start, *end), (0, 48));
        }
        other => panic!("{other:?}"),
    }

    let (patch, _) = client
        .create(req(
            "deed-patch-clap",
            "clap empty-value",
            Body::Patch {
                tree: "github.com/clap-rs/clap".into(),
                diffs: vec![diff],
                functionaries: vec!["implementor".into()],
            },
        ))
        .expect("patch");
    assert_eq!(patch.kind, Kind::Patch);

    let (mail, _) = client
        .create(req(
            "deed-mailDraft-maya",
            "Reply to Maya",
            Body::MailDraft {
                message_id: MailMessageId::parse("<desk-maya-1@praxis.local>").unwrap(),
                in_reply_to: Some(MailMessageId::parse("maya@example.com").unwrap()),
                subject: "Re: Thursday".into(),
                path: eml,
            },
        ))
        .expect("mail");
    assert_eq!(mail.kind, Kind::MailDraft);
    assert_eq!(mail.face.as_str(), "letter");

    let (clip, _) = client
        .create(req(
            "deed-clip-sting",
            "Thirty-second sting",
            Body::Clip {
                sources: vec![wav.clone()],
                in_point: 0.0,
                out_point: 30.0,
                duration: 30.0,
                path: wav,
            },
        ))
        .expect("clip");
    assert_eq!(clip.kind, Kind::Clip);
    assert_eq!(clip.face.as_str(), "waveform");
    match &clip.body {
        Body::Clip {
            in_point,
            out_point,
            ..
        } => assert_eq!((*in_point, *out_point), (0.0, 30.0)),
        other => panic!("{other:?}"),
    }

    let (page, _) = client
        .create(req(
            "deed-page-year",
            "Year 4 page",
            Body::Page {
                url: "https://oakhill.sch.uk/parents/year-4".into(),
                snapshot: snap,
            },
        ))
        .expect("page");
    assert_eq!(page.kind, Kind::Page);

    let (form, _) = client
        .create(req(
            "deed-form-slip",
            "Museum slip",
            Body::Form {
                blank: "science-museum-slip".into(),
                fields: vec![FormField {
                    name: "child".into(),
                    value: "Sam".into(),
                }],
                path: slip,
            },
        ))
        .expect("form");
    assert_eq!(form.kind, Kind::Form);

    let (table, _) = client
        .create(req(
            "deed-table-fairing",
            "Fairing numbers",
            Body::Table {
                measures: vec![Measure {
                    name: "dish".into(),
                    value: "3.80 m".into(),
                }],
            },
        ))
        .expect("table");
    assert_eq!(table.kind, Kind::Table);

    let (procedure, _) = client
        .create(req(
            "deed-procedure-week",
            "This week",
            Body::Procedure {
                steps: vec![Step {
                    text: "Sign the slip".into(),
                    done: false,
                }],
            },
        ))
        .expect("procedure");
    assert_eq!(procedure.kind, Kind::Procedure);

    let (event, _) = client
        .create(req(
            "deed-event-trip",
            "Friday trip",
            Body::Event {
                when: "Friday".into(),
                where_: "Science Museum".into(),
                who: "Year 4".into(),
            },
        ))
        .expect("event");
    assert_eq!(event.kind, Kind::Event);

    let listed = client.list().expect("list");
    assert_eq!(listed.len(), 11, "{}", listed.len());

    let fetched = client.get(&quote.id).expect("get");
    assert_eq!(fetched.id.as_str(), "deed-quote-heatpump");

    let trail = client.trail(&quote.id).expect("trail");
    let trail_ids: Vec<_> = trail.iter().map(|d| d.id.as_str().to_string()).collect();
    assert_eq!(trail_ids[0], "deed-quote-heatpump");
    assert!(trail_ids.contains(&"deed-set-covers".to_string()));
    assert!(trail_ids.contains(&"deed-file-letter".to_string()));

    client.delete(&set.id).expect("delete");
    assert!(matches!(
        client.get(&set.id),
        Err(deed::Error::Tombstoned(_))
    ));
    let after = client.list().unwrap();
    assert!(!after.iter().any(|d| d.id.as_str() == "deed-set-covers"));

    let err = client
        .create(req(
            "deed-set-covers",
            "again",
            Body::Set {
                title: "no".into(),
                members: vec![PathBuf::from("/x")],
            },
        ))
        .expect_err("frozen");
    assert!(
        matches!(err, deed::Error::Frozen(_)) || err.to_string().contains("frozen"),
        "{err}"
    );
}

#[test]
fn later_take_is_new_accession_with_prior_in_sources() {
    let mut client = Client::open(&tmp_url()).unwrap();
    let (first, _) = client
        .create(req(
            "deed-quote-heatpump",
            "v1",
            Body::Quote {
                edition: "ed-1".into(),
                start: 0,
                end: 4,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/energy-system/buildings/heat-pumps".into()],
            },
        ))
        .unwrap();
    let (second, _) = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-quote-heatpump-v2").unwrap()),
            name: "v2".into(),
            sources: vec![Source::deed(first.id.clone())],
            agent_id: "reader".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::Quote {
                edition: "ed-1".into(),
                start: 0,
                end: 10,
                excerpt: "heat pump".into(),
                urls: vec!["https://www.iea.org/energy-system/buildings/heat-pumps".into()],
            },
            supersedes: None,
        })
        .unwrap();
    assert_ne!(second.id, first.id);
    assert!(second
        .sources
        .iter()
        .any(|s| matches!(s, Source::Deed(id) if id == &first.id)));
    let trail = client.trail(&second.id).unwrap();
    assert!(trail.iter().any(|d| d.id == first.id));
}

#[test]
fn writer_stamps_agent_and_clock_not_caller_time() {
    let url = tmp_url();
    let mut client = Client::open(&url).unwrap();
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (deed, evidence) = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-quote-heatpump").unwrap()),
            name: "Heat".into(),
            sources: Vec::new(),
            agent_id: "seat-agent".into(),
            activity_id: Some("act-1".into()),
            grants: vec![Grant::Host("www.iea.org".into())],
            body: Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 1,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/x".into()],
            },
            supersedes: None,
        })
        .unwrap();
    let after = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(deed.produced_by.agent_id, "seat-agent");
    assert_eq!(deed.produced_by.activity_id.as_deref(), Some("act-1"));
    assert_eq!(evidence.produced_by.agent_id, "seat-agent");
    assert!(evidence.unix_time >= before && evidence.unix_time <= after);
    assert!(!evidence.signature.is_empty());

    let (deed2, evidence2) = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-quote-second").unwrap()),
            name: "Second".into(),
            sources: Vec::new(),
            agent_id: "other".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 1,
                excerpt: "more".into(),
                urls: vec!["https://www.iea.org/y".into()],
            },
            supersedes: None,
        })
        .unwrap();
    assert_eq!(deed2.produced_by.agent_id, "other");
    assert_eq!(deed2.produced_by.activity_id, None);
    assert_eq!(evidence2.produced_by.activity_id, None);
}

#[test]
fn capnp_client_round_trip_quote_and_clip() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let wav = write_blob(&root, "sting.wav", b"RIFF");
    let mut client = Client::open(&url).unwrap();
    let (quote, _) = client
        .create(req(
            "deed-quote-heatpump",
            "Heat-pump piece",
            Body::Quote {
                edition: "ed".into(),
                start: 2,
                end: 22,
                excerpt: "A heat pump moves heat".into(),
                urls: vec!["https://www.iea.org/energy-system/buildings/heat-pumps".into()],
            },
        ))
        .unwrap();
    let got = client.get(&quote.id).unwrap();
    assert_eq!(got.id.as_str(), "deed-quote-heatpump");
    assert_eq!(got.kind, Kind::Quote);
    match &got.body {
        Body::Quote {
            excerpt,
            start,
            end,
            ..
        } => {
            assert_eq!(excerpt, "A heat pump moves heat");
            assert_eq!((*start, *end), (2, 22));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(got.face.as_str(), "document");

    let (clip, _) = client
        .create(req(
            "deed-clip-sting",
            "sting",
            Body::Clip {
                sources: vec![wav.clone()],
                in_point: 1.5,
                out_point: 31.5,
                duration: 30.0,
                path: wav,
            },
        ))
        .unwrap();
    let got = client.get(&clip.id).unwrap();
    assert_eq!(got.kind, Kind::Clip);
    match &got.body {
        Body::Clip {
            in_point,
            out_point,
            ..
        } => assert_eq!((*in_point, *out_point), (1.5, 31.5)),
        other => panic!("{other:?}"),
    }
    assert_eq!(got.face.as_str(), "waveform");
}

#[test]
fn open_file_url_round_trips() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let note = write_blob(&root, "note.md", b"a short note\n");
    let mut a = Client::open(&url).unwrap();
    a.create(req(
        "deed-file-note",
        "Note",
        Body::File {
            path: note,
            media_type: Some("text/markdown".into()),
        },
    ))
    .unwrap();
    let mut b = Client::open(&url).unwrap();
    let got = b.get(&DeedId::parse("deed-file-note").unwrap()).unwrap();
    assert_eq!(got.name, "Note");
    assert_eq!(got.kind, Kind::File);
    assert!(got.paths.iter().all(is_write_once_addr));
}

#[test]
fn quote_without_urls_is_rejected() {
    let mut client = Client::open(&tmp_url()).unwrap();
    let err = client
        .create(CreateRequest {
            id: None,
            name: "empty".into(),
            sources: Vec::new(),
            agent_id: "reader".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 1,
                excerpt: "no sources".into(),
                urls: Vec::new(),
            },
            supersedes: None,
        })
        .expect_err("empty urls");
    assert!(err.to_string().contains("url"), "{err}");
}

#[test]
fn evidence_accepts_fresh_file_and_rejects_changed_bytes() {
    let url = tmp_url();
    let root = deedar::store_dir(&url).unwrap();
    let letter = write_blob(&root, "letter.pdf", b"%PDF-letter");
    let mut client = Client::open(&url).unwrap();
    let id = DeedId::parse("deed-file-letter").unwrap();
    let (deed, issued) = client
        .create(req(
            "deed-file-letter",
            "Hospital letter",
            Body::File {
                path: letter,
                media_type: Some("application/pdf".into()),
            },
        ))
        .unwrap();
    let checked = client.evidence(&id).expect("fresh evidence");
    assert_eq!(checked.deed_id, deed.id);
    assert_eq!(checked.signature, issued.signature);
    assert_eq!(checked.produced_by.agent_id, "reader");

    let hex = deed.paths[0]
        .to_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .expect("write-once path");
    std::fs::write(root.join("bytes").join(hex), b"changed").unwrap();
    let err = client.evidence(&id).expect_err("changed bytes");
    assert!(err.to_string().contains("evidence"), "{err}");
}

#[test]
fn evidence_rejects_a_changed_deed_record() {
    let url = tmp_url();
    let mut client = Client::open(&url).unwrap();
    let id = DeedId::parse("deed-quote-heatpump").unwrap();
    client
        .create(req(
            "deed-quote-heatpump",
            "Heat",
            Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 4,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/x".into()],
            },
        ))
        .unwrap();
    client.evidence(&id).expect("fresh");
    let mut changed = client.get(&id).unwrap();
    changed.name = "tampered".into();
    let cap = deedar::store_dir(&url)
        .unwrap()
        .join("deeds")
        .join("deed-quote-heatpump.cap");
    std::fs::write(&cap, encode_response(&Response::Deed(changed)).unwrap()).unwrap();
    let err = client.evidence(&id).expect_err("changed deed");
    assert!(err.to_string().contains("evidence"), "{err}");
}

#[test]
fn create_rejects_a_body_path_that_is_not_a_file() {
    let mut client = Client::open(&tmp_url()).unwrap();
    let err = client
        .create(req(
            "deed-file-note",
            "Note",
            Body::File {
                path: PathBuf::from("/no/such/note.md"),
                media_type: Some("text/markdown".into()),
            },
        ))
        .expect_err("missing file");
    assert!(
        matches!(err, Error::Io(ref s) if s.contains("not a file"))
            || err.to_string().contains("not a file"),
        "{err}"
    );
}

#[test]
fn create_of_a_frozen_id_is_frozen() {
    let mut client = Client::open(&tmp_url()).unwrap();
    client
        .create(req(
            "deed-quote-heatpump",
            "Heat",
            Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 4,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/x".into()],
            },
        ))
        .unwrap();
    let err = client
        .create(req(
            "deed-quote-heatpump",
            "Heat",
            Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 4,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/x".into()],
            },
        ))
        .expect_err("frozen");
    assert!(matches!(err, Error::Frozen(_)), "{err}");
}

#[test]
fn list_fails_on_a_corrupt_deed_file() {
    let url = tmp_url();
    let mut client = Client::open(&url).unwrap();
    client
        .create(req(
            "deed-quote-heatpump",
            "Heat",
            Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 4,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/x".into()],
            },
        ))
        .unwrap();
    let cap = deedar::store_dir(&url)
        .unwrap()
        .join("deeds")
        .join("deed-quote-heatpump.cap");
    std::fs::write(&cap, b"not-a-deed").unwrap();
    let err = client.list().expect_err("corrupt");
    assert!(matches!(err, Error::Decode(_) | Error::Io(_)), "{err}");
}

#[test]
fn trail_fails_when_an_input_deed_is_tombstoned() {
    let url = tmp_url();
    let mut client = Client::open(&url).unwrap();
    let first = client
        .create(req(
            "deed-quote-heatpump",
            "Heat",
            Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 4,
                excerpt: "heat".into(),
                urls: vec!["https://www.iea.org/x".into()],
            },
        ))
        .unwrap()
        .0;
    let second = client
        .create(CreateRequest {
            id: Some(DeedId::parse("deed-quote-later").unwrap()),
            name: "Later".into(),
            sources: vec![Source::deed(first.id.clone())],
            agent_id: "reader".into(),
            activity_id: None,
            grants: Vec::new(),
            body: Body::Quote {
                edition: "ed".into(),
                start: 0,
                end: 4,
                excerpt: "more".into(),
                urls: vec!["https://www.iea.org/y".into()],
            },
            supersedes: None,
        })
        .unwrap()
        .0;
    client.delete(&first.id).unwrap();
    let err = client.trail(&second.id).expect_err("tombstoned source");
    assert!(matches!(err, Error::Tombstoned(_)), "{err}");
}

#[test]
fn fs_store_direct_create_uses_same_writer() {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("deedar-fs-{n}"));
    let mut store = FsStore::open(&dir).unwrap();
    let (d, _) = store
        .create(req(
            "deed-event-trip",
            "trip",
            Body::Event {
                when: "Friday".into(),
                where_: "Museum".into(),
                who: "Sam".into(),
            },
        ))
        .unwrap();
    assert_eq!(d.kind, Kind::Event);
}
