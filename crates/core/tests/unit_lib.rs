//! [unit] tests for the crate-root API. One file per level per module: `<level>_<module>.rs`.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use myproj_core::{Error, Name, greet};

#[test]
fn ac1_greets_a_valid_name() -> Result<(), Error> {
    let name = Name::new("  ada ")?;
    assert_eq!(greet(&name), "hello, ada");
    Ok(())
}

#[test]
fn ac2_rejects_empty_name() {
    assert_eq!(Name::new("   "), Err(Error::EmptyName));
}
