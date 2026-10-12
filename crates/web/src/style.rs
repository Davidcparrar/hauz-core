//! `GET /style.css`: the embedded theme tokens followed by the app rules.

use topcoat::Result;
use topcoat::router::{header, response::IntoResponse, route};

const CSS: &str = concat!(include_str!("theme.css"), include_str!("app.css"));

#[route(GET "/style.css")]
async fn css() -> Result<impl IntoResponse> {
    Ok(([(header::CONTENT_TYPE, "text/css")], CSS))
}
