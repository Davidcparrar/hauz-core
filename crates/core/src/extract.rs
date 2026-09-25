//! Owns "document ⇒ candidate fields": [`Extractor`] turns an [`Envelope`] into a partial
//! [`Extraction`] (amount, dates, period, vendor), each field carrying a [`Confidence`] and
//! a [`Span`] into the source text it came from. [`merge`] combines several extractions
//! (e.g. one per [`Extractor`] impl, or text vs. HTML) by keeping the highest-confidence
//! value per field. [`TextExtractor`] is the first, dependency-free heuristic implementation:
//! a hand-written scanner over the text body and a tag-stripped HTML body. PDF text and an
//! LLM-backed implementation slot in later behind the same trait.

use std::cmp::Reverse;

use crate::bill::{BillingPeriod, Currency, Money, Vendor};
use crate::email::Envelope;

/// Errors this module can return. Library code never panics; it returns one of these.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A confidence value was outside `0..=100`.
    #[error("confidence must be 0..=100, got {0}")]
    InvalidConfidence(u8),
}

/// A validated confidence score in `0..=100`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Confidence(u8);

impl Confidence {
    /// Parse, don't validate.
    ///
    /// # Errors
    /// Returns [`Error::InvalidConfidence`] when `value` exceeds 100.
    pub fn new(value: u8) -> Result<Self, Error> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(Error::InvalidConfidence(value))
        }
    }

    /// The validated score.
    #[must_use]
    pub fn get(&self) -> u8 {
        self.0
    }
}

/// Which document a [`Span`] indexes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Source {
    /// The envelope's plain-text body.
    Text,
    /// The envelope's HTML body, after tag-stripping.
    Html,
    /// The `n`th attachment's extracted text (future PDF extractor).
    Document(usize),
}

/// A byte range `[start, end)` into the scanned string named by `source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Which document this span indexes into.
    pub source: Source,
    /// The inclusive start byte offset.
    pub start: usize,
    /// The exclusive end byte offset.
    pub end: usize,
}

/// One extracted value, with how confident the extractor is and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<T> {
    /// The extracted value.
    pub value: T,
    /// How confident the extractor is in `value`.
    pub confidence: Confidence,
    /// Where in the source document `value` was found.
    pub span: Span,
}

/// A partial set of candidate bill fields, each optional and independently sourced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extraction {
    /// The bill amount, when found.
    pub amount: Option<Field<Money>>,
    /// The invoice/issue date, when found.
    pub issued: Option<Field<time::Date>>,
    /// The due date, when found.
    pub due: Option<Field<time::Date>>,
    /// The billing period, when found.
    pub period: Option<Field<BillingPeriod>>,
    /// The vendor, when found.
    pub vendor: Option<Field<Vendor>>,
}

/// Something that can turn an [`Envelope`] into a partial [`Extraction`]. Implementations may
/// read text, HTML, or attachments; a heuristic pass, PDF text, and an LLM-backed pass all
/// implement this trait so callers can run several and [`merge`] the results.
pub trait Extractor: Send + Sync {
    /// Extracts candidate fields from `envelope`.
    ///
    /// # Errors
    /// Returns [`Error`] when the implementation cannot build a valid field (e.g. an internal
    /// confidence computation is out of range).
    fn extract(&self, envelope: &Envelope) -> Result<Extraction, Error>;
}

/// Combines several extractions into one, keeping the highest-confidence value for each
/// field. Ties break on a structural order of the value (smallest wins), then on the
/// smallest [`Span`].
#[must_use]
pub fn merge(extractions: Vec<Extraction>) -> Extraction {
    let mut result = Extraction::default();
    for extraction in extractions {
        merge_field(&mut result.amount, extraction.amount, |money: &Money| {
            (money.minor_units(), money.currency().as_str().to_string())
        });
        merge_field(
            &mut result.issued,
            extraction.issued,
            |date: &time::Date| *date,
        );
        merge_field(&mut result.due, extraction.due, |date: &time::Date| *date);
        merge_field(
            &mut result.period,
            extraction.period,
            |period: &BillingPeriod| (period.start(), period.end()),
        );
        merge_field(&mut result.vendor, extraction.vendor, |vendor: &Vendor| {
            vendor.name().to_string()
        });
    }
    result
}

