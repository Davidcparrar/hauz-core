//! [unit] tests for the `store` module's public API: `InMemoryStore`. One file per level per
//! module. Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::Result;
use hauz_core::store::InMemoryStore;

#[tokio::test]
async fn ac1_insert_then_get() -> Result<()> {
    common::ac1_insert_then_get(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac2_unknown_key_is_none() -> Result<()> {
    common::ac2_unknown_key_is_none(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac3_second_insert_under_stored_hash_is_duplicate() -> Result<()> {
    common::ac3_second_insert_under_stored_hash_is_duplicate(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac4_insert_with_known_id_under_new_hash_is_rejected() -> Result<()> {
    common::ac4_insert_with_known_id_under_new_hash_is_rejected(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac5_list_returns_insertion_order() -> Result<()> {
    common::ac5_list_returns_insertion_order(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac6_bare_needs_review_bill_round_trips() -> Result<()> {
    common::ac6_bare_needs_review_bill_round_trips(&InMemoryStore::new()).await
}
