//! Conversion of compiled filters into browser-native rule formats.
//!
//! * [`dnr_rules`] — Chromium / Firefox `declarativeNetRequest` dynamic rules.
//!   The browser evaluates them natively, so no request data ever leaves the
//!   browser and no TLS interception is needed.
//! * [`safari_content_blocker`] — Safari Content Blocker JSON.
//!
//! Both formats are more limited than the filter syntax; filters that cannot
//! be expressed faithfully are skipped rather than approximated.

use crate::domain::HostPattern;
use crate::engine::Engine;
use crate::network::{Anchor, NetworkFilter, Party};
use crate::request::{ResourceType, ResourceTypes};
use serde_json::{Value, json};

/// Priorities: higher wins in DNR.
mod priority {
    /// Query-parameter stripping yields to every blocking and allow rule.
    pub const REMOVE_PARAM: u32 = 1;
    pub const BLOCK: u32 = 2;
    pub const ALLOW: u32 = 3;
    pub const IMPORTANT_BLOCK: u32 = 4;
    pub const IMPORTANT_ALLOW: u32 = 5;
    pub const USER_BLOCK: u32 = 6;
    pub const USER_ALLOW: u32 = 7;
}

/// Hostnames per `requestDomains` rule.
const DOMAIN_CHUNK: usize = 1000;

/// User overrides from `adblock allow` / `adblock block`.
#[derive(Debug, Clone, Default)]
pub struct UserOverrides<'a> {
    pub allow: &'a [String],
    pub block: &'a [String],
}

fn plain_domains(patterns: &[HostPattern]) -> Option<Vec<&str>> {
    patterns
        .iter()
        .map(|p| match p {
            HostPattern::Domain(d) => Some(d.as_str()),
            HostPattern::Entity(_) => None,
        })
        .collect()
}

fn resource_types(types: ResourceTypes) -> Vec<&'static str> {
    types.iter().map(ResourceType::dnr_name).collect()
}

fn url_filter(f: &NetworkFilter) -> Option<String> {
    if f.regex.is_some() || !f.pattern.is_ascii() {
        return None;
    }
    let mut s = String::with_capacity(f.pattern.len() + 3);
    match f.anchor {
        Anchor::Hostname => s.push_str("||"),
        Anchor::Left => s.push('|'),
        Anchor::None => {}
    }
    s.push_str(&f.pattern);
    if f.end_anchor {
        s.push('|');
    }
    // DNR forbids a urlFilter starting with `||*`.
    if s.starts_with("||*") {
        return None;
    }
    Some(s)
}

fn network_condition(f: &NetworkFilter, types: ResourceTypes) -> Option<Value> {
    let mut cond = serde_json::Map::new();
    {
        let filter = url_filter(f)?;
        if !filter.is_empty() {
            cond.insert("urlFilter".into(), json!(filter));
        }
    }
    if f.match_case {
        cond.insert("isUrlFilterCaseSensitive".into(), json!(true));
    }
    cond.insert("resourceTypes".into(), json!(resource_types(types)));
    match f.party {
        Some(Party::Third) => {
            cond.insert("domainType".into(), json!("thirdParty"));
        }
        Some(Party::First) => {
            cond.insert("domainType".into(), json!("firstParty"));
        }
        None => {}
    }
    if let Some(d) = &f.domains {
        let include = plain_domains(&d.include)?;
        let exclude = plain_domains(&d.exclude)?;
        if !include.is_empty() {
            cond.insert("initiatorDomains".into(), json!(include));
        }
        if !exclude.is_empty() {
            cond.insert("excludedInitiatorDomains".into(), json!(exclude));
        }
    }
    Some(Value::Object(cond))
}

/// The DNR action for a blocking filter: plain `block`, or a redirect to a
/// packaged stub resource for `$redirect` filters.
fn block_action(f: &NetworkFilter) -> Value {
    match f.redirect {
        Some(file) => json!({
            "type": "redirect",
            "redirect": { "extensionPath": format!("/resources/{file}") }
        }),
        None => json!({ "type": "block" }),
    }
}

struct RuleSink {
    rules: Vec<Value>,
    max: usize,
}

impl RuleSink {
    fn push(&mut self, priority: u32, action: Value, condition: Value) -> bool {
        if self.rules.len() >= self.max {
            return false;
        }
        let id = self.rules.len() + 1;
        self.rules.push(
            json!({ "id": id, "priority": priority, "action": action, "condition": condition }),
        );
        true
    }

