//! Rule lookup benchmarks: `cargo bench --bench filter_lookup`.
//!
//! Uses a synthetic list shaped like real ones (mostly hostname rules, some
//! URL patterns and cosmetic filters). Set FUCK_ADS_BENCH_LIST=/path/to/list
//! to benchmark against a real downloaded list instead.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fuck_ads_filter::{Engine, Request, ResourceType};
use std::hint::black_box;

fn synthetic_list() -> String {
    let mut s = String::new();
    for i in 0..150_000 {
        s.push_str(&format!("||ads{i}.network{}.example^\n", i % 5_000));
    }
    for i in 0..30_000 {
        s.push_str(&format!(
            "/promo{i}/banner^$third-party\n||cdn{i}.example/ads/*.js$script\n"
        ));
    }
    for i in 0..20_000 {
        s.push_str(&format!(
            "site{i}.example##.sponsored-{i}\n##.ad-slot-{i}\n"
        ));
    }
    s
}

fn engine() -> Engine {
    match std::env::var("FUCK_ADS_BENCH_LIST") {
        Ok(path) => Engine::from_rules(&std::fs::read_to_string(path).expect("readable list")),
        Err(_) => Engine::from_rules(&synthetic_list()),
    }
}

fn bench(c: &mut Criterion) {
    let text = synthetic_list();
    let mut g = c.benchmark_group("build");
    g.sample_size(10);
    g.throughput(Throughput::Elements(text.lines().count() as u64));
    g.bench_function("parse_and_index_250k_rules", |b| {
        b.iter(|| Engine::from_rules(black_box(&text)))
    });
    g.finish();

    let e = engine();
    let mut g = c.benchmark_group("hostname");
    for (label, host) in [
        ("blocked_exact", "ads4242.network4242.example"),
        ("blocked_subdomain", "a.b.ads4242.network4242.example"),
        (
            "allowed_deep",
            "static.assets.cdn.some-news-site.example.co.uk",
        ),
    ] {
        g.bench_with_input(BenchmarkId::from_parameter(label), host, |b, h| {
            b.iter(|| e.check_hostname(black_box(h)))
        });
    }
    g.finish();

    let mut g = c.benchmark_group("url");
    let cases = [
        (
            "blocked_pattern",
            "https://cdn.other.example/promo29999/banner?x=1",
            ResourceType::Image,
        ),
        (
            "blocked_wildcard",
            "https://cdn123.example/ads/a/b/c.js",
            ResourceType::Script,
        ),
        (
            "allowed",
            "https://www.news.example/articles/2026/10/02/story.html?ref=home",
            ResourceType::Document,
        ),
        (
            "allowed_long",
            "https://static.example/assets/js/app.bundle.min.js?v=1234567890&cache=abcdef",
            ResourceType::Script,
        ),
    ];
    for (label, url, ty) in cases {
        let req = Request::new(url, Some("https://www.news.example/"), ty).unwrap();
        g.bench_with_input(BenchmarkId::from_parameter(label), &req, |b, r| {
            b.iter(|| e.check_request(black_box(r)))
        });
    }
    g.finish();

    c.bench_function("cosmetic/page_with_200_tokens", |b| {
        let classes: Vec<String> = (0..150)
            .map(|i| format!("cls-{i}"))
            .chain(["ad-slot-77".into()])
            .collect();
        let ids: Vec<String> = (0..50).map(|i| format!("id-{i}")).collect();
        b.iter(|| e.cosmetic_filters(black_box("www.site77.example"), &classes, &ids, true))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