/// Keeps `candidate` over `*acc` when `candidate` has higher confidence, or (tied) a smaller
/// structural key, or (tied again) a smaller span.
fn merge_field<T, K, F>(acc: &mut Option<Field<T>>, candidate: Option<Field<T>>, key: F)
where
    K: Ord,
    F: Fn(&T) -> K,
{
    let Some(candidate) = candidate else {
        return;
    };
    let replace = match acc {
        None => true,
        Some(current) => {
            let candidate_rank = (
                candidate.confidence,
                Reverse(key(&candidate.value)),
                Reverse(candidate.span),
            );
            let current_rank = (
                current.confidence,
                Reverse(key(&current.value)),
                Reverse(current.span),
            );
            candidate_rank > current_rank
        }
    };
    if replace {
        *acc = Some(candidate);
    }
}

// ---------------------------------------------------------------------------------------
// TextExtractor: heuristic scanner over text and stripped-HTML bodies.
// ---------------------------------------------------------------------------------------

const AMOUNT_ANCHORED: u8 = 90;
const AMOUNT_UNANCHORED: u8 = 40;
const DATE_ANCHORED: u8 = 90;
const DATE_FALLBACK: u8 = 40;
const VENDOR_CONFIDENCE: u8 = 20;

const AMOUNT_ANCHORS: [&str; 7] = [
    "total",
    "amount due",
    "balance due",
    "gesamtbetrag",
    "total a pagar",
    "importe total",
    "montant total",
];
const DUE_ANCHORS: [&str; 5] = ["due", "pay by", "fällig", "vencimiento", "échéance"];
const ISSUED_ANCHORS: [&str; 4] = ["invoice date", "date", "rechnungsdatum", "fecha"];

/// The maximum byte distance an anchor keyword may sit before the value it introduces.
const ANCHOR_WINDOW: usize = 40;

/// A dependency-free heuristic [`Extractor`]: scans the text body and a tag-stripped HTML
/// body for an amount, issue date, due date, and sender-domain vendor. Never sets `period`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextExtractor;

impl Extractor for TextExtractor {
    fn extract(&self, envelope: &Envelope) -> Result<Extraction, Error> {
        let mut parts = Vec::new();
        if let Some(text) = envelope.text.as_deref() {
            parts.push(scan(text, Source::Text)?);
        }
        if let Some(html) = envelope.html.as_deref() {
            let stripped = strip_html(html);
            parts.push(scan(&stripped, Source::Html)?);
        }
        let mut result = merge(parts);
        result.vendor = vendor_of(&envelope.sender)?;
        Ok(result)
    }
}

/// Scans one already-plain-text source (verbatim `text`, or stripped `html`) for amount and
/// dates. Never sets `period` or `vendor` (those are not per-source).
fn scan(text: &str, source: Source) -> Result<Extraction, Error> {
    let amounts = find_amounts(text);
    let dates = find_dates(text);

    let amount = pick_amount(text, &amounts, source)?;
    let due = pick_due(text, &dates, source)?;
    let issued = pick_issued(text, &dates, due.as_ref(), source)?;

    Ok(Extraction {
        amount,
        issued,
        due,
        period: None,
        vendor: None,
    })
}

fn pick_amount(
    text: &str,
    amounts: &[AmountMatch],
    source: Source,
) -> Result<Option<Field<Money>>, Error> {
    if let Some(found) = anchored(text, &AMOUNT_ANCHORS, amounts, |a| a.start, |a| a.end) {
        let confidence = Confidence::new(AMOUNT_ANCHORED)?;
        return Ok(Some(field_of(found, confidence, source, |a| {
            a.money.clone()
        })));
    }
    if let Some(found) = amounts.iter().max_by_key(|a| a.money.minor_units()) {
        let confidence = Confidence::new(AMOUNT_UNANCHORED)?;
        return Ok(Some(field_of(found, confidence, source, |a| {
            a.money.clone()
        })));
    }
    Ok(None)
}

