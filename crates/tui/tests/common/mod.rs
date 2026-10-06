//! Shared fixtures for `tui`'s [e2e] tests.
#![allow(dead_code)] // not every fixture is used by every test file

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hauz_core::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
use hauz_tui::{App, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use time::macros::date;

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// A fresh, unique directory under the system temp dir.
pub(crate) fn tmp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("hauz-tui-{pid}-{nanos}-{n}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A complete `Extracted` bill: Acme Power, 1234.56 COP.
pub(crate) fn extracted() -> Result<Bill> {
    Ok(Bill::try_from(BillDraft {
        id: BillId::new("aaaaaaaa-old-bill")?,
        vendor: Some(Vendor::new("Acme Power")?),
        amount: Some(Money::new(123_456, Currency::new("COP")?)),
        period: Some(BillingPeriod::new(
            date!(2026 - 01 - 01),
            date!(2026 - 01 - 31),
        )?),
        issued: Some(date!(2026 - 01 - 05)),
        due: Some(date!(2026 - 02 - 15)),
        status: Status::Extracted,
    })?)
}

/// An amountless `NeedsReview` bill: Beta Water, no dates.
pub(crate) fn needs_review() -> Result<Bill> {
    Ok(Bill::try_from(BillDraft {
        id: BillId::new("bbbbbbbb-new-bill")?,
        vendor: Some(Vendor::new("Beta Water")?),
        amount: None,
        period: None,
        issued: None,
        due: None,
        status: Status::NeedsReview,
    })?)
}

/// `list` order: oldest first.
pub(crate) fn two_bills() -> Result<Vec<Bill>> {
    Ok(vec![extracted()?, needs_review()?])
}

/// Renders `app` on a 160x24 `TestBackend`, one `String` per terminal row.
pub(crate) fn render(app: &App) -> Result<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(160, 24))?;
    terminal.draw(|frame| draw(frame, app))?;
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    let cells: Vec<&str> = buffer.content().iter().map(|c| c.symbol()).collect();
    Ok(cells.chunks(width).map(|row| row.concat()).collect())
}

/// The whole render as one string.
pub(crate) fn screen(app: &App) -> Result<String> {
    Ok(render(app)?.join("\n"))
}
