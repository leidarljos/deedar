//! Cap'n-shaped encode/decode of schema/deeder.capnp.
//! Field numbers match the schema ordinals.

use std::path::PathBuf;

use deed::{
    Body, Deed, DeedId, Error, Evidence, Face, FormField, Grant, Kind, MailMessageId, Measure,
    ProducedBy, Result, Source, Step,
};

use crate::CreateRequest;

const MAGIC: &[u8; 8] = b"CapNdeed";
const REV: u32 = 1;

const OP_CREATE: u8 = 0;
const OP_GET: u8 = 1;
const OP_LIST: u8 = 2;
const OP_TRAIL: u8 = 3;
const OP_DELETE: u8 = 4;
const OP_EVIDENCE: u8 = 5;
const OP_DEED: u8 = 10;
const OP_DEED_EVIDENCE: u8 = 11;
const OP_DEEDS: u8 = 12;
const OP_OK: u8 = 13;
const OP_ERR: u8 = 14;
const OP_EVIDENCE_OK: u8 = 15;

pub enum Request {
    Create(Box<CreateRequest>),
    Get(DeedId),
    List,
    Trail(DeedId),
    Delete(DeedId),
    Evidence(DeedId),
}

pub enum Response {
    Deed(Deed),
    Created { deed: Deed, evidence: Evidence },
    Deeds(Vec<Deed>),
    Evidence(Evidence),
    Ok,
    Err(String),
}

pub fn encode_request(req: &Request) -> Result<Vec<u8>> {
    let mut p = Vec::new();
    match req {
        Request::Create(c) => {
            p.push(OP_CREATE);
            put_create(&mut p, c.as_ref());
        }
        Request::Get(id) => {
            p.push(OP_GET);
            put_str(&mut p, id.as_str());
        }
        Request::List => p.push(OP_LIST),
        Request::Trail(id) => {
            p.push(OP_TRAIL);
            put_str(&mut p, id.as_str());
        }
        Request::Delete(id) => {
            p.push(OP_DELETE);
            put_str(&mut p, id.as_str());
        }
        Request::Evidence(id) => {
            p.push(OP_EVIDENCE);
            put_str(&mut p, id.as_str());
        }
    }
    wrap(&p)
}

pub fn decode_request(bytes: &[u8]) -> Result<Request> {
    let p = unwrap(bytes)?;
    let (op, rest) = p
        .split_first()
        .ok_or_else(|| Error::Decode("empty".into()))?;
    match *op {
        OP_CREATE => Ok(Request::Create(Box::new(take_create(rest)?))),
        OP_GET => Ok(Request::Get(DeedId::parse(&take_str(rest)?.0)?)),
        OP_LIST => Ok(Request::List),
        OP_TRAIL => Ok(Request::Trail(DeedId::parse(&take_str(rest)?.0)?)),
        OP_DELETE => Ok(Request::Delete(DeedId::parse(&take_str(rest)?.0)?)),
        OP_EVIDENCE => Ok(Request::Evidence(DeedId::parse(&take_str(rest)?.0)?)),
        _ => Err(Error::Decode(format!("bad request op {op}"))),
    }
}

pub fn encode_response(resp: &Response) -> Result<Vec<u8>> {
    let mut p = Vec::new();
    match resp {
        Response::Deed(d) => {
            p.push(OP_DEED);
            put_deed(&mut p, d);
        }
        Response::Created { deed, evidence } => {
            p.push(OP_DEED_EVIDENCE);
            put_deed(&mut p, deed);
            put_evidence(&mut p, evidence);
        }
        Response::Evidence(ev) => {
            p.push(OP_EVIDENCE_OK);
            put_evidence(&mut p, ev);
        }
        Response::Deeds(ds) => {
            p.push(OP_DEEDS);
            put_u32(&mut p, ds.len() as u32);
            for d in ds {
                put_deed(&mut p, d);
            }
        }
        Response::Ok => p.push(OP_OK),
        Response::Err(e) => {
            p.push(OP_ERR);
            put_str(&mut p, e);
        }
    }
    wrap(&p)
}

