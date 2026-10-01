//! DNS query processing benchmarks: `cargo bench --bench dns_processing`.
//!
//! Measures the full in-process path (parse, filter, cache, serialize)
//! without network I/O: blocked answers and cache hits.

use adblock_config::Config;
use adblock_dns::{Decision, QueryFilter, Resolver, ResolverConfig, Transport};
use adblock_filter::{Category, Engine, HostVerdict};
use adblock_stats::StatsStore;
use criterion::{Criterion, criterion_group, criterion_main};
use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::{Name, RecordType};
use std::hint::black_box;
use std::sync::Arc;

struct EngineFilter(Engine);

impl QueryFilter for EngineFilter {
    fn check(&self, host: &str) -> Decision {
        match self.0.check_hostname(host) {
            HostVerdict::Blocked { domain, .. } => Decision::Block {
                domain,
                category: Category::Ads,
            },
            _ => Decision::Allow,
        }
    }
}

fn query(name: &str) -> Vec<u8> {
    let mut m = Message::new(0x1234, MessageType::Query, OpCode::Query);
    m.metadata.recursion_desired = true;
    m.add_query(Query::query(Name::from_ascii(name).unwrap(), RecordType::A));
    m.to_vec().unwrap()
}

fn bench(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let rules: String = (0..100_000)
        .map(|i| format!("||ads{i}.example^\n"))
        .collect();
    let filter = Arc::new(EngineFilter(Engine::from_rules(&rules)));
    let stats = Arc::new(StatsStore::open(None, true, true));

    // Upstream: the in-process mock from the test support crate.
    let upstream = rt.block_on(adblock_tests::MockUpstream::start());
    let mut config = Config::default();
    config.dns.upstreams = vec![upstream.addr.to_string()];
    let resolver = Resolver::new(ResolverConfig::from_config(&config), filter, stats).unwrap();

    let blocked = query("ads4242.example.");
    c.bench_function("dns/blocked_query", |b| {
        b.iter(|| {
            rt.block_on(resolver.handle(black_box(&blocked), Transport::Udp))
                .unwrap()
        })
    });

    let cached = query("www.cached.example.");
    rt.block_on(resolver.handle(&cached, Transport::Udp))
        .unwrap(); // warm the cache
    c.bench_function("dns/cache_hit", |b| {
        b.iter(|| {
            rt.block_on(resolver.handle(black_box(&cached), Transport::Udp))
                .unwrap()
        })
    });

    let local = query("adblock-probe.test.");
    c.bench_function("dns/local_answer", |b| {
        b.iter(|| {
            rt.block_on(resolver.handle(black_box(&local), Transport::Udp))
                .unwrap()
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