fn pick_due(
    text: &str,
    dates: &[DateMatch],
    source: Source,
) -> Result<Option<Field<time::Date>>, Error> {
    if let Some(found) = anchored(text, &DUE_ANCHORS, dates, |d| d.start, |d| d.end) {
        let confidence = Confidence::new(DATE_ANCHORED)?;
        return Ok(Some(field_of(found, confidence, source, |d| d.date)));
    }
    Ok(None)
}

fn pick_issued(
    text: &str,
    dates: &[DateMatch],
    due: Option<&Field<time::Date>>,
    source: Source,
) -> Result<Option<Field<time::Date>>, Error> {
    let due_span = due.map(|field| (field.span.start, field.span.end));
    let not_due = |d: &&DateMatch| Some((d.start, d.end)) != due_span;

    for anchor_end in anchor_positions(text, &ISSUED_ANCHORS) {
        let found = dates
            .iter()
            .filter(|d| d.start >= anchor_end && d.start <= anchor_end + ANCHOR_WINDOW)
            .filter(not_due)
            .min_by_key(|d| d.start);
        if let Some(found) = found {
            let confidence = Confidence::new(DATE_ANCHORED)?;
            return Ok(Some(field_of(found, confidence, source, |d| d.date)));
        }
    }

    if let Some(found) = dates.iter().filter(not_due).min_by_key(|d| d.start) {
        let confidence = Confidence::new(DATE_FALLBACK)?;
        return Ok(Some(field_of(found, confidence, source, |d| d.date)));
    }
    Ok(None)
}

/// Builds a [`Field`] from a match, its confidence, and which source it was found in.
fn field_of<M, T>(
    found: &M,
    confidence: Confidence,
    source: Source,
    value: impl Fn(&M) -> T,
) -> Field<T>
where
    M: Spanned,
{
    Field {
        value: value(found),
        confidence,
        span: Span {
            source,
            start: found.start(),
            end: found.end(),
        },
    }
}

/// A match with a byte span, so [`field_of`] and [`anchored`] can work generically over
/// [`AmountMatch`] and [`DateMatch`].
trait Spanned {
    fn start(&self) -> usize;
    fn end(&self) -> usize;
}

/// Finds the first anchor (in order of appearance) that has a match within
/// [`ANCHOR_WINDOW`] bytes after it, and returns the earliest such match.
fn anchored<'a, M>(
    text: &str,
    anchors: &[&str],
    matches: &'a [M],
    start: impl Fn(&M) -> usize,
    end: impl Fn(&M) -> usize,
) -> Option<&'a M> {
    for anchor_end in anchor_positions(text, anchors) {
        let found = matches
            .iter()
            .filter(|m| start(m) >= anchor_end && start(m) <= anchor_end + ANCHOR_WINDOW)
            .min_by_key(|m| start(m));
        if found.is_some() {
            return found;
        }
    }
    let _ = end;
    None
}

/// Byte offsets right after every case-insensitive, word-boundary occurrence of any anchor
/// in `anchors`, in order of appearance.
fn anchor_positions(text: &str, anchors: &[&str]) -> Vec<usize> {
    let mut positions = Vec::new();
    for (i, _) in text.char_indices() {
        let preceded_by_alpha = text
            .get(..i)
            .and_then(|prefix| prefix.chars().next_back())
            .is_some_and(|c| c.is_ascii_alphabetic());
        if preceded_by_alpha {
            continue;
        }
        for anchor in anchors {
            let Some(window) = text.get(i..).and_then(|rest| rest.get(..anchor.len())) else {
                continue;
            };
            if !window.eq_ignore_ascii_case(anchor) {
                continue;
            }
            let followed_by_alpha = text
                .get(i + anchor.len()..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|c| c.is_ascii_alphabetic());
            if followed_by_alpha {
                continue;
            }
            positions.push(i + anchor.len());
            break;
        }
    }
    positions
}

