use hauz_core::store::{BillStore, SqliteStore};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{Router, RouterBuilderDiscoverExt, module_router, page, route, path_param, module_param},
    view::{View, view},
};

const CSS: &str = concat!(include_str!("theme.css"), include_str!("app.css"));

pub fn router(store: SqliteStore) -> Router {
    module_router!().discover().app_context(store).build()
}

pub mod style {
    use super::*;
    #[route(GET)]
    async fn css() -> Result<impl topcoat::router::response::IntoResponse> {
        Ok(([(topcoat::router::header::CONTENT_TYPE, "text/css")], CSS))
    }
}

pub mod admin {
    use super::*;
    #[page]
    async fn list(cx: &Cx) -> Result<impl View> {
        let n = app_context::<SqliteStore>(cx).list().await.map(|b| b.len()).unwrap_or(0);
        let rows = ["1", "2", "3"];
        Ok(view! {
            <!DOCTYPE html>
            <html><head><link rel="stylesheet" href="/style"></head><body>
            <h1>"Bills (" (n) " in db)"</h1>
            <table>for r in rows { <tr><td><a href=(format!("/admin/bills/{r}"))>"bill " (r)</a></td></tr> }</table>
            </body></html>
        })
    }
    pub mod bills {
        pub mod id {
            use super::super::*;
            module_param!(id);
            #[page]
            async fn detail(cx: &Cx) -> Result<impl View> {
                let id = path_param::<Id>(cx);
                Ok(view! { <h1>"Bill " (id)</h1> })
            }
        }
    }
}
