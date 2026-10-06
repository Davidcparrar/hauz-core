//! [e2e] acceptance tests for the bill browser's `App` + `draw` + `load` (feature #41),
//! rendered through ratatui's `TestBackend`.

mod common;

use common::{Result, extracted, needs_review, render, screen, two_bills};
use crossterm::event::KeyCode;
use hauz_core::store::{BillStore, SqliteStore};
use hauz_tui::{App, Flow, load};

#[test]
fn ac4_lists_both_rows_newest_first_with_fields() -> Result<()> {
    let app = App::new(two_bills()?);
    let lines = render(&app)?;

    let newest = lines.iter().position(|l| l.contains("bbbbbbbb"));
    let oldest = lines.iter().position(|l| l.contains("aaaaaaaa"));
    assert!(newest.is_some() && oldest.is_some());
    assert!(newest < oldest);

    let newest_row = lines.iter().find(|l| l.contains("bbbbbbbb Beta Water"));
    assert!(newest_row.is_some_and(|l| l.contains("Beta Water - - - needs_review")));
    assert!(lines.iter().any(|l| {
        l.contains("aaaaaaaa Acme Power 1234.56 COP 2026-01-05 2026-02-15 extracted")
    }));
    Ok(())
}

#[test]
fn ac5_moving_shows_every_field_and_up_clamps() -> Result<()> {
    let mut app = App::new(two_bills()?);
    assert_eq!(app.selected(), Some(&needs_review()?));

    assert_eq!(app.on_key(KeyCode::Down), Flow::Continue);
    assert_eq!(app.selected(), Some(&extracted()?));
    let screen_text = screen(&app)?;
    for field in [
        "aaaaaaaa-old-bill",
        "Acme Power",
        "1234.56 COP",
        "2026-01-01 to 2026-01-31",
        "2026-01-05",
        "2026-02-15",
        "extracted",
    ] {
        assert!(screen_text.contains(field), "missing {field}");
    }

    app.on_key(KeyCode::Down);
    assert_eq!(app.selected(), Some(&extracted()?));
    app.on_key(KeyCode::Char('k'));
    assert_eq!(app.selected(), Some(&needs_review()?));
    app.on_key(KeyCode::Up);
    assert_eq!(app.selected(), Some(&needs_review()?));
    app.on_key(KeyCode::Char('j'));
    assert_eq!(app.selected(), Some(&extracted()?));
    Ok(())
}

#[test]
fn ac6_r_toggles_needs_review_filter() -> Result<()> {
    let mut app = App::new(two_bills()?);
    app.on_key(KeyCode::Down);

    app.on_key(KeyCode::Char('r'));
    assert!(app.needs_review_only());
    assert_eq!(app.selected(), Some(&needs_review()?));
    let filtered = screen(&app)?;
    assert!(filtered.contains("needs_review only"));
    assert!(!filtered.contains("Acme Power"));

    app.on_key(KeyCode::Char('r'));
    assert!(!app.needs_review_only());
    assert_eq!(app.selected(), Some(&needs_review()?));
    let all = screen(&app)?;
    assert!(!all.contains("needs_review only"));
    assert!(all.contains("Acme Power"));
    Ok(())
}

#[test]
fn ac7_empty_view_shows_no_bills_and_moving_is_safe() -> Result<()> {
    let mut none = App::new(Vec::new());
    assert!(screen(&none)?.contains("no bills"));
    assert_eq!(none.selected(), None);
    for key in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Char('j'),
        KeyCode::Char('k'),
    ] {
        none.on_key(key);
    }
    assert_eq!(none.selected(), None);

    let mut filtered = App::new(vec![extracted()?]);
    filtered.on_key(KeyCode::Char('r'));
    assert!(screen(&filtered)?.contains("no bills"));
    assert_eq!(filtered.selected(), None);
    filtered.on_key(KeyCode::Down);
    Ok(())
}

#[test]
fn ac8_q_and_esc_quit_other_keys_continue() -> Result<()> {
    let mut app = App::new(two_bills()?);
    assert_eq!(app.on_key(KeyCode::Char('q')), Flow::Quit);
    assert_eq!(app.on_key(KeyCode::Esc), Flow::Quit);
    assert_eq!(app.on_key(KeyCode::Char('x')), Flow::Continue);
    assert_eq!(app.on_key(KeyCode::Enter), Flow::Continue);
    Ok(())
}

#[tokio::test]
async fn ac9_load_shows_every_stored_vendor() -> Result<()> {
    let db = common::tmp_dir().join("a.db");
    {
        let writer = SqliteStore::open(&db).await?;
        for (n, bill) in two_bills()?.iter().enumerate() {
            writer
                .insert(&hauz_core::store::RawHash::new([n as u8; 32]), bill)
                .await?;
        }
    }
    let mut app = load(&db).await?;
    let first = screen(&app)?;
    assert!(first.contains("Beta Water") && first.contains("Acme Power"));
    app.on_key(KeyCode::Down);
    assert!(screen(&app)?.contains("Acme Power"));
    Ok(())
}
