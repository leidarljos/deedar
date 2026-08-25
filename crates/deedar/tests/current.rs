//! Later take records replacement; get stays on the named accession.

mod common;

use deed::DeedId;

use common::{open, quote, tmp_url};

#[test]
fn current_follows_supersedes_get_stays_on_frozen_id() {
    let mut client = open(&tmp_url());
    let a = DeedId::parse("deed-quote-first").unwrap();
    let b = DeedId::parse("deed-quote-later").unwrap();
    client
        .create(quote(
            "deed-quote-first",
            "first take",
            "https://example.com/a",
        ))
        .unwrap();
    let mut later = quote("deed-quote-later", "later take", "https://example.com/b");
    later.supersedes = Some(a.clone());
    client.create(later).unwrap();

    let got_a = client.get(&a).expect("get frozen A");
    assert_eq!(got_a.id.as_str(), "deed-quote-first");
    assert_eq!(got_a.name, "deed-quote-first");

    let cur_a = client.current(&a).expect("current A");
    assert_eq!(cur_a.id.as_str(), "deed-quote-later");
    assert_eq!(cur_a.name, "deed-quote-later");

    let cur_b = client.current(&b).expect("current B");
    assert_eq!(cur_b.id.as_str(), "deed-quote-later");
}

#[test]
fn create_frame_keeps_supersedes() {
    let prior = DeedId::parse("deed-quote-first").unwrap();
    let mut req = quote("deed-quote-later", "later take", "https://example.com/b");
    req.supersedes = Some(prior.clone());
    let bytes = deedar::encode_request(&deedar::Request::Create(Box::new(req))).unwrap();
    match deedar::decode_request(&bytes).unwrap() {
        deedar::Request::Create(got) => {
            assert_eq!(got.supersedes.as_ref(), Some(&prior));
        }
        _ => panic!("expected create frame"),
    }
}