    fn push_domains<'a>(
        &mut self,
        priority: u32,
        action: Value,
        domains: impl Iterator<Item = &'a str>,
    ) {
        let mut all: Vec<&str> = domains.collect();
        all.sort_unstable();
        for chunk in all.chunks(DOMAIN_CHUNK) {
            if !self.push(priority, action.clone(), json!({ "requestDomains": chunk })) {
                return;
            }
        }
    }
}

/// Builds `declarativeNetRequest` rules, capped at `max_rules` (Chromium
/// allows 30,000 dynamic rules). Most important rules come first so the cap
/// only ever drops the least specific blocking rules.
pub fn dnr_rules(engine: &Engine, user: &UserOverrides<'_>, max_rules: usize) -> Vec<Value> {
    let mut sink = RuleSink {
        rules: Vec::new(),
        max: max_rules,
    };
    let block = json!({ "type": "block" });
    let allow = json!({ "type": "allow" });

    if !user.allow.is_empty() {
        // Allow everything on and from allowlisted sites.
        sink.push(
            priority::USER_ALLOW,
            json!({ "type": "allowAllRequests" }),
            json!({ "requestDomains": user.allow, "resourceTypes": ["main_frame", "sub_frame"] }),
        );
        sink.push(
            priority::USER_ALLOW,
            allow.clone(),
            json!({ "requestDomains": user.allow }),
        );
        sink.push(
            priority::USER_ALLOW,
            allow.clone(),
            json!({ "initiatorDomains": user.allow }),
        );
    }
    if !user.block.is_empty() {
        sink.push_domains(
            priority::USER_BLOCK,
            block.clone(),
            user.block.iter().map(String::as_str),
        );
    }
    sink.push_domains(priority::ALLOW, allow.clone(), engine.allowed_host_list());
    sink.push_domains(
        priority::IMPORTANT_BLOCK,
        block.clone(),
        engine.important_host_list(),
    );

    let mut deferred_blocks: Vec<&NetworkFilter> = Vec::new();
    for (f, _) in engine.network_filters() {
        if !f.exception && !f.important {
            deferred_blocks.push(f);
            continue;
        }
        let prio = match (f.exception, f.important) {
            (true, true) => priority::IMPORTANT_ALLOW,
            (true, false) => priority::ALLOW,
            _ => priority::IMPORTANT_BLOCK,
        };
        if f.exception && f.types.contains(ResourceType::Document) {
            let doc_types =
                ResourceTypes(ResourceType::Document.bit() | ResourceType::Subdocument.bit());
            if let Some(cond) = network_condition(f, doc_types) {
                sink.push(prio, json!({ "type": "allowAllRequests" }), cond);
            }
        }
        let other = ResourceTypes(f.types.0 & !ResourceType::Document.bit());
        if other != ResourceTypes::NONE
            && let Some(cond) = network_condition(f, other)
        {
            let action = if f.exception {
                allow.clone()
            } else {
                block_action(f)
            };
            sink.push(prio, action, cond);
        }
    }

    sink.push_domains(priority::BLOCK, block.clone(), engine.blocked_host_list());
    for f in deferred_blocks {
        if let Some(cond) = network_condition(f, f.types)
            && !sink.push(priority::BLOCK, block_action(f), cond)
        {
            break;
        }
    }
    for f in engine.param_filters() {
        let Some(name) = f.removeparam.as_deref() else {
            continue;
        };
        if let Some(cond) = network_condition(f, f.types)
            && !sink.push(
                priority::REMOVE_PARAM,
                json!({
                    "type": "redirect",
                    "redirect": { "transform": { "queryTransform": { "removeParams": [name] } } }
                }),
                cond,
            )
        {
            break;
        }
    }
    sink.rules
}

/// Escapes a literal for Safari's regex subset.
fn safari_escape(out: &mut String, c: char) {
    if matches!(
        c,
        '.' | '?' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' | '$' | '^' | '/'
    ) {
        out.push('\\');
    }
    out.push(c);
}

