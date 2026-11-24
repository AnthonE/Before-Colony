//! The server's key signs what Elo Pros checks (`meter/serverkeys.py` in AnthonE/scry-forge): the
//! platform's own vectors (`sdk/testdata/server_texts.json` there, copied to `data/`), byte for
//! byte. Ed25519 is deterministic, so the seed signs each text to exactly its signature.

use base64::Engine;
use bc_server::serverkey::{ServerKey, server_text};
use ring::signature::{ED25519, UnparsedPublicKey};

fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!("data/server_texts.json")).expect("the vectors parse")
}

fn b64(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(s).expect("base64")
}

#[test]
fn it_signs_the_platforms_texts() {
    let v = vectors();
    // The vector ships no key header; the test wraps the PKCS#8 in its armour itself.
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        v["private_pkcs8_b64"].as_str().unwrap()
    );
    let seed = hex::decode(v["seed_hex"].as_str().unwrap()).unwrap();
    let public = UnparsedPublicKey::new(&ED25519, b64(v["public_b64"].as_str().unwrap()));
    for case in v["cases"].as_array().unwrap() {
        let s = |k: &str| case[k].as_str().unwrap();
        let (game, id) = s("who").split_once('/').unwrap();
        let key = ServerKey::from_pem(game, id, &pem).expect("the PEM reads");
        assert_eq!(key.public_b64(), v["public_b64"]);
        assert_eq!(ServerKey::from_seed(game, id, &seed).unwrap().public_b64(), v["public_b64"]);
        assert_eq!(key.who(), s("who"));
        let at: u64 = s("at").parse().unwrap();
        let body = s("body_utf8").as_bytes();
        let text = server_text(s("method"), s("path"), s("who"), at, body, case["nonce"].as_str());
        assert_eq!(text, s("text"));
        // The platform's signature is over exactly that text.
        public.verify(text.as_bytes(), &b64(s("signature_b64"))).expect("the vector verifies");
        // And this key, given the case's nonce, sends it in its headers.
        if let Some(nonce) = case["nonce"].as_str() {
            let h = key.headers(s("method"), s("path"), body, at, nonce);
            let names: Vec<&str> = h.iter().map(|(k, _)| *k).collect();
            assert_eq!(names, ["X-Elo-Server", "X-Elo-Time", "X-Elo-Nonce", "X-Elo-Signature"]);
            assert_eq!((h[0].1.as_str(), h[1].1.as_str(), h[2].1.as_str()), (s("who"), s("at"), nonce));
            assert_eq!(h[3].1, s("signature_b64"));
        }
    }
}

#[test]
fn a_file_that_isnt_a_key_is_refused_without_being_quoted() {
    let not_key = "-----BEGIN PRIVATE KEY-----\nc2VjcmV0LXNvdXA=\n-----END PRIVATE KEY-----\n";
    let why = ServerKey::from_pem("before-colony", "sector-1", not_key).unwrap_err();
    assert!(!why.contains("c2VjcmV0") && !why.contains("secret"), "{why}");
    assert!(ServerKey::from_pem("before-colony", "sector-1", "not a pem at all!").is_err());
}
