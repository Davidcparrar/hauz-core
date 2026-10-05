# Review log (#39)

## Cycle 1
VERDICT: APPROVE
- verify.sh ALL GREEN (rerun by reviewer); AC1–AC9 present at tagged level/file and asserting the criterion (AC9 per the amended generator: no `"`/`\`).
- `sender_name` from the same `Addr` as `sender` (From, else Sender), trimmed, `None` when missing/blank/equal to the address ignoring ASCII case.
- `TextExtractor` vendor = display name, else lowercased domain; confidence 20, span `Text 0..0` on both paths; an invalid-`Vendor` name falls back to the domain.
- No unwrap/expect/panic in core, no dependency change; test literals only gain `sender_name: None`; non-goals hold.