fn safari_url_filter(f: &NetworkFilter) -> Option<String> {
    if f.regex.is_some() || !f.pattern.is_ascii() {
        return None;
    }
    let mut re = String::new();
    match f.anchor {
        Anchor::Hostname => re.push_str("^[a-z-]+://([^/]*\\.)?"),
        Anchor::Left => re.push('^'),
        Anchor::None => {}
    }
    let mut in_host = f.anchor == Anchor::Hostname;
    for c in f.pattern.chars() {
        match c {
            '*' => re.push_str(".*"),
            '^' => {
                re.push_str(if in_host { "[/:]" } else { "[/:&?=]" });
                in_host = false;
            }
            '/' => {
                in_host = false;
                safari_escape(&mut re, c);
            }
            _ => safari_escape(&mut re, c),
        }
    }
    if f.end_anchor {
        re.push('$');
    }
    if re.is_empty() {
        re.push_str(".*");
    }
    Some(re)
}

fn safari_domains(patterns: &[HostPattern]) -> Option<Vec<String>> {
    patterns
        .iter()
        .map(|p| match p {
            HostPattern::Domain(d) => Some(format!("*{d}")),
            HostPattern::Entity(_) => None,
        })
        .collect()
}

fn safari_trigger(f: &NetworkFilter) -> Option<Value> {
    let mut t = serde_json::Map::new();
    t.insert("url-filter".into(), json!(safari_url_filter(f)?));
    if f.match_case {
        t.insert("url-filter-is-case-sensitive".into(), json!(true));
    }
    if f.explicit_types {
        let mut types: Vec<&str> = f
            .types
            .iter()
            .filter_map(ResourceType::safari_name)
            .collect();
        types.sort_unstable();
        types.dedup();
        if types.is_empty() {
            return None;
        }
        t.insert("resource-type".into(), json!(types));
    }
    match f.party {
        Some(Party::Third) => {
            t.insert("load-type".into(), json!(["third-party"]));
        }
        Some(Party::First) => {
            t.insert("load-type".into(), json!(["first-party"]));
        }
        None => {}
    }
    if let Some(d) = &f.domains {
        match (d.include.is_empty(), d.exclude.is_empty()) {
            (false, true) => {
                t.insert("if-domain".into(), json!(safari_domains(&d.include)?));
            }
            (true, false) => {
                t.insert("unless-domain".into(), json!(safari_domains(&d.exclude)?));
            }
            (false, false) => return None, // Safari cannot combine both
            (true, true) => {}
        }
    }
    Some(Value::Object(t))
}

