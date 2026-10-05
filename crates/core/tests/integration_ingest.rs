//! [integration] tests for the `ingest` module's public API: the real `TextExtractor` and a
//! real `SqliteStore` on a unique tmp-file database. One file per level per module. Test fn
//! names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::{Result, TmpDbFile};
use hauz_core::bill::{BillingPeriod, Currency, Money, Status, Vendor};
use hauz_core::email::Envelope;
use hauz_core::extract::{
    Chain, Confidence, Error as ExtractError, Escalate, Extraction, Extractor, Field,
    PdfTextExtractor, Source, Span, TextExtractor, XmlInvoiceExtractor,
};
use hauz_core::ingest::{EXTRACTED_MIN_CONFIDENCE, Error, Outcome, ingest, raw_hash};
use hauz_core::llm::{
    Error as LlmError, LlmClient, LlmExtractor, LlmOptions, LlmRequest, Rasterizer,
};
use hauz_core::store::{BillStore, SqliteStore};
use time::macros::date;

const PLAIN: &[u8] = include_bytes!("fixtures/plain.eml");
const MALFORMED: &[u8] = include_bytes!("fixtures/malformed.eml");
const CO_BARE_DOLLAR: &[u8] = include_bytes!("fixtures/co_bare_dollar.eml");
const DIAN_FULL_EML: &[u8] = include_bytes!("fixtures/ubl/dian_full.eml");
const DIAN_NO_PERIOD_EML: &[u8] = include_bytes!("fixtures/ubl/dian_no_period.eml");

/// Lowercase hex encoding, test-local (the crate's own encoder is private).
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// AC1: `bill_eml()` ingested into an empty store returns `Created(id)` whose id is the
/// lowercase hex of `raw_hash(raw)`, stored as `NeedsReview` with amount, due, and vendor set,
/// no period.
#[tokio::test]
async fn ac1_bill_eml_into_empty_store_is_created_needs_review() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac1");
    let store = SqliteStore::open(&db.path).await?;
    let raw = common::bill_eml();

    let outcome = ingest(&raw, &TextExtractor, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    assert_eq!(id.as_str(), to_hex(raw_hash(&raw).as_bytes()).as_str());

    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(
        bill.amount(),
        Some(&Money::new(123_456, Currency::new("EUR")?))
    );
    assert_eq!(bill.due(), Some(date!(2026 - 10 - 15)));
    assert_eq!(bill.vendor(), Some(&Vendor::new("acme-power.example")?));
    assert_eq!(bill.period(), None);
    Ok(())
}

/// AC2: the same bytes ingested again return `Duplicate(id)` with AC1's id, and `list` still
/// has length 1.
#[tokio::test]
async fn ac2_same_bytes_twice_is_duplicate_and_list_len_one() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac2");
    let store = SqliteStore::open(&db.path).await?;
    let raw = common::bill_eml();

    let first = ingest(&raw, &TextExtractor, &store).await?;
    let Outcome::Created(id) = first else {
        return Err(format!("expected Created, got {first:?}").into());
    };

    let second = ingest(&raw, &TextExtractor, &store).await?;
    assert_eq!(second, Outcome::Duplicate(id));
    assert_eq!(store.list().await?.len(), 1);
    Ok(())
}

/// AC3: a message with no amount and no date is `Created(_)`, stored `NeedsReview` with
/// amount and due both `None`.
#[tokio::test]
async fn ac3_no_amount_no_date_is_created_needs_review_with_nones() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac3");
    let store = SqliteStore::open(&db.path).await?;

    let outcome = ingest(PLAIN, &TextExtractor, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(bill.amount(), None);
    assert_eq!(bill.due(), None);
    Ok(())
}

/// AC4: a malformed message returns `Err(Error::Email(_))` and stores nothing, on the first
/// call and again on a second.
#[tokio::test]
async fn ac4_malformed_message_is_email_error_and_list_stays_empty() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac4");
    let store = SqliteStore::open(&db.path).await?;

    let first = ingest(MALFORMED, &TextExtractor, &store).await;
    assert!(matches!(first, Err(Error::Email(_))));
    assert_eq!(store.list().await?.len(), 0);

    let second = ingest(MALFORMED, &TextExtractor, &store).await;
    assert!(matches!(second, Err(Error::Email(_))));
    assert_eq!(store.list().await?.len(), 0);
    Ok(())
}

/// Always returns a clone of the fixed extraction it was built with, ignoring the envelope.
struct Fixed(Extraction);

impl Extractor for Fixed {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, ExtractError>> {
        let extraction = self.0.clone();
        Box::pin(async move { Ok(extraction) })
    }
}

/// An extraction with only `period` set.
fn period_only() -> Result<Extraction> {
    Ok(Extraction {
        period: Some(Field {
            value: BillingPeriod::new(date!(2026 - 01 - 01), date!(2026 - 01 - 31))?,
            confidence: Confidence::new(90)?,
            span: Span {
                source: Source::Text,
                start: 0,
                end: 0,
            },
        }),
        ..Extraction::default()
    })
}

