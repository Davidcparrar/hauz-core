//! [property] tests for the `store` module's public API: `InMemoryStore` paging. One file per
//! level per module. Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use hauz_core::bill::Status;
use hauz_core::store::{BillStore, InMemoryStore, ListQuery, PageSize};
use proptest::prelude::*;

fn arb_status() -> impl Strategy<Value = Option<Status>> {
    prop_oneof![
        Just(None),
        Just(Some(Status::Extracted)),
        Just(Some(Status::NeedsReview)),
    ]
}

proptest! {
    /// #66 AC5: following `next` from `before = None` yields exactly the filtered `list()`
    /// reversed, with no repeats or gaps.
    #[test]
    fn ac5_following_next_yields_filtered_list_reversed(
        review_flags in proptest::collection::vec(any::<bool>(), 0..30),
        status in arb_status(),
        limit in 1u32..=8,
    ) {
        let rt = tokio::runtime::Builder::new_current_thread().build().expect("runtime");
        let store = InMemoryStore::new();
        for (index, review) in review_flags.iter().enumerate() {
            let id = format!("bill-{index}");
            let bill = if *review {
                common::bare_needs_review_bill(&id).expect("bill")
            } else {
                common::extracted_bill(&id).expect("bill")
            };
            let byte = u8::try_from(index).expect("small index");
            rt.block_on(store.insert(&common::hash(byte), &bill)).expect("insert");
        }

        let mut expected: Vec<_> = rt
            .block_on(store.list())
            .expect("list")
            .into_iter()
            .filter(|b| status.is_none_or(|s| b.status() == s))
            .collect();
        expected.reverse();

        let mut seen = Vec::new();
        let mut before = None;
        loop {
            let query = ListQuery {
                status,
                limit: PageSize::new(limit).expect("limit in range"),
                before: before.clone(),
            };
            let page = rt.block_on(store.list_page(&query)).expect("list_page");
            prop_assert!(page.bills.len() <= limit as usize);
            seen.extend(page.bills);
            match page.next {
                Some(next) => before = Some(next),
                None => break,
            }
        }
        prop_assert_eq!(seen, expected);
    }
}