/// Builds a Safari content blocker (`[{trigger, action}]`), capped at
/// `max_rules` (Safari accepts up to 150,000 per blocker).
pub fn safari_content_blocker(
    engine: &Engine,
    user: &UserOverrides<'_>,
    max_rules: usize,
) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut ignores = Vec::new();

    for host in engine
        .important_host_list()
        .chain(engine.blocked_host_list())
        .chain(user.block.iter().map(String::as_str))
    {
        let mut escaped = String::new();
        host.chars().for_each(|c| safari_escape(&mut escaped, c));
        blocks.push(json!({
            "trigger": { "url-filter": format!("^[a-z-]+://([^/]*\\.)?{escaped}[/:]") },
            "action": { "type": "block" }
        }));
    }
    for (f, _) in engine.network_filters() {
        // Safari cannot serve a stand-in resource; blocking instead could
        // break the page, so redirect filters are left out.
        if f.redirect.is_some() {
            continue;
        }
        if let Some(trigger) = safari_trigger(f) {
            let rule = json!({
                "trigger": trigger,
                "action": { "type": if f.exception { "ignore-previous-rules" } else { "block" } }
            });
            if f.exception {
                ignores.push(rule)
            } else {
                blocks.push(rule)
            }
        }
    }

    // Site-specific element hiding, grouped per site.
    let mut by_site: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for (host, selector) in engine.site_cosmetics() {
        by_site.entry(host).or_default().push(selector);
    }
    let excepted: std::collections::HashSet<&str> = engine
        .hide_exception_list()
        .filter(|(d, _)| d.is_none())
        .map(|(_, s)| s)
        .collect();
    for (host, selectors) in by_site {
        let selectors: Vec<&str> = selectors
            .into_iter()
            .filter(|s| !excepted.contains(s))
            .collect();
        for chunk in selectors.chunks(200) {
            blocks.push(json!({
                "trigger": { "url-filter": ".*", "if-domain": [format!("*{host}")] },
                "action": { "type": "css-display-none", "selector": chunk.join(", ") }
            }));
        }
    }

    for host in engine.allowed_host_list() {
        let mut escaped = String::new();
        host.chars().for_each(|c| safari_escape(&mut escaped, c));
        ignores.push(json!({
            "trigger": { "url-filter": format!("^[a-z-]+://([^/]*\\.)?{escaped}[/:]") },
            "action": { "type": "ignore-previous-rules" }
        }));
    }
    if !user.allow.is_empty() {
        let domains: Vec<String> = user.allow.iter().map(|d| format!("*{d}")).collect();
        ignores.push(json!({
            "trigger": { "url-filter": ".*", "if-domain": domains },
            "action": { "type": "ignore-previous-rules" }
        }));
    }

    // Exceptions must follow the rules they override; keep them all and trim
    // blocking rules to fit.
    let budget = max_rules.saturating_sub(ignores.len());
    blocks.truncate(budget);
    blocks.extend(ignores);
    blocks.truncate(max_rules);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Request;

    #[test]
    fn dnr_conversion() {
        let engine = Engine::from_rules(
            "||ads.example^\n||t.example/p^$third-party,image,domain=a.com|~b.a.com\n@@||ok.example^$document\n/re[0-9]/\n",
        );
        let allow = vec!["mysite.example".to_string()];
        let rules = dnr_rules(
            &engine,
            &UserOverrides {
                allow: &allow,
                block: &[],
            },
            100,
        );
        let text = serde_json::to_string(&rules).unwrap();
        assert!(text.contains("\"requestDomains\":[\"ads.example\"]"));
        assert!(text.contains("\"urlFilter\":\"||t.example/p^\""));
        assert!(text.contains("\"domainType\":\"thirdParty\""));
        assert!(text.contains("\"excludedInitiatorDomains\":[\"b.a.com\"]"));
        assert!(text.contains("allowAllRequests"));
        assert!(!text.contains("re[0-9]"), "regex filters are not exported");
        // IDs are unique and sequential.
        for (i, r) in rules.iter().enumerate() {
            assert_eq!(r["id"], i + 1);
        }
        assert!(dnr_rules(&engine, &UserOverrides::default(), 2).len() <= 2);
    }

    #[test]
    fn dnr_redirect() {
        let engine = Engine::from_rules(
            "||t.example/ads.js^$script,redirect=noop.js\n||t.example/i^$important,image,redirect=1x1.gif\n",
        );
        let rules = dnr_rules(&engine, &UserOverrides::default(), 100);
        let paths: Vec<_> = rules
            .iter()
            .map(|r| r["action"]["redirect"]["extensionPath"].as_str())
            .collect();
        assert!(paths.contains(&Some("/resources/noop.js")));
        assert!(paths.contains(&Some("/resources/1x1.gif")));
        assert!(rules.iter().all(|r| r["action"]["type"] == "redirect"));
        // Redirects are still blocks for URL checks.
        let req = Request::new(
            "https://t.example/ads.js",
            Some("https://a.example/"),
            ResourceType::Script,
        )
        .unwrap();
        assert!(engine.check_request(&req).is_blocked());
        assert!(
            safari_content_blocker(&engine, &UserOverrides::default(), 1000)
                .iter()
                .all(|r| !r.to_string().contains("ads\\\\.js"))
        );
    }

    #[test]
    fn dnr_removeparam() {
        let engine = Engine::from_rules("$removeparam=utm_source\n||ads.example^\n");
        let rules = dnr_rules(&engine, &UserOverrides::default(), 100);
        let rule = rules
            .iter()
            .find(|r| r["action"]["type"] == "redirect")
            .expect("redirect rule");
        assert_eq!(
            rule["action"]["redirect"]["transform"]["queryTransform"]["removeParams"][0],
            "utm_source"
        );
        assert_eq!(rule["priority"], priority::REMOVE_PARAM);
        assert!(rule["condition"].get("urlFilter").is_none());
        assert!(
            safari_content_blocker(&engine, &UserOverrides::default(), 1000)
                .iter()
                .all(|r| !r.to_string().contains("utm_source"))
        );
    }

    #[test]
    fn safari_conversion() {
        let engine = Engine::from_rules("||ads.example^\nexample.com##.ad\n@@||ok.example^\n");
        let rules = safari_content_blocker(&engine, &UserOverrides::default(), 1000);
        let text = serde_json::to_string(&rules).unwrap();
        assert!(text.contains("ads\\\\.example[/:]"));
        assert!(text.contains("css-display-none"));
        assert_eq!(
            rules.last().unwrap()["action"]["type"],
            "ignore-previous-rules"
        );
    }
}
