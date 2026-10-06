//! Rendering: list left, detail right.

use hauz_core::bill::{Bill, Money, Status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::App;

const ID_PREFIX_LEN: usize = 8;

/// Draws the whole browser into `frame`.
pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas(frame.area());

    let visible = app.visible();
    let title = if app.needs_review_only() {
        " bills (needs_review only) "
    } else {
        " bills "
    };
    let list_block = Block::bordered().title(title);
    if visible.is_empty() {
        frame.render_widget(Paragraph::new("no bills").block(list_block), left);
    } else {
        let items: Vec<ListItem<'_>> = visible.iter().map(|b| ListItem::new(row(b))).collect();
        let list = List::new(items)
            .block(list_block)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default().with_selected(Some(app.selected_index()));
        frame.render_stateful_widget(list, left, &mut state);
    }

    let detail = app.selected().map(detail).unwrap_or_default();
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" detail ")),
        right,
    );
}

fn row(bill: &Bill) -> String {
    let id: String = bill.id().as_str().chars().take(ID_PREFIX_LEN).collect();
    format!(
        "{id} {} {} {} {} {}",
        vendor(bill),
        amount(bill),
        date(bill.issued()),
        date(bill.due()),
        status(bill.status()),
    )
}

fn detail(bill: &Bill) -> String {
    let period = bill.period().map_or_else(
        || "-".to_owned(),
        |p| format!("{} to {}", p.start(), p.end()),
    );
    format!(
        "id: {}\nvendor: {}\namount: {}\nperiod: {period}\nissued: {}\ndue: {}\nstatus: {}",
        bill.id().as_str(),
        vendor(bill),
        amount(bill),
        date(bill.issued()),
        date(bill.due()),
        status(bill.status()),
    )
}

fn vendor(bill: &Bill) -> &str {
    bill.vendor().map_or("-", |v| v.name())
}

fn amount(bill: &Bill) -> String {
    bill.amount().map_or_else(|| "-".to_owned(), money)
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

fn date(d: Option<impl std::fmt::Display>) -> String {
    d.map_or_else(|| "-".to_owned(), |d| d.to_string())
}

fn status(s: Status) -> &'static str {
    match s {
        Status::Extracted => "extracted",
        Status::NeedsReview => "needs_review",
    }
}

#[cfg(test)]
mod tests {
    use super::money;
    use hauz_core::bill::{Currency, Money};

    #[test]
    fn money_formats_minor_units() -> Result<(), Box<dyn std::error::Error>> {
        let cop = Currency::new("COP")?;
        assert_eq!(money(&Money::new(123_456, cop.clone())), "1234.56 COP");
        assert_eq!(money(&Money::new(-5, Currency::new("USD")?)), "-0.05 USD");
        Ok(())
    }
}