pub fn decode_response(bytes: &[u8]) -> Result<Response> {
    let p = unwrap(bytes)?;
    let (op, mut rest) = p
        .split_first()
        .ok_or_else(|| Error::Decode("empty".into()))?;
    match *op {
        OP_DEED => Ok(Response::Deed(take_deed(rest)?.0)),
        OP_DEED_EVIDENCE => {
            let (deed, r) = take_deed(rest)?;
            let (evidence, _) = take_evidence(r)?;
            Ok(Response::Created { deed, evidence })
        }
        OP_EVIDENCE_OK => Ok(Response::Evidence(take_evidence(rest)?.0)),
        OP_DEEDS => {
            let n = take_u32(rest)?;
            rest = &rest[4..];
            let mut out = Vec::new();
            for _ in 0..n {
                let (d, r) = take_deed(rest)?;
                out.push(d);
                rest = r;
            }
            Ok(Response::Deeds(out))
        }
        OP_OK => Ok(Response::Ok),
        OP_ERR => Ok(Response::Err(take_str(rest)?.0)),
        _ => Err(Error::Decode(format!("bad response op {op}"))),
    }
}

fn wrap(payload: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&REV.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

fn unwrap(bytes: &[u8]) -> Result<&[u8]> {
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return Err(Error::Decode("not a deeder Cap'n frame".into()));
    }
    let rev = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if rev != REV {
        return Err(Error::Decode(format!("schema rev {rev}")));
    }
    let n = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if bytes.len() < 16 + n {
        return Err(Error::Decode("truncated".into()));
    }
    Ok(&bytes[16..16 + n])
}

fn put_u32(p: &mut Vec<u8>, n: u32) {
    p.extend_from_slice(&n.to_le_bytes());
}

fn put_u64(p: &mut Vec<u8>, n: u64) {
    p.extend_from_slice(&n.to_le_bytes());
}

fn put_f64(p: &mut Vec<u8>, n: f64) {
    p.extend_from_slice(&n.to_le_bytes());
}

fn put_str(p: &mut Vec<u8>, s: &str) {
    put_u32(p, s.len() as u32);
    p.extend_from_slice(s.as_bytes());
}

fn take_u32(p: &[u8]) -> Result<u32> {
    if p.len() < 4 {
        return Err(Error::Decode("u32".into()));
    }
    Ok(u32::from_le_bytes(p[..4].try_into().unwrap()))
}

fn take_u64(p: &[u8]) -> Result<(u64, &[u8])> {
    if p.len() < 8 {
        return Err(Error::Decode("u64".into()));
    }
    Ok((u64::from_le_bytes(p[..8].try_into().unwrap()), &p[8..]))
}

fn take_f64(p: &[u8]) -> Result<(f64, &[u8])> {
    if p.len() < 8 {
        return Err(Error::Decode("f64".into()));
    }
    Ok((f64::from_le_bytes(p[..8].try_into().unwrap()), &p[8..]))
}

fn take_str(p: &[u8]) -> Result<(String, &[u8])> {
    let n = take_u32(p)? as usize;
    if p.len() < 4 + n {
        return Err(Error::Decode("text".into()));
    }
    let s = std::str::from_utf8(&p[4..4 + n]).map_err(|e| Error::Decode(e.to_string()))?;
    Ok((s.to_string(), &p[4 + n..]))
}

fn put_create(p: &mut Vec<u8>, c: &CreateRequest) {
    put_str(p, c.id.as_ref().map(|i| i.as_str()).unwrap_or(""));
    put_str(p, &c.name);
    put_u32(p, c.sources.len() as u32);
    for s in &c.sources {
        put_source(p, s);
    }
    put_str(p, &c.seat_agent_id);
    put_str(p, c.seat_activity_id.as_deref().unwrap_or(""));
    put_u32(p, c.policy_grants.len() as u32);
    for g in &c.policy_grants {
        put_grant(p, g);
    }
    put_body(p, &c.body);
}

