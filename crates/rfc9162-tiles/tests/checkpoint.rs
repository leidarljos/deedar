//! The C2SP examples parse and verify as the pages give them, a log's
//! checkpoint round-trips through its signer, and cosignatures check over
//! the cosignature/v1 lines.

use ed25519_dalek::pkcs8::EncodePrivateKey;
use ed25519_dalek::SigningKey;
use rfc9162_tiles::checkpoint::{
    key_id, verify, verify_cosignatures, ED25519_COSIGNATURE, ED25519_LOG,
};
use rfc9162_tiles::index::no_keys;
use rfc9162_tiles::layout::CHECKPOINT;
use rfc9162_tiles::{
    base64, Checkpoint, Cosigner, Error, MemoryStore, Note, Reader, Signer, TileStore, Verifier,
    Writer,
};

/// The example on c2sp.org/tlog-checkpoint.
const CHECKPOINT_EXAMPLE: &str = "example.com/behind-the-sofa\n\
20852163\n\
CsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=\n\
\n\
\u{2014} example.com/behind-the-sofa Az3grlgtzPICa5OS8npVmf1Myq/5IZniMp+ZJurmRDeOoRDe4URYN7u5/Zhcyv2q1gGzGku9nTo+zyWE+xeMcTOAYQ8=\n";

/// The key and signed message on c2sp.org/signed-note.
const NOTE_KEY: &str = "example.com/foo+530d903a+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k";
const NOTE_EXAMPLE: &str = "This is an example message.\n\
\n\
\u{2014} example.com/foo Uw2QOkn8srV1yJGh2VYRlL1Tnagv1YEq6TfXppzi2ONncAlTgK7Ztg1ERYNZXsYjOBH3mFXmRKuwHjG1Yu72IneyaQM=\n";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

#[test]
fn the_checkpoint_example_parses() {
    let note = Note::parse(CHECKPOINT_EXAMPLE).unwrap();
    assert_eq!(
        note.text,
        "example.com/behind-the-sofa\n20852163\nCsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=\n"
    );
    let checkpoint = Checkpoint::parse(&note.text).unwrap();
    assert_eq!(checkpoint.origin, "example.com/behind-the-sofa");
    assert_eq!(checkpoint.size, 20_852_163);
    assert_eq!(
        base64::encode(&checkpoint.root),
        "CsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I="
    );
    assert!(checkpoint.extensions.is_empty());
    assert_eq!(checkpoint.to_text(), note.text);
    assert_eq!(note.signatures.len(), 1);
    let sig = &note.signatures[0];
    assert_eq!(sig.name, "example.com/behind-the-sofa");
    assert_eq!(sig.key_id, [0x03, 0x3d, 0xe0, 0xae]);
    assert_eq!(sig.payload.len(), 64);
    assert_eq!(note.to_text(), CHECKPOINT_EXAMPLE);
    // The page's key is not published, so the only trusted keys here are
    // strangers and the note has no signature a verifier accepts.
    let stranger = Signer::new("example.com/behind-the-sofa", signing_key(1))
        .unwrap()
        .verifier();
    assert!(matches!(
        verify(&note, &[stranger]),
        Err(Error::Signature(_))
    ));
}

#[test]
fn the_signed_note_example_verifies() {
    let key = Verifier::parse(NOTE_KEY).unwrap();
    assert_eq!(key.name(), "example.com/foo");
    assert_eq!(key.kind(), ED25519_LOG);
    assert_eq!(key.key_id(), [0x53, 0x0d, 0x90, 0x3a]);
    assert_eq!(key.to_text(), NOTE_KEY);
    let note = Note::parse(NOTE_EXAMPLE).unwrap();
    let verified = verify(&note, std::slice::from_ref(&key)).unwrap();
    assert_eq!(verified, vec![&key]);

    // A changed text fails under the known key.
    let mut changed = note.clone();
    changed.text = "This is an example message!\n".to_string();
    assert!(matches!(
        verify(&changed, std::slice::from_ref(&key)),
        Err(Error::Signature(_))
    ));
    // An unknown key is ignored, so a note signed only by strangers fails.
    let other = Signer::new("example.com/foo", signing_key(2))
        .unwrap()
        .verifier();
    assert!(verify(&note, std::slice::from_ref(&other)).is_err());
    let both = [other, key.clone()];
    assert_eq!(verify(&note, &both).unwrap(), vec![&key]);
    // A key id that is not the key's is refused.
    assert!(Verifier::parse(
        "example.com/foo+530d903b+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k"
    )
    .is_err());
}