/// The domain after `@` in `sender`, lowercased, low confidence, span `Text 0..0`. `None`
/// when `sender` has no `@` or the domain is not a valid [`Vendor`].
fn vendor_of(sender: &str) -> Result<Option<Field<Vendor>>, Error> {
    let Some((_, domain)) = sender.split_once('@') else {
        return Ok(None);
    };
    let Ok(vendor) = Vendor::new(&domain.to_lowercase()) else {
        return Ok(None);
    };
    let confidence = Confidence::new(VENDOR_CONFIDENCE)?;
    Ok(Some(Field {
        value: vendor,
        confidence,
        span: Span {
            source: Source::Text,
            start: 0,
            end: 0,
        },
    }))
}

// ---------------------------------------------------------------------------------------
// Amount scanning
// ---------------------------------------------------------------------------------------

/// A matched amount and the byte span (digits plus adjacent currency marker) it came from.
struct AmountMatch {
    money: Money,
    start: usize,
    end: usize,
}

impl Spanned for AmountMatch {
    fn start(&self) -> usize {
        self.start
    }
    fn end(&self) -> usize {
        self.end
    }
}

/// Finds every amount in `text`: a digit-group numeral immediately adjacent (≤1 space) to a
/// currency symbol or 3-letter code. A numeral with no adjacent currency marker is not an
/// amount and is skipped.
fn find_amounts(text: &str) -> Vec<AmountMatch> {
    let mut results = Vec::new();
    for (token_start, token_end) in number_tokens(text) {
        let Some(token) = text.get(token_start..token_end) else {
            continue;
        };
        let Some(minor_units) = parse_amount_value(token) else {
            continue;
        };
        let currency_match = match_currency_after(text, token_end)
            .or_else(|| match_currency_before(text, token_start));
        let Some(currency_match) = currency_match else {
            continue;
        };
        let Ok(currency) = Currency::new(&currency_match.code) else {
            continue;
        };
        results.push(AmountMatch {
            money: Money::new(minor_units, currency),
            start: token_start.min(currency_match.start),
            end: token_end.max(currency_match.end),
        });
    }
    results
}

/// Byte ranges of maximal digit-group numerals: a run of ASCII digits, optionally continued
/// by more digit runs separated by `.`, `,`, ' ', or NBSP. Always starts and ends with a
/// digit.
fn number_tokens(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let Some(&(start, c)) = chars.get(i) else {
            break;
        };
        if !c.is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut end = start + c.len_utf8();
        loop {
            while let Some(&(idx, next)) = chars.get(j + 1) {
                if !next.is_ascii_digit() {
                    break;
                }
                j += 1;
                end = idx + next.len_utf8();
            }
            let Some(&(_, sep)) = chars.get(j + 1) else {
                break;
            };
            if !is_amount_separator(sep) {
                break;
            }
            let Some(&(next_idx, next_digit)) = chars.get(j + 2) else {
                break;
            };
            if !next_digit.is_ascii_digit() {
                break;
            }
            j += 2;
            end = next_idx + next_digit.len_utf8();
        }
        tokens.push((start, end));
        i = j + 1;
    }
    tokens
}

fn is_amount_separator(c: char) -> bool {
    matches!(c, '.' | ',' | ' ' | '\u{00A0}')
}

/// Parses a digit-group numeral (as found by [`number_tokens`]) into minor units, assuming
/// two decimal places. The decimal separator is the last of `.`/`,` when both occur, or a
/// lone one followed by exactly two digits; otherwise every `.`/`,`/space/NBSP is grouping.
fn parse_amount_value(token: &str) -> Option<i64> {
    let last_dot = token.rfind('.');
    let last_comma = token.rfind(',');
    let dot_count = token.matches('.').count();
    let comma_count = token.matches(',').count();

    let decimal_pos = match (last_dot, last_comma) {
        (Some(d), Some(c)) => Some(d.max(c)),
        (Some(d), None) if dot_count == 1 && has_two_digits_after(token, d) => Some(d),
        (None, Some(c)) if comma_count == 1 && has_two_digits_after(token, c) => Some(c),
        _ => None,
    };

    let (int_part, frac_part) = match decimal_pos {
        Some(pos) => {
            let int_raw = token.get(..pos)?;
            let frac_raw = token.get(pos + 1..)?;
            (strip_grouping(int_raw), frac_raw.to_string())
        }
        None => (strip_grouping(token), String::new()),
    };
    if int_part.is_empty() {
        return None;
    }
    let integer: i64 = int_part.parse().ok()?;
    let frac = normalize_frac(&frac_part)?;
    integer.checked_mul(100)?.checked_add(frac)
}

