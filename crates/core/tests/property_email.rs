//! [property] tests for the `email` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::email::Envelope;
use proptest::prelude::*;

proptest! {
    #[test]
    fn ac8_parse_never_panics(raw: Vec<u8>) {
        let _ = Envelope::parse(&raw);
    }

    #[test]
    fn ac9_body_after_fixed_headers_still_parses(body in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut raw = b"From: alice@example.com\r\nSubject: Fuzz\r\nContent-Type: multipart/mixed; boundary=xyz\r\n\r\n".to_vec();
        raw.extend_from_slice(&body);

        let envelope = Envelope::parse(&raw).expect("fixed header block always yields a sender");
        prop_assert_eq!(envelope.sender, "alice@example.com");
    }
}