/// AC5: `bill_eml()` (amount, due and vendor, no period) ingested via `Escalate(TextExtractor,
/// Fixed(period only), EXTRACTED_MIN_CONFIDENCE)` over a `SqliteStore` is stored `Extracted`
/// with that period; `TextExtractor` alone on the same bytes is stored `NeedsReview` (AC1).
#[tokio::test]
async fn ac5_escalate_fills_missing_period_text_extractor_alone_needs_review() -> Result<()> {
    let raw = common::bill_eml();
    let period = period_only()?;
    let expected_period = period.period.clone().map(|field| field.value);

    let db = TmpDbFile::new("ingest-ac5-escalate");
    let store = SqliteStore::open(&db.path).await?;
    let escalate = Escalate::new(
        Box::new(TextExtractor),
        Box::new(Fixed(period)),
        EXTRACTED_MIN_CONFIDENCE,
    );

    let outcome = ingest(&raw, &escalate, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(bill.period(), expected_period.as_ref());

    let plain_db = TmpDbFile::new("ingest-ac5-plain");
    let plain_store = SqliteStore::open(&plain_db.path).await?;
    let plain_outcome = ingest(&raw, &TextExtractor, &plain_store).await?;
    let Outcome::Created(plain_id) = plain_outcome else {
        return Err(format!("expected Created, got {plain_outcome:?}").into());
    };
    let plain_bill = plain_store.get(&plain_id).await?.ok_or("missing bill")?;
    assert_eq!(plain_bill.status(), Status::NeedsReview);
    Ok(())
}

/// A full, schema-conformant reply at confidence 100 — the spec's "full reply" fixture.
const FULL_REPLY: &str = "```json\n{\"vendor\":\"Acme Power\",\"amount_minor_units\":999,\
\"currency\":\"USD\",\"period_start\":\"2026-09-01\",\"period_end\":\"2026-09-30\",\
\"issued\":\"2026-10-01\",\"due\":\"2026-10-15\",\"confidence\":100}\n```";

/// Always replies with the fixed "full reply" fixture, ignoring the request.
struct FullReplyClient;

impl LlmClient for FullReplyClient {
    fn complete<'a>(
        &'a self,
        _req: &'a LlmRequest,
    ) -> hauz_core::BoxFuture<'a, core::result::Result<String, LlmError>> {
        Box::pin(async move { Ok(FULL_REPLY.to_owned()) })
    }
}

/// `bill_eml()` has no attachments, so this is never called; it errors loudly if it ever is.
struct UnusedRasterizer;

impl Rasterizer for UnusedRasterizer {
    fn rasterize(
        &self,
        _pdf: &[u8],
        _max_pages: u8,
    ) -> core::result::Result<Vec<Vec<u8>>, LlmError> {
        Err(LlmError::Rasterizer {
            reason: "bill_eml() has no PDF attachments; the rasterizer must not run".to_owned(),
        })
    }
}

/// AC6: `bill_eml()` (amount/due at heuristic confidence 90, sender-domain vendor at 20, no
/// period) ingested via `Escalate(Chain([TextExtractor, PdfTextExtractor]),
/// LlmExtractor(full-reply fake), 50)` is stored `Extracted`: the heuristic's higher-confidence
/// amount (123 456 EUR, 90 beats 70) and due survive, while the model's higher-confidence
/// vendor (`Acme Power`, 70 beats 20) and its only period (2026-09-01..30) fill the rest.
#[tokio::test]
async fn ac6_escalate_with_llm_extractor_fills_vendor_and_period() -> Result<()> {
    let raw = common::bill_eml();
    let db = TmpDbFile::new("ingest-ac6-llm-escalate");
    let store = SqliteStore::open(&db.path).await?;

    let primary = Chain::new(vec![Box::new(TextExtractor), Box::new(PdfTextExtractor)]);
    let secondary = LlmExtractor::new(
        Box::new(FullReplyClient),
        Box::new(UnusedRasterizer),
        LlmOptions::default(),
    );
    let escalate = Escalate::new(Box::new(primary), Box::new(secondary), 50);

    let outcome = ingest(&raw, &escalate, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;

    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(bill.vendor(), Some(&Vendor::new("Acme Power")?));
    assert_eq!(
        bill.period(),
        Some(&BillingPeriod::new(
            date!(2026 - 09 - 01),
            date!(2026 - 09 - 30)
        )?)
    );
    assert_eq!(bill.due(), Some(date!(2026 - 10 - 15)));
    assert_eq!(
        bill.amount(),
        Some(&Money::new(123_456, Currency::new("EUR")?))
    );
    Ok(())
}

/// AC4: a bare-`$` bill (no other currency marker) ingested via `Chain([TextExtractor,
/// PdfTextExtractor])` is stored `NeedsReview` with amount `None`, the due date and the
/// sender-domain vendor still set.
#[tokio::test]
async fn ac4_bare_dollar_bill_is_needs_review_with_no_amount() -> Result<()> {
    let db = TmpDbFile::new("ingest-28-ac4");
    let store = SqliteStore::open(&db.path).await?;
    let chain = Chain::new(vec![Box::new(TextExtractor), Box::new(PdfTextExtractor)]);

    let outcome = ingest(CO_BARE_DOLLAR, &chain, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };

    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(bill.amount(), None);
    assert_eq!(bill.due(), Some(date!(2026 - 10 - 15)));
    assert_eq!(bill.vendor(), Some(&Vendor::new("acme-energia.example")?));
    Ok(())
}

/// A fake [`LlmClient`] that always replies with a fixed, full, schema-conformant extraction
/// for the bare-dollar bill: vendor `Acme Energía`, amount 123 456 700 `COP`, period
/// 2026-09-01..30, due 2026-10-15, confidence 100.
struct BareDollarLlmClient;

impl LlmClient for BareDollarLlmClient {
    fn complete<'a>(
        &'a self,
        _req: &'a LlmRequest,
    ) -> hauz_core::BoxFuture<'a, core::result::Result<String, LlmError>> {
        let reply = "```json\n{\"vendor\":\"Acme Energía\",\"amount_minor_units\":123456700,\
\"currency\":\"COP\",\"period_start\":\"2026-09-01\",\"period_end\":\"2026-09-30\",\
\"issued\":null,\"due\":\"2026-10-15\",\"confidence\":100}\n```"
            .to_owned();
        Box::pin(async move { Ok(reply) })
    }
}

