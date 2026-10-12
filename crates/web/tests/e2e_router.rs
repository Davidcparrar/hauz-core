//! [e2e] acceptance tests for the `hauz-web` router (feature #63), in-process.

mod common;

use common::{Result, extracted, get, needs_review, router_with, scripted};

#[tokio::test]
async fn ac1_list_shows_both_bills_newest_first_with_links() -> Result<()> {
    let router = router_with(&[extracted()?, needs_review()?]).await?;
    let (status, _, body) = get(&router, "/admin").await?;
    assert_eq!(status, 200, "{body}");
    for expected in [
        "aaaaaaaa",
        "Acme Power",
        "1234.56 COP",
        "2026-01-05",
        "2026-02-15",
        "extracted",
        "bbbbbbbb",
        "Beta Water",
        "needs_review",
        "/admin/bills/aaaaaaaa-old-bill",
        "/admin/bills/bbbbbbbb-new-bill",
    ] {
        assert!(body.contains(expected), "missing {expected}: {body}");
    }
    assert!(body.find("Beta Water") < body.find("Acme Power"), "{body}");
    Ok(())
}

#[tokio::test]
async fn ac2_needs_review_path_lists_only_needs_review() -> Result<()> {
    let router = router_with(&[extracted()?, needs_review()?]).await?;
    let (status, _, body) = get(&router, "/admin/needs-review").await?;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Beta Water"), "{body}");
    assert!(!body.contains("Acme Power"), "{body}");
    Ok(())
}

#[tokio::test]
async fn ac3_detail_shows_every_field_with_dash_for_absent() -> Result<()> {
    let router = router_with(&[extracted()?, needs_review()?]).await?;
    let (status, _, body) = get(&router, "/admin/bills/aaaaaaaa-old-bill").await?;
    assert_eq!(status, 200, "{body}");
    for expected in [
        "aaaaaaaa-old-bill",
        "Acme Power",
        "1234.56 COP",
        "2026-01-01 to 2026-01-31",
        "2026-01-05",
        "2026-02-15",
        "extracted",
    ] {
        assert!(body.contains(expected), "missing {expected}: {body}");
    }
    let (status, _, body) = get(&router, "/admin/bills/bbbbbbbb-new-bill").await?;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(">-<"), "{body}");
    Ok(())
}

#[tokio::test]
async fn ac4_unknown_or_malformed_id_is_404() -> Result<()> {
    let router = router_with(&[extracted()?]).await?;
    for uri in [
        "/admin/bills/nope",
        "/admin/bills/with%20space",
        "/admin/bills/%C3%A9",
    ] {
        let (status, _, body) = get(&router, uri).await?;
        assert_eq!(status, 404, "{uri}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn ac5_vendor_markup_is_escaped_in_list_and_detail() -> Result<()> {
    let router = router_with(&[scripted()?]).await?;
    for uri in ["/admin", "/admin/bills/cccccccc-xss-bill"] {
        let (status, _, body) = get(&router, uri).await?;
        assert_eq!(status, 200, "{uri}: {body}");
        assert!(!body.contains("<script>"), "{uri}: {body}");
        assert!(body.contains("&lt;script&gt;"), "{uri}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn ac6_style_css_serves_tokens_and_pages_link_it() -> Result<()> {
    let router = router_with(&[extracted()?]).await?;
    let (status, content_type, css) = get(&router, "/style.css").await?;
    assert_eq!(status, 200);
    assert!(content_type.starts_with("text/css"), "{content_type}");
    for token in [
        ":root",
        "--color-bg",
        "--color-fg",
        "--color-accent",
        "--font-body",
        "--radius",
        "prefers-color-scheme: dark",
    ] {
        assert!(css.contains(token), "missing {token}");
    }
    for uri in [
        "/admin",
        "/admin/needs-review",
        "/admin/bills/aaaaaaaa-old-bill",
    ] {
        let (_, _, body) = get(&router, uri).await?;
        assert!(body.contains("/style.css"), "{uri}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn ac7_empty_database_renders_no_bills_message() -> Result<()> {
    let router = router_with(&[]).await?;
    let (status, _, body) = get(&router, "/admin").await?;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("no bills"), "{body}");
    Ok(())
}