#[test]
fn malformed_notes_are_refused() {
    for text in [
        "",
        "no blank line\n",
        "text\n\n",
        "text\n\n- example.com/x AAAA\n",
        "text\n\n\u{2014} example.com/x\n",
        "text\n\n\u{2014} example.com/x AAAA",
        "text\n\n\u{2014} bad+name AAAAAAAAAA==\n",
        "text\n\n\u{2014} example.com/x AAAAAA==\n",
        "text\n\n\u{2014} example.com/x AAAAAAAA!\n",
        "te\u{7}xt\n\n\u{2014} example.com/x AAAAAAAAAA==\n",
    ] {
        assert!(Note::parse(text).is_err(), "{text:?}");
    }
    assert!(
        Checkpoint::parse("origin\n007\nCsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=\n").is_err()
    );
    assert!(Checkpoint::parse("origin\n7\nCsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I\n").is_err());
    assert!(Checkpoint::parse("\n7\nCsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=\n").is_err());
    assert!(
        Checkpoint::parse("origin\n7\nCsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=\n\nx\n")
            .is_err()
    );
    let with_extension =
        Checkpoint::parse("origin\n7\nCsUYapGGPo4dkMgIAUqom/Xajj7h2fB2MPA3j2jxq2I=\nextra\n")
            .unwrap();
    assert_eq!(with_extension.extensions, vec!["extra".to_string()]);
}

#[test]
fn the_key_id_is_the_spec_derivation() {
    let key = signing_key(9);
    let public = key.verifying_key();
    let expected = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"example.com/log");
        hasher.update([0x0A, 0x01]);
        hasher.update(public.as_bytes());
        let digest = hasher.finalize();
        [digest[0], digest[1], digest[2], digest[3]]
    };
    assert_eq!(
        key_id("example.com/log", ED25519_LOG, public.as_bytes()),
        expected
    );
    assert_ne!(
        key_id("example.com/log", ED25519_COSIGNATURE, public.as_bytes()),
        expected
    );
    assert_ne!(
        key_id("example.com/other", ED25519_LOG, public.as_bytes()),
        expected
    );
}

