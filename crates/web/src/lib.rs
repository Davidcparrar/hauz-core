//! Read-only web admin bill browser: server-rendered HTML on Topcoat.

mod admin;
mod format;
mod style;

use hauz_core::store::SqliteStore;
use topcoat::Result;
use topcoat::router::{Router, RouterBuilderDiscoverExt, Slot, layout, module_router};
use topcoat::view::{View, view};

/// Builds the router serving every page from `store`.
#[must_use]
pub fn router(store: SqliteStore) -> Router {
    module_router!().discover().app_context(store).build()
}

/// The shared page chrome: stylesheet link and navigation. At the crate root, so it wraps
/// every page.
#[layout]
async fn shell(slot: Slot<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html>
            <head>
                <meta charset="utf-8">
                <title>"hauz"</title>
                <link rel="stylesheet" href="/style.css">
            </head>
            <body>
                <header>
                    <a href="/admin">"All bills"</a>
                    <a href="/admin/needs-review">"Needs review"</a>
                </header>
                <main>(slot)</main>
            </body>
        </html>
    })
}
