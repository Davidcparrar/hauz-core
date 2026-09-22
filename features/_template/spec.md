# Spec: <feature title> (#<issue>)
<!-- ≤800 words, verify-enforced. Drafted by the Leader WITH the human; approved at Gate 1. -->

## Problem
(one paragraph: what changes for the user)

## Non-goals
- (the scope creep this feature invites, excluded explicitly)

## Assumptions
- [spike-verified] …
- [unverified] … <!-- no unverified assumption may be load-bearing for the test plan -->
<!-- no spike: <reason> — when the Leader skipped the spike, say why here -->

## Reference implementation
<!-- ≤150 lines, excerpt or pointer into features/<n>/spike/ref/.
     Illustrative — reimplement under constitution rules. Omit if no spike ran. -->

## Architecture delta
(module-level changes in `core`; new crate or entry point; dependency additions.
`PROMOTES: <module>` iff a pub interface or crate boundary changes → update
`docs/architecture.md` and add a `docs/decisions.md` line in the same PR)

## Test plan
<!-- EARS form, interface-level only. Each criterion = one test fn `acN_<behavior>` written
     first, in the file for its level (constitution › Test levels).
     [integration] required when crossing modules; [e2e] required when touching the entry point. -->
- AC1 [unit] WHEN <condition> THE SYSTEM SHALL <behavior>
- AC2 [integration] WHEN <condition> THE SYSTEM SHALL <behavior>
- AC3 [e2e] WHEN <invalid input at entry point> THE SYSTEM SHALL <failure behavior>
- AC4 [property] FOR ALL <input> THE SYSTEM SHALL <invariant / round-trip law>

<!-- GATE 1 CHECKLIST (human + Leader, before "approved"):
     [ ] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [ ] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [ ] no [unverified] assumption is load-bearing; spike questions answered or carried
     [ ] non-goals actually exclude the creep this feature invites
     [ ] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [ ] fits an implementer context of ~15k tokens (else split into two issues)
     [ ] ≤800 words -->
