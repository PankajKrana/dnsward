//! Procedural cosmetic filters through a real daemon: what the browser
//! extension receives for a page.

use fuck_ads_core::protocol::{Request, Response};
use fuck_ads_filter::CosmeticResult;
use fuck_ads_tests::*;

async fn cosmetic(env: &TestEnv, host: &str, complex: bool) -> CosmeticResult {
    match env
        .control(Request::Cosmetic {
            hostname: host.into(),
            classes: vec![],
            ids: vec![],
            complex,
        })
        .await
    {
        Response::Cosmetic(r) => r,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn env_with_rules() -> TestEnv {
    let mut env = TestEnv::new().await;
    env.config.custom.rules = vec![
        "shop.example##.card:has-text(Sponsored)".into(),
        "shop.example##.row:has-text(/promo/i):upward(2)".into(),
        "shop.example##.plain-ad".into(),
        // Not supported: skipped, and must not break anything else.
        "shop.example##div:style(color: red)".into(),
        "##.everywhere:has-text(x)".into(),
    ];
    env
}

#[tokio::test]
async fn first_request_carries_the_sites_procedural_filters() {
    let env = env_with_rules().await;
    let d = env.start().await;

    let r = cosmetic(&env, "shop.example", true).await;
    assert_eq!(r.procedural.len(), 2);
    assert_eq!(r.procedural[0].base, ".card");
    // Plain CSS still goes the old way.
    assert_eq!(r.selectors, vec![".plain-ad".to_string()]);

    // The JSON the extension reads.
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["procedural"][0]["steps"][0]["op"], "has-text");
    assert_eq!(
        json["procedural"][0]["steps"][0]["text"]["pattern"],
        "Sponsored"
    );
    assert_eq!(json["procedural"][1]["steps"][1]["op"], "upward");
    assert_eq!(json["procedural"][1]["steps"][1]["levels"], 2);
    d.shutdown().await;
}

#[tokio::test]
async fn later_requests_other_sites_and_allowlisted_sites_get_none() {
    let env = env_with_rules().await;
    let d = env.start().await;

    // Token requests (not the once-per-frame one) carry no procedural filters.
    assert!(
        cosmetic(&env, "shop.example", false)
            .await
            .procedural
            .is_empty()
    );
    // Other sites, including one that merely looks similar.
    assert!(
        cosmetic(&env, "other.example", true)
            .await
            .procedural
            .is_empty()
    );
    assert!(
        cosmetic(&env, "notshop.example", true)
            .await
            .procedural
            .is_empty()
    );
    // Subdomains of the site do get them.
    assert_eq!(
        cosmetic(&env, "www.shop.example", true)
            .await
            .procedural
            .len(),
        2
    );

    // An allowlisted site is left alone entirely.
    let resp = env
        .control(Request::Allow {
            domain: "shop.example".into(),
        })
        .await;
    assert!(matches!(resp, Response::Ok { .. }), "{resp:?}");
    let r = cosmetic(&env, "shop.example", true).await;
    assert!(r.procedural.is_empty() && r.selectors.is_empty());
    d.shutdown().await;
}

#[tokio::test]
async fn pausing_filtering_sends_nothing() {
    let env = env_with_rules().await;
    let d = env.start().await;
    d.daemon.filter.set_enabled(false);
    let r = cosmetic(&env, "shop.example", true).await;
    assert!(r.procedural.is_empty() && r.selectors.is_empty());
    d.shutdown().await;
}
