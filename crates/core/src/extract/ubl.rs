//! `XmlInvoiceExtractor`: exact fields from a DIAN e-invoice zip (UBL `AttachedDocument`
//! wrapping an `Invoice` in CDATA). See `features/27/spec.md` for the field map; no LLM, no
//! new dependency, confidence always 100.

use std::collections::BTreeSet;

use super::xml::{Element, find_element};
use super::{Confidence, Error, Extraction, Field, Source, Span, merge};
use crate::BoxFuture;
use crate::bill::{BillingPeriod, Currency, Money, Vendor};
use crate::email::Envelope;
use crate::extract::Extractor;
use crate::zip;

/// `Content-Type` values this extractor opens as a zip archive.
const ZIP_MIMES: [&str; 2] = ["application/zip", "application/x-zip-compressed"];

/// Every field this extractor sets carries this confidence: the DIAN UBL shape is exact, not
/// heuristic.
const CONFIDENCE: u8 = 100;

/// Exact extractor for DIAN e-invoice zip attachments: opens each `application/zip`
/// attachment with [`crate::zip`], finds the first `*.xml` entry whose text holds an
/// `Invoice` start tag, and plucks amount, vendor, issue/due dates and (when declared)
/// billing period from it. Attachments that are not a zip, or whose zip holds no `Invoice`
/// root (e.g. a `CreditNote`), contribute nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XmlInvoiceExtractor;

impl XmlInvoiceExtractor {
    fn extract_sync(&self, envelope: &Envelope) -> Result<Extraction, Error> {
        let mut parts = Vec::new();
        for (index, document) in envelope.documents.iter().enumerate() {
            if !ZIP_MIMES.contains(&document.mime.as_str()) {
                continue;
            }
            let entries = zip::read(&document.bytes).map_err(|source| Error::Zip {
                document: index,
                source,
            })?;
            let Some(xml) = invoice_xml_text(&entries) else {
                continue;
            };
            parts.push(extraction_of(&xml, index)?);
        }
        Ok(merge(parts))
    }
}

impl Extractor for XmlInvoiceExtractor {
    fn extract<'a>(&'a self, envelope: &'a Envelope) -> BoxFuture<'a, Result<Extraction, Error>> {
        Box::pin(async move { self.extract_sync(envelope) })
    }
}

/// The first `*.xml` entry (ASCII case-insensitive), decoded with `from_utf8_lossy`, whose
/// text holds an `Invoice` start tag (any prefix).
fn invoice_xml_text(entries: &[zip::Entry]) -> Option<String> {
    entries
        .iter()
        .filter(|entry| entry.name.to_ascii_lowercase().ends_with(".xml"))
        .map(|entry| String::from_utf8_lossy(&entry.bytes).into_owned())
        .find(|text| find_element(text, &[], "Invoice").is_some())
}

/// Builds one document's [`Extraction`] from its `Invoice` XML text.
fn extraction_of(xml: &str, document: usize) -> Result<Extraction, Error> {
    Ok(Extraction {
        amount: amount_field(xml, document)?,
        issued: date_field(xml, &["Invoice"], "IssueDate", document)?,
        due: due_field(xml, document)?,
        period: period_field(xml, document)?,
        vendor: vendor_field(xml, document)?,
        notes: BTreeSet::new(),
    })
}

/// `LegalMonetaryTotal/PayableAmount` text (`digits[.d{1,2}]`, scaled to 2 decimals) with its
/// `currencyID` attribute via [`Currency::new`]. Either failing its grammar/constructor omits
/// the field.
fn amount_field(xml: &str, document: usize) -> Result<Option<Field<Money>>, Error> {
    let Some(element) = find_element(xml, &["Invoice", "LegalMonetaryTotal"], "PayableAmount")
    else {
        return Ok(None);
    };
    let Some(minor_units) = parse_amount(&element.text) else {
        return Ok(None);
    };
    let Some(currency_raw) = element.attr("currencyID") else {
        return Ok(None);
    };
    let Ok(currency) = Currency::new(&currency_raw) else {
        return Ok(None);
    };
    Ok(Some(money_field(
        Money::new(minor_units, currency),
        &element,
        document,
    )?))
}

/// Parses `digits[.d{1,2}]` (no grouping separators, no sign) into minor units scaled to two
/// decimal places; anything else is `None` rather than a best-effort guess.
fn parse_amount(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    let (int_part, frac_part) = match trimmed.split_once('.') {
        Some((int_part, frac_part)) => (int_part, Some(frac_part)),
        None => (trimmed, None),
    };
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let integer: i64 = int_part.parse().ok()?;
    let minor: i64 = match frac_part {
        None => 0,
        Some(frac) if frac.len() == 1 && frac.bytes().all(|b| b.is_ascii_digit()) => {
            frac.parse::<i64>().ok()?.checked_mul(10)?
        }
        Some(frac) if frac.len() == 2 && frac.bytes().all(|b| b.is_ascii_digit()) => {
            frac.parse().ok()?
        }
        Some(_) => return None,
    };
    integer.checked_mul(100)?.checked_add(minor)
}

