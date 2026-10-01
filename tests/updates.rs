//! Filter-list updates: validation, atomic install, rollback, conditional
//! requests and backoff.

use adblock_core::rules::LiveFilter;
use adblock_core::rules::UserRules;
use adblock_core::update::{FetchResponse, Updater};
use adblock_filter::{Category, Engine, HostVerdict};
use adblock_tests::*;
use std::sync::Arc;
use std::time::Duration;

fn hosts(prefix: &str, n: usize) -> String {
    (0..n)
        .map(|i| format!("||{prefix}{i}.example^\n"))
        .collect()
}

struct Setup {
    env: TestEnv,
    updater: Updater,
    filter: LiveFilter,
}

async fn setup(lists: &[(&str, usize)]) -> Setup {
    let mut env = TestEnv::new().await;
    env.config.filters.lists = lists
        .iter()
        .map(|(n, min)| remote_list(n, Category::Ads, *min))
        .collect();
    std::fs::create_dir_all(env.paths.lists_dir()).unwrap();
    let updater = Updater::new(env.paths.clone(), env.fetcher.clone())
        .with_retry_base(Duration::from_millis(1));
    let filter = LiveFilter::new(Engine::default(), &UserRules::default(), &env.config);
    Setup {
        env,
        updater,
        filter,
    }
}

fn url(name: &str) -> String {
    format!("https://lists.test/{name}.txt")
}

fn blocked(f: &LiveFilter, host: &str) -> bool {
    matches!(f.engine().check_hostname(host), HostVerdict::Blocked { .. })
}

#[tokio::test]
async fn successful_update_activates_rules() {
    let s = setup(&[("a", 2)]).await;
    s.env
        .fetcher
        .set_body(&url("a"), &hosts("a", 50), Some("\"v1\""));
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert_eq!(out.changed, vec!["a"]);
    assert!(out.activated && out.failed.is_empty(), "{out:?}");
    assert!(blocked(&s.filter, "a7.example"));
    assert!(s.env.paths.lists_dir().join("a.txt").is_file());
    let state = s.updater.state();
    assert!(state.last_success.is_some());
    assert_eq!(state.lists["a"].etag.as_deref(), Some("\"v1\""));
    assert_eq!(state.lists["a"].rules, 50);
}

#[tokio::test]
async fn corrupted_update_keeps_previous_rules() {
    let s = setup(&[("a", 2)]).await;
    s.env.fetcher.set_body(&url("a"), &hosts("a", 50), None);
    s.updater.run(&s.env.config, &s.filter, false).await;
    let installed = std::fs::read(s.env.paths.lists_dir().join("a.txt")).unwrap();

    for bad in [
        b"\x00\x01\x02binary garbage".to_vec(),
        vec![0xff, 0xfe, 0xfd],
        b"<html>Error 500</html>".to_vec(),
    ] {
        s.env.fetcher.set_bytes(&url("a"), bad);
        let out = s.updater.run(&s.env.config, &s.filter, true).await;
        assert_eq!(out.failed.len(), 1, "{out:?}");
        assert!(!out.activated);
        assert!(
            blocked(&s.filter, "a7.example"),
            "previous rules stay active"
        );
        assert_eq!(
            std::fs::read(s.env.paths.lists_dir().join("a.txt")).unwrap(),
            installed,
            "installed file untouched"
        );
        assert!(
            !s.env.paths.lists_dir().join("a.txt.new").exists(),
            "staging file cleaned up"
        );
    }
    assert!(s.updater.state().consecutive_failures >= 1);
}

#[tokio::test]
async fn truncated_update_is_rejected_unless_forced() {
    let s = setup(&[("a", 2)]).await;
    s.env.fetcher.set_body(&url("a"), &hosts("a", 3000), None);
    assert!(
        s.updater
            .run(&s.env.config, &s.filter, false)
            .await
            .activated
    );
    s.env.fetcher.set_body(&url("a"), &hosts("b", 100), None);
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert!(out.failed[0].1.contains("dropped"), "{out:?}");
    assert!(blocked(&s.filter, "a2999.example"));
    let out = s.updater.run(&s.env.config, &s.filter, true).await;
    assert!(out.activated, "{out:?}");
    assert!(blocked(&s.filter, "b1.example") && !blocked(&s.filter, "a2999.example"));
}

