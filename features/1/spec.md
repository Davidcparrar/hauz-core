# Spec: Domain types: Bill, Money, Vendor, BillingPeriod, Status (#1)
<!-- ≤800 words, verify-enforced. Drafted by the Leader WITH the human; approved at Gate 1. -->

## Problem
`hauz-core` has no domain vocabulary; only the scaffold `Name`/`greet`. This feature adds
the `bill` module: the types every later issue (`email`, `extract`, `store`, `ingest`)
speaks. Every type is constructed only through a fallible constructor, so an invalid
value is unrepresentable in the rest of the crate, including after deserialization.

## Non-goals
- No I/O, persistence, hashing, or extraction logic; no `impl BillStore`.
- No validation of currency codes against the ISO-4217 list (shape only: 3 ASCII uppercase).
- No arithmetic beyond `checked_add`/`checked_sub`; no FX, rounding, or formatting.
- No `issued_at`, `received_at`, line items, tax, or confidence fields (later issues).
- No new dependencies: only `thiserror`, `serde`, `time` (+ dev: `proptest`, `serde_json`),
  all already workspace-pinned.

## Assumptions
- no spike: pure data types; every design choice is already in `docs/decisions.md`.
- [unverified, not load-bearing] `time::Date` serializes as `"YYYY-MM-DD"` with the
  `serde` feature; if not, the implementer adds a `#[serde(with)]` adapter, no spec change.

## Reference implementation
Omitted (no spike).

## Architecture delta
- New `pub mod bill` in `crates/core/src/lib.rs`; delete scaffold `Error`, `Name`, `greet`,
  and `tests/unit_lib.rs`.
- `crates/core/Cargo.toml`: `serde`, `time` as `{ workspace = true }`; `serde_json` dev-dep.
- Public surface of `bill` (all `Debug + Clone + PartialEq + Eq + Serialize + Deserialize`):
  - `Error` (`thiserror`, `#[non_exhaustive]`): `InvalidCurrency`, `CurrencyMismatch`,
    `Overflow`, `EmptyVendor`, `InvalidBillId`, `InvertedPeriod`, `IncompleteBill`.
  - `Currency::new(&str)`; `as_str()`.
  - `Money::new(minor_units: i64, currency: Currency)`; `minor_units()`, `currency()`,
    `checked_add(&self, &Money)`, `checked_sub(&self, &Money)` → `Result<Money, Error>`.
  - `Vendor::new(&str)`; `name()`.
  - `BillId::new(&str)`; `as_str()`.
  - `BillingPeriod::new(start: time::Date, end: time::Date)`; `start()`, `end()`.
  - `Status { Extracted, NeedsReview }`, serde `snake_case`.
  - `BillDraft` (plain struct, pub fields): `id: BillId`, `vendor: Option<Vendor>`,
    `amount: Option<Money>`, `period: Option<BillingPeriod>`, `due: Option<time::Date>`,
    `status: Status`.
  - `Bill`: `TryFrom<BillDraft>`; getters for each field; `#[serde(try_from = "BillDraft",
    into = "BillDraft")]` so JSON can never bypass the constructor.
- Deserialization of every newtype goes through its constructor (`try_from` a raw shape).
- `PROMOTES: bill` → add `Currency` and `BillDraft` to the `bill` line in
  `docs/architecture.md`; one `docs/decisions.md` line for the draft/`TryFrom` pattern.

## Test plan
Files: `crates/core/tests/unit_bill.rs`, `crates/core/tests/property_bill.rs`.
- AC1 [unit] WHEN `Currency::new` receives exactly 3 ASCII uppercase letters THE SYSTEM
  SHALL accept; WHEN it receives lowercase, 2 or 4 letters, digits, whitespace, or empty
  input THE SYSTEM SHALL return `Error::InvalidCurrency`.
- AC2 [unit] WHEN `Money::new` receives any `i64` (negative, zero, `i64::MAX`) THE SYSTEM
  SHALL accept and expose it unchanged via `minor_units()` and `currency()`.
- AC3 [unit] WHEN `checked_add`/`checked_sub` operands differ in currency THE SYSTEM SHALL
  return `Error::CurrencyMismatch`; WHEN the `i64` result overflows THE SYSTEM SHALL return
  `Error::Overflow`; otherwise the result carries the shared currency.
- AC4 [unit] WHEN `Vendor::new` receives a name with surrounding whitespace THE SYSTEM SHALL
  store it trimmed; WHEN the trimmed name is empty THE SYSTEM SHALL return `Error::EmptyVendor`.
- AC5 [unit] WHEN `BillId::new` receives 1–128 bytes of ASCII graphic characters (no
  whitespace) THE SYSTEM SHALL accept; WHEN input is empty, contains whitespace or non-ASCII,
  or exceeds 128 bytes THE SYSTEM SHALL return `Error::InvalidBillId`.
- AC6 [unit] WHEN `BillingPeriod::new(start, end)` has `end < start` THE SYSTEM SHALL return
  `Error::InvertedPeriod`; WHEN `end == start` THE SYSTEM SHALL accept (one-day period).
- AC7 [unit] WHEN a `BillDraft` with `Status::Extracted` lacks `vendor`, `amount`, or
  `period` THE SYSTEM SHALL return `Error::IncompleteBill` from `Bill::try_from`; WHEN all
  three are present THE SYSTEM SHALL accept (`due` may be `None`).
- AC8 [unit] WHEN a `BillDraft` with `Status::NeedsReview` has any subset of optional fields
  (including none) THE SYSTEM SHALL accept.
- AC9 [unit] WHEN JSON violating an invariant is deserialized (currency `"usd"`, inverted
  period, `extracted` bill missing `amount`) THE SYSTEM SHALL fail to deserialize.
- AC10 [unit] WHEN `Status` is serialized THE SYSTEM SHALL emit `"extracted"` /
  `"needs_review"` and read them back.
- AC11 [property] FOR ALL `Money` a, b of one currency where neither step overflows THE
  SYSTEM SHALL satisfy `a.checked_add(b)?.checked_sub(b)? == a`.
- AC12 [property] FOR ALL valid `Bill` THE SYSTEM SHALL satisfy
  `serde_json::from_str(&serde_json::to_string(&bill)?)? == bill`.

<!-- GATE 1 CHECKLIST (human + Leader, before "approved"):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (no cross-module, no entry point ⇒ unit + property only)
     [x] no [unverified] assumption is load-bearing; spike skipped with reason
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens
     [x] ≤800 words (745) -->