fn take_create(mut p: &[u8]) -> Result<CreateRequest> {
    let (id, r) = take_str(p)?;
    p = r;
    let (name, r) = take_str(p)?;
    p = r;
    let n = take_u32(p)?;
    p = &p[4..];
    let mut sources = Vec::new();
    for _ in 0..n {
        let (s, r) = take_source(p)?;
        sources.push(s);
        p = r;
    }
    let (agent, r) = take_str(p)?;
    p = r;
    let (act, r) = take_str(p)?;
    p = r;
    let ng = take_u32(p)?;
    p = &p[4..];
    let mut grants = Vec::new();
    for _ in 0..ng {
        let (g, r) = take_grant(p)?;
        grants.push(g);
        p = r;
    }
    let (body, _) = take_body(p)?;
    Ok(CreateRequest {
        id: if id.is_empty() {
            None
        } else {
            Some(DeedId::parse(&id)?)
        },
        name,
        sources,
        seat_agent_id: agent,
        seat_activity_id: if act.is_empty() { None } else { Some(act) },
        policy_grants: grants,
        body,
    })
}

fn put_source(p: &mut Vec<u8>, s: &Source) {
    match s {
        Source::Deed(id) => {
            p.push(0);
            put_str(p, id.as_str());
        }
        Source::Path(path) => {
            p.push(1);
            put_str(p, &path.to_string_lossy());
        }
        Source::Url(u) => {
            p.push(2);
            put_str(p, u);
        }
    }
}

fn take_source(p: &[u8]) -> Result<(Source, &[u8])> {
    let tag = *p.first().ok_or_else(|| Error::Decode("source".into()))?;
    let (s, r) = take_str(&p[1..])?;
    let src = match tag {
        0 => Source::deed(DeedId::parse(&s)?),
        1 => Source::path(s),
        2 => Source::url(&s)?,
        _ => return Err(Error::Decode("source tag".into())),
    };
    Ok((src, r))
}

fn put_grant(p: &mut Vec<u8>, g: &Grant) {
    match g {
        Grant::Host(h) => {
            p.push(0);
            put_str(p, h);
        }
        Grant::Path(path) => {
            p.push(1);
            put_str(p, &path.to_string_lossy());
        }
    }
}

fn take_grant(p: &[u8]) -> Result<(Grant, &[u8])> {
    let tag = *p.first().ok_or_else(|| Error::Decode("grant".into()))?;
    let (s, r) = take_str(&p[1..])?;
    let g = match tag {
        0 => Grant::Host(s),
        1 => Grant::Path(PathBuf::from(s)),
        _ => return Err(Error::Decode("grant tag".into())),
    };
    Ok((g, r))
}

fn put_produced(p: &mut Vec<u8>, pb: &ProducedBy) {
    put_str(p, &pb.agent_id);
    put_str(p, pb.activity_id.as_deref().unwrap_or(""));
}

fn take_produced(p: &[u8]) -> Result<(ProducedBy, &[u8])> {
    let (agent, r) = take_str(p)?;
    let (act, r) = take_str(r)?;
    Ok((
        ProducedBy {
            agent_id: agent,
            activity_id: if act.is_empty() { None } else { Some(act) },
        },
        r,
    ))
}

fn put_kind(p: &mut Vec<u8>, k: &Kind) {
    let n = match k {
        Kind::File => 0u8,
        Kind::Set => 1,
        Kind::Quote => 2,
        Kind::Patch => 3,
        Kind::MailDraft => 4,
        Kind::Clip => 5,
        Kind::Page => 6,
        Kind::Form => 7,
        Kind::Table => 8,
        Kind::Procedure => 9,
        Kind::Event => 10,
    };
    p.push(n);
}

fn take_kind(p: &[u8]) -> Result<(Kind, &[u8])> {
    let t = *p.first().ok_or_else(|| Error::Decode("kind".into()))?;
    let k = match t {
        0 => Kind::File,
        1 => Kind::Set,
        2 => Kind::Quote,
        3 => Kind::Patch,
        4 => Kind::MailDraft,
        5 => Kind::Clip,
        6 => Kind::Page,
        7 => Kind::Form,
        8 => Kind::Table,
        9 => Kind::Procedure,
        10 => Kind::Event,
        _ => return Err(Error::Decode("kind tag".into())),
    };
    Ok((k, &p[1..]))
}

