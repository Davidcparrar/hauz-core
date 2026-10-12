//! Text formatting shared by the pages; mirrors `hauz-tui` (debt: move into core if a third
//! view appears).

use hauz_core::bill::{Bill, Money, Status};

const ID_PREFIX_LEN: usize = 8;

pub(crate) fn id_prefix(bill: &Bill) -> String {
    bill.id().as_str().chars().take(ID_PREFIX_LEN).collect()
}

pub(crate) fn vendor(bill: &Bill) -> String {
    bill.vendor().map_or("-", |v| v.name()).to_owned()
}

pub(crate) fn amount(bill: &Bill) -> String {
    bill.amount().map_or_else(|| "-".to_owned(), money)
}

pub(crate) fn period(bill: &Bill) -> String {
    bill.period().map_or_else(
        || "-".to_owned(),
        |p| format!("{} to {}", p.start(), p.end()),
    )
}

pub(crate) fn date(d: Option<impl std::fmt::Display>) -> String {
    d.map_or_else(|| "-".to_owned(), |d| d.to_string())
}

pub(crate) fn status(s: Status) -> &'static str {
    match s {
        Status::Extracted => "extracted",
        Status::NeedsReview => "needs_review",
    }
}

/// Minor units at two decimals plus the currency code, the extractors' own assumption.
fn money(m: &Money) -> String {
    let minor = m.minor_units();
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    format!(
        "{sign}{}.{:02} {}",
        abs / 100,
        abs % 100,
        m.currency().as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::money;
    use hauz_core::bill::{Currency, Money};

    #[test]
    fn money_formats_minor_units() -> Result<(), Box<dyn std::error::Error>> {
        let cop = Currency::new("COP")?;
        assert_eq!(money(&Money::new(123_456, cop)), "1234.56 COP");
        assert_eq!(money(&Money::new(-5, Currency::new("USD")?)), "-0.05 USD");
        Ok(())
    }
}