/// Whether the separator at byte offset `pos` in `token` is followed by exactly two digits
/// and nothing else.
fn has_two_digits_after(token: &str, pos: usize) -> bool {
    let Some(after) = token.get(pos + 1..) else {
        return false;
    };
    after.chars().count() == 2 && after.chars().all(|c| c.is_ascii_digit())
}

/// Keeps only ASCII digits (drops `.`, `,`, space, and NBSP grouping separators).
fn strip_grouping(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// Normalizes a fractional-part string to exactly two digits: pads a shorter one with
/// trailing zeros, truncates a longer one.
fn normalize_frac(frac: &str) -> Option<i64> {
    let mut chars = frac.chars();
    let first = chars.next().unwrap_or('0');
    let second = chars.next().unwrap_or('0');
    if !first.is_ascii_digit() || !second.is_ascii_digit() {
        return None;
    }
    let mut value = String::with_capacity(2);
    value.push(first);
    value.push(second);
    value.parse().ok()
}

struct CurrencyMatch {
    code: String,
    start: usize,
    end: usize,
}

/// Looks for a currency symbol or 3-letter code starting at `from`, allowing up to one
/// space/NBSP between `from` and the marker.
fn match_currency_after(text: &str, from: usize) -> Option<CurrencyMatch> {
    let rest = text.get(from..)?;
    let (skip, rest) = skip_one_space(rest);
    if let Some((code, len)) = symbol_marker(rest) {
        return Some(CurrencyMatch {
            code,
            start: from,
            end: from + skip + len,
        });
    }
    if let Some((code, len)) = code_marker(rest) {
        return Some(CurrencyMatch {
            code,
            start: from,
            end: from + skip + len,
        });
    }
    None
}

/// Looks for a currency symbol or 3-letter code ending at `before`, allowing up to one
/// space/NBSP between the marker and `before`.
fn match_currency_before(text: &str, before: usize) -> Option<CurrencyMatch> {
    let prefix = text.get(..before)?;
    let trimmed_end = trim_one_trailing_space(prefix);

    if let Some(c) = text.get(..trimmed_end)?.chars().next_back() {
        if let Some(code) = symbol_currency(c) {
            return Some(CurrencyMatch {
                code: code.to_string(),
                start: trimmed_end - c.len_utf8(),
                end: before,
            });
        }
    }
    let start = trimmed_end.checked_sub(3)?;
    let candidate = text.get(start..trimmed_end)?;
    if candidate.len() == 3 && candidate.chars().all(|c| c.is_ascii_uppercase()) {
        let boundary_ok = text
            .get(..start)
            .and_then(|p| p.chars().next_back())
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if boundary_ok {
            return Some(CurrencyMatch {
                code: candidate.to_string(),
                start,
                end: before,
            });
        }
    }
    None
}

fn skip_one_space(s: &str) -> (usize, &str) {
    if let Some(c) = s.chars().next() {
        if c == ' ' || c == '\u{00A0}' {
            let len = c.len_utf8();
            return (len, s.get(len..).unwrap_or(""));
        }
    }
    (0, s)
}

/// Byte offset marking the end of `s` with at most one trailing space/NBSP removed.
fn trim_one_trailing_space(s: &str) -> usize {
    if let Some(c) = s.chars().next_back() {
        if c == ' ' || c == '\u{00A0}' {
            return s.len() - c.len_utf8();
        }
    }
    s.len()
}

fn symbol_currency(c: char) -> Option<&'static str> {
    match c {
        '€' => Some("EUR"),
        '$' => Some("USD"),
        '£' => Some("GBP"),
        _ => None,
    }
}

fn symbol_marker(s: &str) -> Option<(String, usize)> {
    let c = s.chars().next()?;
    let code = symbol_currency(c)?;
    Some((code.to_string(), c.len_utf8()))
}