fn put_face(p: &mut Vec<u8>, f: Face) {
    p.push(match f {
        Face::Syntax => 0,
        Face::Sheet => 1,
        Face::Letter => 2,
        Face::Waveform => 3,
        Face::Document => 4,
    });
}

fn take_face(p: &[u8]) -> Result<(Face, &[u8])> {
    let t = *p.first().ok_or_else(|| Error::Decode("face".into()))?;
    let f = match t {
        0 => Face::Syntax,
        1 => Face::Sheet,
        2 => Face::Letter,
        3 => Face::Waveform,
        4 => Face::Document,
        _ => return Err(Error::Decode("face tag".into())),
    };
    Ok((f, &p[1..]))
}

fn put_body(p: &mut Vec<u8>, b: &Body) {
    match b {
        Body::File { path, media_type } => {
            p.push(0);
            put_str(p, &path.to_string_lossy());
            put_str(p, media_type.as_deref().unwrap_or(""));
        }
        Body::Set { title, members } => {
            p.push(1);
            put_str(p, title);
            put_u32(p, members.len() as u32);
            for m in members {
                put_str(p, &m.to_string_lossy());
            }
        }
        Body::Quote {
            edition,
            start,
            end,
            excerpt,
            urls,
        } => {
            p.push(2);
            put_str(p, edition);
            put_u64(p, *start);
            put_u64(p, *end);
            put_str(p, excerpt);
            put_u32(p, urls.len() as u32);
            for u in urls {
                put_str(p, u);
            }
        }
        Body::Patch {
            tree,
            diffs,
            functionaries,
        } => {
            p.push(3);
            put_str(p, tree);
            put_u32(p, diffs.len() as u32);
            for d in diffs {
                put_str(p, &d.to_string_lossy());
            }
            put_u32(p, functionaries.len() as u32);
            for f in functionaries {
                put_str(p, f);
            }
        }
        Body::MailDraft {
            message_id,
            in_reply_to,
            subject,
            path,
        } => {
            p.push(4);
            put_str(p, message_id.as_str());
            put_str(
                p,
                in_reply_to
                    .as_ref()
                    .map(MailMessageId::as_str)
                    .unwrap_or(""),
            );
            put_str(p, subject);
            put_str(p, &path.to_string_lossy());
        }
        Body::Clip {
            sources,
            in_point,
            out_point,
            duration,
            path,
        } => {
            p.push(5);
            put_u32(p, sources.len() as u32);
            for s in sources {
                put_str(p, &s.to_string_lossy());
            }
            put_f64(p, *in_point);
            put_f64(p, *out_point);
            put_f64(p, *duration);
            put_str(p, &path.to_string_lossy());
        }
        Body::Page { url, snapshot } => {
            p.push(6);
            put_str(p, url);
            put_str(p, &snapshot.to_string_lossy());
        }
        Body::Form {
            blank,
            fields,
            path,
        } => {
            p.push(7);
            put_str(p, blank);
            put_u32(p, fields.len() as u32);
            for f in fields {
                put_str(p, &f.name);
                put_str(p, &f.value);
            }
            put_str(p, &path.to_string_lossy());
        }
        Body::Table { measures } => {
            p.push(8);
            put_u32(p, measures.len() as u32);
            for m in measures {
                put_str(p, &m.name);
                put_str(p, &m.value);
            }
        }
        Body::Procedure { steps } => {
            p.push(9);
            put_u32(p, steps.len() as u32);
            for s in steps {
                put_str(p, &s.text);
                p.push(u8::from(s.done));
            }
        }
        Body::Event { when, where_, who } => {
            p.push(10);
            put_str(p, when);
            put_str(p, where_);
            put_str(p, who);
        }
    }
}

