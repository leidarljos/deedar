use std::path::PathBuf;

use deed::{
    Body, Deed, DeedId, Draft, Error, Face, FormField, Grant, Kind, MailMessageId, Measure,
    ProducedBy, Source, Step,
};

fn agent(name: &str, activity: Option<&str>) -> ProducedBy {
    ProducedBy {
        agent_id: name.into(),
        activity_id: activity.map(str::to_string),
    }
}

fn draft(id: &str, name: &str, body: Body) -> Draft {
    Draft {
        id: Some(DeedId::parse(id).unwrap()),
        name: name.into(),
        sources: Vec::new(),
        produced_by: agent("reader", Some("aa02")),
        grants: vec![Grant::Host("www.iea.org".into())],
        body,
    }
}

#[test]
fn from_draft_fills_envelope_from_quote_body() {
    let deed = Deed::from_draft(draft(
        "deed-quote-heatpump",
        "Heat-pump piece",
        Body::Quote {
            edition: "https://www.iea.org/energy-system/buildings/heat-pumps".into(),
            start: 0,
            end: 40,
            excerpt: "A heat pump moves heat".into(),
            urls: vec!["https://www.iea.org/energy-system/buildings/heat-pumps".into()],
        },
    ))
    .unwrap();
    assert_eq!(deed.id.as_str(), "deed-quote-heatpump");
    assert_eq!(deed.kind, Kind::Quote);
    assert_eq!(deed.face, Face::Document);
    assert!(matches!(deed.sources[0], Source::Url(_)));
    assert_eq!(deed.label(), "quote · Heat-pump piece");
}

#[test]
fn assemble_every_design_kind_sets_face_and_deed_prefix() {
    let cases: Vec<(&str, Body, Face)> = vec![
        (
            "deed-file-letter",
            Body::File {
                path: PathBuf::from("/seat/letter.pdf"),
                media_type: Some("application/pdf".into()),
            },
            Face::Document,
        ),
        (
            "deed-file-source",
            Body::File {
                path: PathBuf::from("/seat/main.rs"),
                media_type: Some("text/x-rust".into()),
            },
            Face::Syntax,
        ),
        (
            "deed-file-rfc822",
            Body::File {
                path: PathBuf::from("/seat/note.eml"),
                media_type: Some("message/rfc822".into()),
            },
            Face::Letter,
        ),
        (
            "deed-set-covers",
            Body::Set {
                title: "Cover shots".into(),
                members: vec![PathBuf::from("/shots/a.jpg")],
            },
            Face::Sheet,
        ),
        (
            "deed-quote-heatpump",
            Body::Quote {
                edition: "ed-1".into(),
                start: 4,
                end: 20,
                excerpt: "excerpt".into(),
                urls: vec!["https://www.iea.org/energy-system/buildings/heat-pumps".into()],
            },
            Face::Document,
        ),
        (
            "deed-patch-clap",
            Body::Patch {
                tree: "github.com/clap-rs/clap".into(),
                diffs: vec![PathBuf::from("parser.rs")],
                functionaries: vec!["implementor".into()],
            },
            Face::Syntax,
        ),
        (
            "deed-mailDraft-maya",
            Body::MailDraft {
                message_id: MailMessageId::parse("<desk-maya-1@praxis.local>").unwrap(),
                in_reply_to: Some(MailMessageId::parse("maya@example.com").unwrap()),
                subject: "Re: Thursday".into(),
                path: PathBuf::from("drafts/maya.eml"),
            },
            Face::Letter,
        ),
        (
            "deed-clip-sting",
            Body::Clip {
                sources: vec![PathBuf::from("/mix/take.wav")],
                in_point: 0.0,
                out_point: 30.0,
                duration: 30.0,
                path: PathBuf::from("/mix/sting.wav"),
            },
            Face::Waveform,
        ),
        (
            "deed-page-year",
            Body::Page {
                url: "https://oakhill.sch.uk/parents/year-4".into(),
                snapshot: PathBuf::from("/snap/year.html"),
            },
            Face::Document,
        ),
        (
            "deed-form-slip",
            Body::Form {
                blank: "science-museum-slip".into(),
                fields: vec![FormField {
                    name: "child".into(),
                    value: "Sam".into(),
                }],
                path: PathBuf::from("/forms/slip.pdf"),
            },
            Face::Document,
        ),
        (
            "deed-table-fairing",
            Body::Table {
                measures: vec![Measure {
                    name: "dish".into(),
                    value: "3.80 m".into(),
                }],
            },
            Face::Document,
        ),
        (
            "deed-procedure-week",
            Body::Procedure {
                steps: vec![Step {
                    text: "Sign the slip".into(),
                    done: false,
                }],
            },
            Face::Document,
        ),
        (
            "deed-event-trip",
            Body::Event {
                when: "Friday".into(),
                where_: "Science Museum".into(),
                who: "Year 4".into(),
            },
            Face::Document,
        ),
    ];
    assert!(Kind::all().len() == 11);
    let mut seen = Vec::new();
    for (id, body, face) in cases {
        let kind = body.kind();
        let deed = Deed::from_draft(draft(id, id, body)).expect(id);
        assert_eq!(deed.face, face, "{id}");
        assert!(
            deed.id
                .as_str()
                .starts_with(&format!("deed-{}-", kind.token())),
            "{id}"
        );
        seen.push(kind);
    }
    for k in Kind::all() {
        assert!(seen.contains(k), "missing assemble for {}", k.token());
    }
}

#[test]
fn rejects_bad_message_id_and_closed_kind() {
    assert!(MailMessageId::parse("not-an-id").is_err());
    assert!(DeedId::parse("quote-heat").is_err());
    assert!(DeedId::parse("mat-quote-heatpump").is_err());
    assert!(Kind::parse("NOPE").is_err());
    assert!(Kind::parse("waveform").is_err());
    assert!(Kind::parse("mail-draft").is_err());
}

#[test]
fn mail_id_canonicalizes_brackets() {
    let id = MailMessageId::parse("maya@example.com").unwrap();
    assert_eq!(id.as_str(), "<maya@example.com>");
}

#[test]
fn empty_file_path_is_rejected() {
    assert!(Deed::from_draft(Draft {
        id: None,
        name: "x".into(),
        sources: Vec::new(),
        produced_by: agent("a", None),
        grants: Vec::new(),
        body: Body::File {
            path: PathBuf::new(),
            media_type: None,
        },
    })
    .is_err());
}

#[test]
fn error_display_covers_variants() {
    for e in [
        Error::InvalidId("x".into()),
        Error::InvalidKind("x".into()),
        Error::InvalidUrl("x".into()),
        Error::InvalidMessageId("x".into()),
        Error::InvalidBody("x".into()),
        Error::NotFound("x".into()),
        Error::Frozen("x".into()),
        Error::Tombstoned("x".into()),
        Error::Evidence("x".into()),
        Error::Io("x".into()),
        Error::Decode("x".into()),
    ] {
        assert!(!e.to_string().is_empty());
    }
}