fn code_marker(s: &str) -> Option<(String, usize)> {
    let head = s.get(..3)?;
    if head.len() == 3 && head.chars().all(|c| c.is_ascii_uppercase()) {
        let boundary_ok = s
            .get(3..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if boundary_ok {
            return Some((head.to_string(), 3));
        }
    }
    None
}

// ---------------------------------------------------------------------------------------
// Date scanning
// ---------------------------------------------------------------------------------------

struct DateMatch {
    date: time::Date,
    start: usize,
    end: usize,
}

impl Spanned for DateMatch {
    fn start(&self) -> usize {
        self.start
    }
    fn end(&self) -> usize {
        self.end
    }
}

const MONTH_NAMES: [(&str, &str, time::Month); 12] = [
    ("jan", "january", time::Month::January),
    ("feb", "february", time::Month::February),
    ("mar", "march", time::Month::March),
    ("apr", "april", time::Month::April),
    ("may", "may", time::Month::May),
    ("jun", "june", time::Month::June),
    ("jul", "july", time::Month::July),
    ("aug", "august", time::Month::August),
    ("sep", "september", time::Month::September),
    ("oct", "october", time::Month::October),
    ("nov", "november", time::Month::November),
    ("dec", "december", time::Month::December),
];

/// Finds every date in `text`, matching `yyyy-mm-dd`, `dd.mm.yyyy`, `dd/mm/yyyy`,
/// `d Mon yyyy`, `d Month yyyy`, and `Month d, yyyy` (English months, case-insensitive), in
/// order of appearance.
fn find_dates(text: &str) -> Vec<DateMatch> {
    let mut matches = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let Some(rest) = text.get(i..) else {
            i += 1;
            continue;
        };
        let found = try_ymd(rest)
            .or_else(|| try_dmy(rest, '.'))
            .or_else(|| try_dmy(rest, '/'))
            .or_else(|| try_day_month_year(rest))
            .or_else(|| try_month_day_year(rest));
        if let Some((date, len)) = found {
            matches.push(DateMatch {
                date,
                start: i,
                end: i + len,
            });
            i += len;
        } else {
            let step = rest.chars().next().map_or(1, char::len_utf8);
            i += step;
        }
    }
    matches
}

fn take_digits(s: &str, n: usize) -> Option<(u32, &str)> {
    let head = s.get(..n)?;
    if head.len() != n || !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: u32 = head.parse().ok()?;
    Some((value, s.get(n..)?))
}

/// Takes up to `max` leading ASCII digits (at least one), for unpadded day numbers.
fn take_digits_range(s: &str, max: usize) -> Option<(u32, &str)> {
    let len = s.chars().take(max).take_while(char::is_ascii_digit).count();
    if len == 0 {
        return None;
    }
    let head = s.get(..len)?;
    Some((head.parse().ok()?, s.get(len..)?))
}

fn month_from_number(n: u32) -> Option<time::Month> {
    time::Month::try_from(u8::try_from(n).ok()?).ok()
}

fn take_month(s: &str) -> Option<(time::Month, &str)> {
    for (abbr, full, month) in MONTH_NAMES {
        for name in [full, abbr] {
            let Some(head) = s.get(..name.len()) else {
                continue;
            };
            if !head.eq_ignore_ascii_case(name) {
                continue;
            }
            let rest = s.get(name.len()..).unwrap_or("");
            let boundary_ok = rest.chars().next().is_none_or(|c| !c.is_ascii_alphabetic());
            if boundary_ok {
                return Some((month, rest));
            }
        }
    }
    None
}

fn build_date(year: u32, month: time::Month, day: u32) -> Option<time::Date> {
    let year = i32::try_from(year).ok()?;
    let day = u8::try_from(day).ok()?;
    time::Date::from_calendar_date(year, month, day).ok()
}

/// `yyyy-mm-dd`
fn try_ymd(s: &str) -> Option<(time::Date, usize)> {
    let (year, rest) = take_digits(s, 4)?;
    let rest = rest.strip_prefix('-')?;
    let (month, rest) = take_digits(rest, 2)?;
    let rest = rest.strip_prefix('-')?;
    let (day, _rest) = take_digits(rest, 2)?;
    let date = build_date(year, month_from_number(month)?, day)?;
    Some((date, 10))
}