fn take_body(mut p: &[u8]) -> Result<(Body, &[u8])> {
    let tag = *p.first().ok_or_else(|| Error::Decode("body".into()))?;
    p = &p[1..];
    match tag {
        0 => {
            let (path, r) = take_str(p)?;
            let (mt, r) = take_str(r)?;
            Ok((
                Body::File {
                    path: PathBuf::from(path),
                    media_type: if mt.is_empty() { None } else { Some(mt) },
                },
                r,
            ))
        }
        1 => {
            let (title, r) = take_str(p)?;
            let n = take_u32(r)?;
            let mut r = &r[4..];
            let mut members = Vec::new();
            for _ in 0..n {
                let (s, nr) = take_str(r)?;
                members.push(PathBuf::from(s));
                r = nr;
            }
            Ok((Body::Set { title, members }, r))
        }
        2 => {
            let (edition, r) = take_str(p)?;
            let (start, r) = take_u64(r)?;
            let (end, r) = take_u64(r)?;
            let (excerpt, r) = take_str(r)?;
            let n = take_u32(r)?;
            let mut r = &r[4..];
            let mut urls = Vec::new();
            for _ in 0..n {
                let (u, nr) = take_str(r)?;
                urls.push(u);
                r = nr;
            }
            Ok((
                Body::Quote {
                    edition,
                    start,
                    end,
                    excerpt,
                    urls,
                },
                r,
            ))
        }
        3 => {
            let (tree, r) = take_str(p)?;
            let n = take_u32(r)?;
            let mut r = &r[4..];
            let mut diffs = Vec::new();
            for _ in 0..n {
                let (s, nr) = take_str(r)?;
                diffs.push(PathBuf::from(s));
                r = nr;
            }
            let nf = take_u32(r)?;
            r = &r[4..];
            let mut functionaries = Vec::new();
            for _ in 0..nf {
                let (s, nr) = take_str(r)?;
                functionaries.push(s);
                r = nr;
            }
            Ok((
                Body::Patch {
                    tree,
                    diffs,
                    functionaries,
                },
                r,
            ))
        }
        4 => {
            let (mid, r) = take_str(p)?;
            let (irt, r) = take_str(r)?;
            let (subject, r) = take_str(r)?;
            let (path, r) = take_str(r)?;
            Ok((
                Body::MailDraft {
                    message_id: MailMessageId::parse(&mid)?,
                    in_reply_to: if irt.is_empty() {
                        None
                    } else {
                        Some(MailMessageId::parse(&irt)?)
                    },
                    subject,
                    path: PathBuf::from(path),
                },
                r,
            ))
        }
        5 => {
            let n = take_u32(p)?;
            let mut r = &p[4..];
            let mut sources = Vec::new();
            for _ in 0..n {
                let (s, nr) = take_str(r)?;
                sources.push(PathBuf::from(s));
                r = nr;
            }
            let (in_point, r) = take_f64(r)?;
            let (out_point, r) = take_f64(r)?;
            let (duration, r) = take_f64(r)?;
            let (path, r) = take_str(r)?;
            Ok((
                Body::Clip {
                    sources,
                    in_point,
                    out_point,
                    duration,
                    path: PathBuf::from(path),
                },
                r,
            ))
        }
        6 => {
            let (url, r) = take_str(p)?;
            let (snap, r) = take_str(r)?;
            Ok((
                Body::Page {
                    url,
                    snapshot: PathBuf::from(snap),
                },
                r,
            ))
        }
        7 => {
            let (blank, r) = take_str(p)?;
            let n = take_u32(r)?;
            let mut r = &r[4..];
            let mut fields = Vec::new();
            for _ in 0..n {
                let (name, nr) = take_str(r)?;
                let (value, nr) = take_str(nr)?;
                fields.push(FormField { name, value });
                r = nr;
            }
            let (path, r) = take_str(r)?;
            Ok((
                Body::Form {
                    blank,
                    fields,
                    path: PathBuf::from(path),
                },
                r,
            ))
        }
        8 => {
            let n = take_u32(p)?;
            let mut r = &p[4..];
            let mut measures = Vec::new();
            for _ in 0..n {
                let (name, nr) = take_str(r)?;
                let (value, nr) = take_str(nr)?;
                measures.push(Measure { name, value });
                r = nr;
            }
            Ok((Body::Table { measures }, r))
        }
        9 => {
            let n = take_u32(p)?;
            let mut r = &p[4..];
            let mut steps = Vec::new();
            for _ in 0..n {
                let (text, nr) = take_str(r)?;
                let done = *nr.first().ok_or_else(|| Error::Decode("step".into()))? != 0;
                steps.push(Step { text, done });
                r = &nr[1..];
            }
            Ok((Body::Procedure { steps }, r))
        }
        10 => {
            let (when, r) = take_str(p)?;
            let (where_, r) = take_str(r)?;
            let (who, r) = take_str(r)?;
            Ok((Body::Event { when, where_, who }, r))
        }
        _ => Err(Error::Decode("body tag".into())),
    }
}

