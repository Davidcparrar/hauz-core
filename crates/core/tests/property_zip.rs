//! [property] tests for the `zip` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::build_stored_zip;
use hauz_core::zip::{self, Entry};
use proptest::prelude::*;

/// 0..=4 entries with short ASCII names and small payloads.
fn arb_entries() -> impl Strategy<Value = Vec<(String, Vec<u8>)>> {
    proptest::collection::vec(
        (
            "[A-Za-z0-9]{1,16}",
            proptest::collection::vec(any::<u8>(), 0..=2048),
        ),
        0..=4,
    )
}

proptest! {
    #[test]
    fn ac8_stored_archive_round_trips(entries in arb_entries()) {
        let archive = build_stored_zip(&entries);
        let result = zip::read(&archive).expect("a well-formed stored archive always reads");
        let expected: Vec<Entry> = entries
            .into_iter()
            .map(|(name, bytes)| Entry { name, bytes })
            .collect();
        prop_assert_eq!(result, expected);
    }

    #[test]
    fn ac9_read_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..=4096)) {
        let _ = zip::read(&bytes);
    }
}
