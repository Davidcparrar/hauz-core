# Spike #63: Topcoat 0.10.0

Evidence: `ref/` (standalone crate; `cargo test` passes).

## 1. Footprint
Features `default-features = false, features = ["router","serve","view","discover"]`. Diffing package names against the workspace `Cargo.lock` gives **45 new packages**. Notable:
- `topcoat`, `-core`, `-router`, `-view`, `-runtime`, `-asset` and their `-macro`/`-grammar` crates;
- `inventory`, `toml`, `toml_edit`, `winnow`, `anymap3`, `lettre`, `aes-gcm`, `hmac`, `hkdf`, `cookie`.

The mail, session and cookie crates still enter the lock with default features off (the lockfile covers optional deps); they are not compiled in. tokio 1.53.1, hyper 1.11.1, tower 0.5.3, http 1.5.0 and tokio-tungstenite unify with the workspace: no duplicates.

## 2. CSS pipeline
`topcoat-tailwind` downloads the standalone Tailwind CLI from GitHub in the user's `build.rs`. That needs network on a cold cache. The `asset!` feature bundles files in a post-build step via the `topcoat` CLI, and rendering an asset missing from the bundle panics. Both are unsuitable for offline `verify.sh`. Plain CSS works with neither: `include_str!("theme.css")` and `include_str!("app.css")` are concatenated in a const and served by a `#[route(GET)]` returning `text/css`. The test fetches `/style` with 200 and finds `--radius`. Tokens live in `crates/web/src/theme.css` (`:root` plus `prefers-color-scheme: dark`) and are embedded at compile time.

## 3. Testing
Yes, with no socket. `Router::handle(Request)` is public. The test builds `Request::get(uri).body(Body::default())`, awaits `router.handle(..)`, and collects the body with `http_body_util` (already a workspace dep). `/admin`, `/admin/bills/42` and `/style` pass (`ref/tests/e2e.rs`). The `tower` feature is not needed.

## 4. Data
`.app_context(store)` on the builder; handlers take `cx: &Cx` and call `app_context::<SqliteStore>(cx).list().await`. The async `open_read_only(&db)` runs in `main` before the router is built, so `--db` works as in the TUI. The test showed the page reading 0 bills from an empty DB.

## 5. Binding
`topcoat::serve(TcpListener::bind(([127,0,0,1], port)).await?, router)`. This avoids `start`, which reads the `HOST` and `PORT` env vars (default 127.0.0.1:3000).

## 6. Stability
- Pin `=0.10.0` (MSRV 1.98, edition 2024).
- 0.x APIs are churning: the 0.10 docs use `module_param!` for dynamic module segments, and a bare `path_param!` compiled but 404ed.
- `module_router!` plus `.discover()` use `inventory` link-time registration. Route modules must be `mod`-declared in the crate; a missing `app_context` registration panics on first request, so cover it in the e2e test.

**Recommendation:** adopt `topcoat =0.10.0` with `default-features = false` and features `router, serve, view, discover`. Use plain embedded CSS (`theme.css` tokens) and test via `Router::handle`. Skip tailwind and asset. Record the 45 new packages in `docs/decisions.md`.

<!-- STATUS: COMPLETE -->