pub fn deed_bytes(d: &Deed) -> Vec<u8> {
    let mut p = Vec::new();
    put_deed(&mut p, d);
    p
}

fn put_deed(p: &mut Vec<u8>, d: &Deed) {
    put_str(p, d.id.as_str());
    put_kind(p, &d.kind);
    put_str(p, &d.name);
    put_u32(p, d.paths.len() as u32);
    for path in &d.paths {
        put_str(p, &path.to_string_lossy());
    }
    put_u32(p, d.sources.len() as u32);
    for s in &d.sources {
        put_source(p, s);
    }
    put_produced(p, &d.produced_by);
    put_u32(p, d.grants.len() as u32);
    for g in &d.grants {
        put_grant(p, g);
    }
    put_face(p, d.face);
    put_body(p, &d.body);
}

fn take_deed(mut p: &[u8]) -> Result<(Deed, &[u8])> {
    let (id, r) = take_str(p)?;
    p = r;
    let (kind, r) = take_kind(p)?;
    p = r;
    let (name, r) = take_str(p)?;
    p = r;
    let n = take_u32(p)?;
    p = &p[4..];
    let mut paths = Vec::new();
    for _ in 0..n {
        let (s, r) = take_str(p)?;
        paths.push(PathBuf::from(s));
        p = r;
    }
    let ns = take_u32(p)?;
    p = &p[4..];
    let mut sources = Vec::new();
    for _ in 0..ns {
        let (s, r) = take_source(p)?;
        sources.push(s);
        p = r;
    }
    let (produced_by, r) = take_produced(p)?;
    p = r;
    let ng = take_u32(p)?;
    p = &p[4..];
    let mut grants = Vec::new();
    for _ in 0..ng {
        let (g, r) = take_grant(p)?;
        grants.push(g);
        p = r;
    }
    let (face, r) = take_face(p)?;
    p = r;
    let (body, r) = take_body(p)?;
    Ok((
        Deed {
            id: DeedId::parse(&id)?,
            kind,
            name,
            paths,
            sources,
            produced_by,
            grants,
            face,
            body,
        },
        r,
    ))
}

fn put_evidence(p: &mut Vec<u8>, ev: &Evidence) {
    put_str(p, ev.deed_id.as_str());
    put_produced(p, &ev.produced_by);
    put_u32(p, ev.grants.len() as u32);
    for g in &ev.grants {
        put_grant(p, g);
    }
    put_u64(p, ev.unix_time);
    put_u32(p, ev.signature.len() as u32);
    p.extend_from_slice(&ev.signature);
}

fn take_evidence(mut p: &[u8]) -> Result<(Evidence, &[u8])> {
    let (id, r) = take_str(p)?;
    p = r;
    let (produced_by, r) = take_produced(p)?;
    p = r;
    let n = take_u32(p)?;
    p = &p[4..];
    let mut grants = Vec::new();
    for _ in 0..n {
        let (g, r) = take_grant(p)?;
        grants.push(g);
        p = r;
    }
    let (unix_time, r) = take_u64(p)?;
    p = r;
    let sl = take_u32(p)? as usize;
    p = &p[4..];
    if p.len() < sl {
        return Err(Error::Decode("sig".into()));
    }
    let signature = p[..sl].to_vec();
    Ok((
        Evidence {
            deed_id: DeedId::parse(&id)?,
            produced_by,
            grants,
            unix_time,
            signature,
        },
        &p[sl..],
    ))
}
