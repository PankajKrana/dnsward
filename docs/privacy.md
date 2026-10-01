# Privacy: exactly what AdBlock stores and sends

## Network connections AdBlock makes

| Destination | When | What is sent |
|---|---|---|
| Your DNS upstreams (default: Cloudflare, Google, over HTTPS) | For every allowed query not in the cache | The queried name and type, with no client options |
| Bootstrap DNS servers (default 1.1.1.1, 8.8.8.8) | When the DoH hostnames themselves need resolving | Only the DoH server hostnames |
| Filter-list servers | Every `update_interval_hours` (default 12) | A standard HTTPS GET with `If-None-Match`/`If-Modified-Since` and the user agent `adblock/<version>` |

Nothing else. AdBlock has **no telemetry, analytics, crash reporting, or
update pings**, and `privacy.telemetry = true` is rejected as invalid.

## Data stored on disk

| File | Content | Retention |
|---|---|---|
| `config.toml` | Your settings | Until you change it |
| `data/user-rules.json` | Domains you allowed or blocked | Until you remove them |
| `data/lists/*.txt` | Downloaded filter lists, plus the previous version | Replaced on update |
| `data/update-state.json` | ETag, Last-Modified, SHA-256, rule count, and last update time per list | Replaced on update |
| `data/stats.json` | Per local calendar day: total queries, blocked count, counts per category, cache hits, upstream failures, and (if `stats.record_blocked_domains`) how often each **blocked** domain was blocked, capped at 2,000 domains per day | `stats.retention_days` (default 30); `sudo adblock stats reset` clears it |
| `system-state.json` (root-only) | The DNS settings that were in place before `enable` | Until `disable` |

**Never stored:** allowed (visited) domains, timestamps of individual
queries, client IP addresses, full URLs, page content, or cookies.

## Logs

Logs record service events such as startup, rule counts, update results,
and errors. They do not contain queried names unless you set
`privacy.log_blocked = true`, and even then only *blocked* names are logged.

## Browser extension

The extension sends the local AdBlock service the **hostname** of each frame
and the class/id names present in the page, so the service can pick matching
cosmetic filters. This data goes only to the local process through native
messaging and is never stored. Network blocking runs inside the browser
(`declarativeNetRequest`); requested URLs are never sent to AdBlock.

## Reducing stored data further

```toml
[stats]
enabled = false                  # no statistics at all
# or
record_blocked_domains = false   # counts only, no domain names
```
