# Architecture
<!-- ≤500 words, verify-enforced. Interface-grained crate/module map — the one page every
     agent loads. Decisions go to docs/decisions.md, one line each. -->

## Purpose
(3 bullets max: what this system must do well)

## Crate map
```
┌──────────────┐   ┌──────────────┐
│ server (bin) │   │ app (gpui)   │   thin shells: I/O in, core calls out
└──────┬───────┘   └──────┬───────┘
       └────────┬─────────┘
                ▼
        ┌───────────────┐
        │ core (lib)    │   all domain logic; the only crate specs test
        └───────────────┘
   arrows point one way: binaries → core; core never imports a binary or a spike
```

## Modules in `core`
<!-- One line per module: what it owns + the pub items that are its interface.
     A module is earned when its interface is meaningfully smaller than its implementation
     AND it is testable through that interface alone. Otherwise it is a folder. -->
- `<module>` — owns …; interface: `Type`, `fn …`

## Entry points
- server: `pub fn router(state: AppState) -> axum::Router` (lib) + `main.rs` binds and serves
- app: (none yet)

## Risks / debt
(3 bullets max)