#[tokio::test]
async fn sha256_pin_is_enforced() {
    let mut s = setup(&[("a", 2)]).await;
    s.env.config.filters.lists[0].sha256 = Some("0".repeat(64));
    s.env.fetcher.set_body(&url("a"), &hosts("a", 50), None);
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert!(out.failed[0].1.contains("SHA-256"), "{out:?}");
    assert!(!blocked(&s.filter, "a1.example"));

    let body = hosts("a", 50);
    use sha2::Digest;
    let digest: String = sha2::Sha256::digest(body.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    s.env.config.filters.lists[0].sha256 = Some(digest);
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert!(out.activated, "{out:?}");
}

#[tokio::test]
async fn too_few_rules_is_rejected() {
    let s = setup(&[("a", 10)]).await;
    s.env
        .fetcher
        .set_body(&url("a"), "! only comments\n||one.example^\n", None);
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert!(out.failed[0].1.contains("minimum"), "{out:?}");
}

#[tokio::test]
async fn conditional_requests_and_not_modified() {
    let s = setup(&[("a", 2)]).await;
    s.env
        .fetcher
        .set_body(&url("a"), &hosts("a", 20), Some("\"etag-1\""));
    s.updater.run(&s.env.config, &s.filter, false).await;
    s.env.fetcher.set(&url("a"), Ok(FetchResponse::NotModified));
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert_eq!(out.unchanged, vec!["a"]);
    assert!(out.failed.is_empty());
    let requests = s.env.fetcher.requests.lock().unwrap().clone();
    assert_eq!(
        requests.last().unwrap().1.as_deref(),
        Some("\"etag-1\""),
        "If-None-Match sent"
    );
}

#[tokio::test]
async fn engine_level_rollback() {
    // Two lists just under the per-list shrink guard; together they collapse.
    let s = setup(&[("a", 2), ("b", 2)]).await;
    s.env.fetcher.set_body(&url("a"), &hosts("a", 900), None);
    s.env.fetcher.set_body(&url("b"), &hosts("b", 900), None);
    assert!(
        s.updater
            .run(&s.env.config, &s.filter, false)
            .await
            .activated
    );

    s.env.fetcher.set_body(&url("a"), &hosts("c", 100), None);
    s.env.fetcher.set_body(&url("b"), &hosts("d", 100), None);
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert!(out.rolled_back && !out.activated, "{out:?}");
    assert!(blocked(&s.filter, "a899.example") && blocked(&s.filter, "b899.example"));
    // Files on disk are back to the working version.
    let a = std::fs::read_to_string(s.env.paths.lists_dir().join("a.txt")).unwrap();
    assert!(a.contains("a899.example"));
}

#[tokio::test]
async fn download_failures_back_off_exponentially() {
    let s = setup(&[("a", 2)]).await;
    s.env
        .fetcher
        .set(&url("a"), Err("connection refused".into()));
    let out = s.updater.run(&s.env.config, &s.filter, false).await;
    assert_eq!(out.failed.len(), 1);
    let first = s.updater.next_delay(&s.env.config);
    s.updater.run(&s.env.config, &s.filter, false).await;
    let second = s.updater.next_delay(&s.env.config);
    assert!(
        first <= Duration::from_secs(120) && second > first,
        "{first:?} {second:?}"
    );
    // Retries within a run happened (3 attempts per run).
    assert_eq!(s.env.fetcher.requests.lock().unwrap().len(), 6);

    s.env.fetcher.set_body(&url("a"), &hosts("a", 20), None);
    s.updater.run(&s.env.config, &s.filter, false).await;
    assert_eq!(s.updater.state().consecutive_failures, 0);
    assert!(s.updater.next_delay(&s.env.config) > Duration::from_secs(11 * 3600));
}

#[tokio::test]
async fn update_through_daemon_and_survives_restart() {
    let mut env = TestEnv::new().await;
    env.config
        .filters
        .lists
        .push(remote_list("remote", Category::Trackers, 2));
    env.fetcher.set_body(
        "https://lists.test/remote.txt",
        "||remote-tracker.example^\n||r2.example^\n",
        None,
    );
    let d = env.start().await;
    let r = env
        .control(adblock_core::protocol::Request::Update { force: false })
        .await;
    assert!(
        matches!(r, adblock_core::protocol::Response::Ok { .. }),
        "{r:?}"
    );
    assert!(
        d.daemon
            .filter
            .engine()
            .is_host_blocked("remote-tracker.example")
    );
    d.shutdown().await;

    let d = env.start().await;
    assert!(
        d.daemon
            .filter
            .engine()
            .is_host_blocked("remote-tracker.example"),
        "installed list loads at startup"
    );
    let _ = Arc::clone(&env.fetcher);
    d.shutdown().await;
}