/// Under `AccountingSupplierParty/Party`, the first non-empty of `PartyTaxScheme
/// /RegistrationName`, `PartyLegalEntity/RegistrationName`, `PartyName/Name`.
fn vendor_field(xml: &str, document: usize) -> Result<Option<Field<Vendor>>, Error> {
    const CANDIDATES: [(&str, &str); 3] = [
        ("PartyTaxScheme", "RegistrationName"),
        ("PartyLegalEntity", "RegistrationName"),
        ("PartyName", "Name"),
    ];
    for (parent, local) in CANDIDATES {
        let parents = ["Invoice", "AccountingSupplierParty", "Party", parent];
        let Some(element) = find_element(xml, &parents, local) else {
            continue;
        };
        if element.text.trim().is_empty() {
            continue;
        }
        let Ok(vendor) = Vendor::new(&element.text) else {
            continue;
        };
        let confidence = Confidence::new(CONFIDENCE)?;
        return Ok(Some(Field {
            value: vendor,
            confidence,
            span: span_of(&element, document),
        }));
    }
    Ok(None)
}

/// `PaymentMeans/PaymentDueDate`, else `Invoice/DueDate`.
fn due_field(xml: &str, document: usize) -> Result<Option<Field<time::Date>>, Error> {
    if let Some(field) = date_field(
        xml,
        &["Invoice", "PaymentMeans"],
        "PaymentDueDate",
        document,
    )? {
        return Ok(Some(field));
    }
    date_field(xml, &["Invoice"], "DueDate", document)
}

/// `InvoicePeriod/StartDate` + `EndDate` via [`BillingPeriod::new`]; an inverted or
/// unparsable pair omits the field rather than erroring.
fn period_field(xml: &str, document: usize) -> Result<Option<Field<BillingPeriod>>, Error> {
    let Some(start_el) = find_element(xml, &["Invoice", "InvoicePeriod"], "StartDate") else {
        return Ok(None);
    };
    let Some(end_el) = find_element(xml, &["Invoice", "InvoicePeriod"], "EndDate") else {
        return Ok(None);
    };
    let Some(start) = parse_leading_date(&start_el.text) else {
        return Ok(None);
    };
    let Some(end) = parse_leading_date(&end_el.text) else {
        return Ok(None);
    };
    let Ok(period) = BillingPeriod::new(start, end) else {
        return Ok(None);
    };
    let confidence = Confidence::new(CONFIDENCE)?;
    Ok(Some(Field {
        value: period,
        confidence,
        span: Span {
            source: Source::Document(document),
            start: start_el.start,
            end: end_el.end,
        },
    }))
}

/// Finds `local` under `parents` and parses its leading `YYYY-MM-DD`; a missing element or
/// unparsable date is `None`.
fn date_field(
    xml: &str,
    parents: &[&str],
    local: &str,
    document: usize,
) -> Result<Option<Field<time::Date>>, Error> {
    let Some(element) = find_element(xml, parents, local) else {
        return Ok(None);
    };
    let Some(date) = parse_leading_date(&element.text) else {
        return Ok(None);
    };
    let confidence = Confidence::new(CONFIDENCE)?;
    Ok(Some(Field {
        value: date,
        confidence,
        span: span_of(&element, document),
    }))
}

/// Parses the leading `YYYY-MM-DD` of `text` (anything after is ignored); any other shape,
/// or an invalid calendar date, is `None`.
fn parse_leading_date(text: &str) -> Option<time::Date> {
    let trimmed = text.trim();
    let head = trimmed.get(..10)?;
    if head.as_bytes().get(4) != Some(&b'-') || head.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let year: i32 = head.get(0..4)?.parse().ok()?;
    let month: u8 = head.get(5..7)?.parse().ok()?;
    let day: u8 = head.get(8..10)?.parse().ok()?;
    let month = time::Month::try_from(month).ok()?;
    time::Date::from_calendar_date(year, month, day).ok()
}

/// Builds a [`Field<Money>`] at [`CONFIDENCE`] over `element`'s span.
fn money_field(value: Money, element: &Element, document: usize) -> Result<Field<Money>, Error> {
    let confidence = Confidence::new(CONFIDENCE)?;
    Ok(Field {
        value,
        confidence,
        span: span_of(element, document),
    })
}

/// `element`'s body span, anchored at `document`.
fn span_of(element: &Element, document: usize) -> Span {
    Span {
        source: Source::Document(document),
        start: element.start,
        end: element.end,
    }
}