/// `dd.mm.yyyy` or `dd/mm/yyyy`
fn try_dmy(s: &str, sep: char) -> Option<(time::Date, usize)> {
    let (day, rest) = take_digits(s, 2)?;
    let rest = rest.strip_prefix(sep)?;
    let (month, rest) = take_digits(rest, 2)?;
    let rest = rest.strip_prefix(sep)?;
    let (year, _rest) = take_digits(rest, 4)?;
    let date = build_date(year, month_from_number(month)?, day)?;
    Some((date, 10))
}

/// `d Mon yyyy` / `d Month yyyy`
fn try_day_month_year(s: &str) -> Option<(time::Date, usize)> {
    let (day, rest) = take_digits_range(s, 2)?;
    let rest = rest.strip_prefix(' ')?;
    let (month, rest) = take_month(rest)?;
    let rest = rest.strip_prefix(' ')?;
    let (year, rest) = take_digits(rest, 4)?;
    let date = build_date(year, month, day)?;
    Some((date, s.len().checked_sub(rest.len())?))
}

/// `Month d, yyyy`
fn try_month_day_year(s: &str) -> Option<(time::Date, usize)> {
    let (month, rest) = take_month(s)?;
    let rest = rest.strip_prefix(' ')?;
    let (day, rest) = take_digits_range(rest, 2)?;
    let rest = rest.strip_prefix(',')?;
    let rest = rest.strip_prefix(' ')?;
    let (year, rest) = take_digits(rest, 4)?;
    let date = build_date(year, month, day)?;
    Some((date, s.len().checked_sub(rest.len())?))
}

// ---------------------------------------------------------------------------------------
// HTML stripping
// ---------------------------------------------------------------------------------------

const BLOCK_TAGS: [&str; 20] = [
    "td",
    "tr",
    "th",
    "p",
    "div",
    "table",
    "li",
    "ul",
    "ol",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "tbody",
    "thead",
    "tfoot",
    "section",
    "blockquote",
];

/// Drops HTML tags: block/cell closing tags and `<br>` become a single whitespace character;
/// every other tag is dropped outright. Decodes a fixed entity table.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        let Some(rest) = html.get(i..) else {
            break;
        };
        if rest.starts_with('<') {
            let Some(rel_end) = rest.find('>') else {
                break;
            };
            let Some(tag) = rest.get(..=rel_end) else {
                break;
            };
            if is_whitespace_tag(tag) {
                out.push(' ');
            }
            i += rel_end + 1;
            continue;
        }
        let next_lt = rest.find('<').unwrap_or(rest.len());
        let Some(chunk) = rest.get(..next_lt) else {
            break;
        };
        decode_entities_into(chunk, &mut out);
        i += next_lt;
    }
    out
}

/// Whether `tag` (including its `<` and `>`) is a block/cell closing tag or `<br>`.
fn is_whitespace_tag(tag: &str) -> bool {
    let Some(inner) = tag.get(1..tag.len().saturating_sub(1)) else {
        return false;
    };
    let inner = inner.trim();
    if let Some(name) = inner.strip_prefix('/') {
        let name = name.trim().to_ascii_lowercase();
        return BLOCK_TAGS.contains(&name.as_str());
    }
    let name = inner
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    name == "br"
}

fn decode_entities_into(chunk: &str, out: &mut String) {
    let mut rest = chunk;
    while let Some(amp_pos) = rest.find('&') {
        let Some(before) = rest.get(..amp_pos) else {
            break;
        };
        out.push_str(before);
        let Some(after) = rest.get(amp_pos + 1..) else {
            break;
        };
        if let Some(semi_rel) = after.find(';').filter(|&p| p <= 10) {
            let Some(entity) = after.get(..semi_rel) else {
                out.push('&');
                rest = after;
                continue;
            };
            if let Some(decoded) = decode_entity(entity) {
                out.push(decoded);
                rest = after.get(semi_rel + 1..).unwrap_or("");
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "nbsp" => Some('\u{00A0}'),
        "euro" => Some('€'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "pound" => Some('£'),
        _ => {
            let digits = name.strip_prefix('#')?;
            char::from_u32(digits.parse().ok()?)
        }
    }
}
