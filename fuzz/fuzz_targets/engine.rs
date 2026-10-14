#![no_main]
//! Building an engine from arbitrary lists and querying it must not panic,
//! and exported browser rules must always serialize.
use fuck_ads_filter::export::{UserOverrides, dnr_rules, safari_content_blocker};
use fuck_ads_filter::{Engine, Request, ResourceType};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let engine = Engine::from_rules(text);
    let _ = engine.check_hostname("a.b.example.com");
    if let Some(req) = Request::new("https://a.b.example.com/x/ad.js?q=1", Some("https://site.example/"), ResourceType::Script) {
        let _ = engine.check_request(&req);
    }
    let _ = engine.cosmetic_filters("a.b.example.com", &["ad".into()], &["banner".into()], true);
    let _ = engine.scriptlets_for("www.youtube.com");
    let _ = serde_json::to_string(&dnr_rules(&engine, &UserOverrides::default(), 1000));
    let _ = serde_json::to_string(&safari_content_blocker(&engine, &UserOverrides::default(), 1000));
});