/// AC5: the same bare-`$` bill, now escalated to the model when the heuristic chain is
/// incomplete, is stored `Extracted` with the model's amount (no heuristic amount outranks
/// it, since a bare `$` is no longer read as `USD`).
#[tokio::test]
async fn ac5_bare_dollar_bill_escalates_to_model_amount() -> Result<()> {
    let db = TmpDbFile::new("ingest-28-ac5");
    let store = SqliteStore::open(&db.path).await?;

    let primary = Chain::new(vec![Box::new(TextExtractor), Box::new(PdfTextExtractor)]);
    let secondary = LlmExtractor::new(
        Box::new(BareDollarLlmClient),
        Box::new(UnusedRasterizer),
        LlmOptions::default(),
    );
    let escalate = Escalate::new(Box::new(primary), Box::new(secondary), 50);

    let outcome = ingest(CO_BARE_DOLLAR, &escalate, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };

    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(
        bill.amount(),
        Some(&Money::new(123_456_700, Currency::new("COP")?))
    );
    Ok(())
}

/// AC7 (#27): `dian_full.eml` ingested through `Chain([XmlInvoiceExtractor, TextExtractor,
/// PdfTextExtractor])` is stored `Extracted` with the AC1 vendor, amount, period and due
/// (the no-period eml is covered by `ac8_dian_no_period_chain_is_extracted_with_issued`).
#[tokio::test]
async fn ac7_dian_zip_chain_extracted() -> Result<()> {
    let chain = Chain::new(vec![
        Box::new(XmlInvoiceExtractor),
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]);

    let db = TmpDbFile::new("ingest-27-ac7-full");
    let store = SqliteStore::open(&db.path).await?;
    let outcome = ingest(DIAN_FULL_EML, &chain, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(
        bill.vendor(),
        Some(&Vendor::new("Acme & Luz S.A.S. E.S.P.")?)
    );
    assert_eq!(
        bill.amount(),
        Some(&Money::new(18_435_000, Currency::new("COP")?))
    );
    assert_eq!(
        bill.period(),
        Some(&BillingPeriod::new(
            date!(2026 - 08 - 01),
            date!(2026 - 08 - 31)
        )?)
    );
    assert_eq!(bill.due(), Some(date!(2026 - 09 - 25)));
    Ok(())
}

/// AC8 (#37): `dian_no_period.eml` ingested through the #27 chain is stored `Extracted` with
/// issued 2026-09-10 and no period.
#[tokio::test]
async fn ac8_dian_no_period_chain_is_extracted_with_issued() -> Result<()> {
    let chain = Chain::new(vec![
        Box::new(XmlInvoiceExtractor),
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]);
    let db = TmpDbFile::new("ingest-37-ac8");
    let store = SqliteStore::open(&db.path).await?;
    let outcome = ingest(DIAN_NO_PERIOD_EML, &chain, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(
        bill.vendor(),
        Some(&Vendor::new("Gas Natural Ejemplo S.A.")?)
    );
    assert_eq!(
        bill.amount(),
        Some(&Money::new(9_950, Currency::new("USD")?))
    );
    assert_eq!(bill.due(), Some(date!(2026 - 09 - 30)));
    assert_eq!(bill.period(), None);
    assert_eq!(bill.issued(), Some(date!(2026 - 09 - 10)));
    Ok(())
}