#[test]
fn a_log_publishes_a_checkpoint_a_reader_opens() {
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("log.pem");
    std::fs::write(
        &key_path,
        signing_key(3)
            .to_pkcs8_pem(Default::default())
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    let signer = Signer::from_pem_file("example.com/log", &key_path).unwrap();
    let trusted = vec![signer.verifier()];

    let store = MemoryStore::new();
    let mut writer = Writer::open(store.clone(), no_keys()).unwrap();
    writer
        .append_batch((0..300u32).map(|i| i.to_be_bytes().to_vec()))
        .unwrap();
    let note = writer.checkpoint(&signer, "example.com/log").unwrap();
    let text = String::from_utf8(store.get(CHECKPOINT).unwrap().unwrap()).unwrap();
    assert_eq!(text, note.to_text());
    assert!(text.starts_with(&format!(
        "example.com/log\n300\n{}\n\n\u{2014} example.com/log ",
        base64::encode(&writer.root())
    )));
    assert_eq!(Note::parse(&text).unwrap(), note);

    let reader =
        Reader::open_checkpoint(store.clone(), "example.com/log", &trusted, no_keys()).unwrap();
    assert_eq!(reader.size(), 300);
    assert_eq!(reader.root(), writer.root());
    assert_eq!(
        reader.get(299).unwrap(),
        Some(299u32.to_be_bytes().to_vec())
    );

    // The wrong origin, or no trusted key, refuses.
    assert!(
        Reader::open_checkpoint(store.clone(), "example.com/other", &trusted, no_keys()).is_err()
    );
    let stranger = Signer::new("example.com/log", signing_key(4)).unwrap();
    assert!(Reader::open_checkpoint(
        store.clone(),
        "example.com/log",
        &[stranger.verifier()],
        no_keys()
    )
    .is_err());

    // A second signer cannot publish a checkpoint for a smaller tree under
    // the same origin.
    drop(writer);
    let tampered = MemoryStore::new();
    let mut shorter = Writer::open(tampered.clone(), no_keys()).unwrap();
    shorter
        .append_batch((0..10u32).map(|i| i.to_be_bytes().to_vec()))
        .unwrap();
    tampered.put(CHECKPOINT, text.as_bytes()).unwrap();
    assert!(matches!(
        shorter.checkpoint(&signer, "example.com/log"),
        Err(Error::OutOfRange(_))
    ));
    shorter
        .append_batch((10..300u32).map(|i| i.to_be_bytes().to_vec()))
        .unwrap();
    shorter.checkpoint(&signer, "example.com/log").unwrap();
}

#[test]
fn a_cosignature_verifies_over_the_cosignature_lines() {
    let signer = Signer::new("example.com/log", signing_key(5)).unwrap();
    let witness = Cosigner::new("witness.example/w1", signing_key(6)).unwrap();
    let checkpoint = Checkpoint::new("example.com/log", 42, [7u8; 32]);
    let mut note = signer.sign(&checkpoint);
    witness.cosign(&mut note, 1_679_315_147);
    assert_eq!(note.signatures.len(), 2);
    let line = &note.signatures[1];
    assert_eq!(line.name, "witness.example/w1");
    assert_eq!(line.key_id, witness.verifier().key_id());
    assert_eq!(line.payload.len(), 8 + 64);
    assert_eq!(&line.payload[..8], &1_679_315_147u64.to_be_bytes());

    // Round trip through text, then both checks.
    let parsed = Note::parse(&note.to_text()).unwrap();
    assert_eq!(parsed, note);
    assert_eq!(verify(&parsed, &[signer.verifier()]).unwrap().len(), 1);
    let cosigned = verify_cosignatures(&parsed, &[witness.verifier()]).unwrap();
    assert_eq!(cosigned.len(), 1);
    assert_eq!(cosigned[0].name, "witness.example/w1");
    assert_eq!(cosigned[0].timestamp, 1_679_315_147);

    // The signed message is the spec's lines.
    assert_eq!(
        rfc9162_tiles::checkpoint::cosignature_message(&note.text, 1_679_315_147),
        format!(
            "cosignature/v1\ntime 1679315147\nexample.com/log\n42\n{}\n",
            base64::encode(&[7u8; 32])
        )
        .into_bytes()
    );

    // A witness line with a changed timestamp fails; a stranger witness is
    // ignored; a witness that signed nothing verifies nothing.
    let mut changed = parsed.clone();
    changed.signatures[1].payload[7] ^= 1;
    assert!(matches!(
        verify_cosignatures(&changed, &[witness.verifier()]),
        Err(Error::Signature(_))
    ));
    let other = Cosigner::new("witness.example/w2", signing_key(8)).unwrap();
    assert!(verify_cosignatures(&parsed, &[other.verifier()])
        .unwrap()
        .is_empty());
    // The log key, under the cosignature type, is a different key id, so the
    // log's own line is not taken for a cosignature.
    let log_as_witness = Verifier::new(
        "example.com/log",
        ED25519_COSIGNATURE,
        signing_key(5).verifying_key(),
    )
    .unwrap();
    assert!(verify_cosignatures(&parsed, &[log_as_witness])
        .unwrap()
        .is_empty());
}
