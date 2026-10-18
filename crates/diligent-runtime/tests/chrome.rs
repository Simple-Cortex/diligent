//! Chrome integration tests. They launch a real browser, so they are
//! `#[ignore]`d by default. Run them with:
//!
//! `cargo test -p diligent-runtime --features chrome -- --ignored`
//!
//! Set `DILIGENT_CHROME` to a Chrome binary if it is not in a common location.
#![cfg(feature = "chrome")]

mod support;

use diligent_runtime::browser::Browser;
use diligent_runtime::chrome::{ChromeBrowser, ChromeConfig};
use diligent_runtime::protocol::{ErrorKind, Media};
use std::time::Duration;
use support::server::FixtureServer;

async fn launch(server: &FixtureServer) -> ChromeBrowser {
    let mut config = ChromeConfig::new();
    config.allowed_origins = vec![server.origin()];
    config.timeout = Duration::from_secs(30);
    ChromeBrowser::launch(config).await.expect("launch Chrome")
}

#[tokio::test]
#[ignore = "launches Chrome"]
async fn navigation_info_has_title_and_lang() {
    let server = FixtureServer::start();
    let mut browser = launch(&server).await;
    let info = browser
        .navigate(&server.url("/basic.html"), Duration::from_secs(30))
        .await
        .expect("navigate");
    assert_eq!(info.title, "Diligent runtime fixture: basic");
    assert_eq!(info.lang.as_deref(), Some("en"));
    assert_eq!(info.status, Some(200));
    browser.close().await.expect("close");
}

#[tokio::test]
#[ignore = "launches Chrome"]
async fn ax_tree_contains_the_unlabelled_input_with_empty_name() {
    let server = FixtureServer::start();
    let mut browser = launch(&server).await;
    browser
        .navigate(&server.url("/basic.html"), Duration::from_secs(30))
        .await
        .expect("navigate");
    let dom = browser.dom_snapshot().await.expect("dom");
    let unlabelled = dom
        .nodes
        .iter()
        .find(|n| {
            n.attributes
                .iter()
                .any(|(k, v)| k == "id" && v == "unlabelled")
        })
        .expect("unlabelled input in DOM");
    let backend = unlabelled.backend_node_id.expect("backend id");
    let ax = browser.ax_tree().await.expect("ax");
    let node = ax
        .nodes
        .iter()
        .find(|n| n.backend_dom_node_id == Some(backend))
        .expect("AX node for unlabelled input");
    assert_eq!(node.role.as_deref(), Some("textbox"));
    assert!(
        node.name.as_deref().unwrap_or("").is_empty(),
        "name must be empty"
    );
    browser.close().await.expect("close");
}

#[tokio::test]
#[ignore = "launches Chrome"]
async fn focus_order_matches_the_fixture() {
    let server = FixtureServer::start();
    let mut browser = launch(&server).await;
    browser
        .navigate(&server.url("/focus-order.html"), Duration::from_secs(30))
        .await
        .expect("navigate");
    let stops = browser.focus_order(50).await.expect("focus order").stops;
    let names: Vec<&str> = stops.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Link one",
            "Button two",
            "Text three",
            "Select four",
            "Textarea five",
            "Custom focusable six",
            "Link seven",
            "Checkbox eight",
            "Link ten",
        ]
    );
    browser.close().await.expect("close");
}

#[tokio::test]
#[ignore = "launches Chrome"]
async fn network_log_has_hsts_header_and_redirect_chain() {
    let server = FixtureServer::start();
    let mut browser = launch(&server).await;
    let info = browser
        .navigate(&server.url("/redirect"), Duration::from_secs(30))
        .await
        .expect("navigate");
    assert!(info.final_url.ends_with("/headers.html"));
    assert_eq!(info.redirect_chain, vec![server.url("/redirect")]);
    let log = browser.network_log().await.expect("network log");
    let document = log
        .iter()
        .find(|e| e.resource_type == "Document" && e.url.ends_with("/headers.html"))
        .expect("document entry");
    assert!(document
        .response_headers
        .contains_key("strict-transport-security"));
    assert_eq!(document.redirect_chain, vec![server.url("/redirect")]);
    assert!(!document.third_party);
    browser.close().await.expect("close");
}

#[tokio::test]
#[ignore = "launches Chrome"]
async fn navigation_outside_allowed_origins_is_blocked() {
    let server = FixtureServer::start();
    let mut browser = launch(&server).await;
    let error = browser
        .navigate("http://example.invalid/", Duration::from_secs(30))
        .await
        .expect_err("must be blocked");
    assert_eq!(error.kind, ErrorKind::BlockedOrigin);
    assert!(!error.retryable);
    browser.close().await.expect("close");
}

#[tokio::test]
#[ignore = "launches Chrome"]
async fn tiny_timeout_yields_timeout_not_success() {
    let server = FixtureServer::start();
    let mut browser = launch(&server).await;
    let error = browser
        .navigate(&server.url("/basic.html"), Duration::from_millis(1))
        .await
        .expect_err("1 ms cannot load a page");
    assert_eq!(error.kind, ErrorKind::Timeout);
    assert!(error.retryable);
    browser
        .emulate_media(Media::BASELINE)
        .await
        .expect("browser still usable");
    browser.close().await.expect("close");
}
