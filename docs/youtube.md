# YouTube

## Why YouTube is special

YouTube serves ads from the same domains and servers as videos
(`googlevideo.com`), and decides which ads to show inside the player data.
Blocking ad *servers* therefore can't work: DNS filtering alone will not
remove YouTube ads.

## What FuckAds does

All YouTube handling is **rules, not code changes**, kept in a separate
ruleset ([`filters/youtube.txt`](../filters/youtube.txt)) and in the
community-maintained uBlock filters list:

1. **Network rules** block ad and ad-measurement endpoints
   (`/api/stats/ads`, `/pagead/`, `/ptracking`, doubleclick). Video
   playback is never blocked.
2. **Scriptlets** remove ad placement fields (`adPlacements`, `playerAds`,
   `adSlots`) from the player configuration before the player reads it:
   * `set-constant` traps `ytInitialPlayerResponse.*` for the initial page,
   * `json-prune` hooks `JSON.parse` / `Response.json`,
   * `json-prune-fetch-response` / `json-prune-xhr-response` handle later
     navigations (`/youtubei/v1/player`).
   The scriptlet *implementations* are part of the extension
   (`browser/common/scriptlets.js`). The rules only name a scriptlet and
   pass property names.
3. **Cosmetic rules** hide ad containers (`ytd-ad-slot-renderer`, masthead
   ads, promoted videos).

FuckAds never touches DRM, decrypts anything, or bypasses authentication or
access controls. It removes advertising data from pages the user's own
browser has already received.

## When YouTube changes

YouTube changes frequently, and blocking can break for a while. Fixes ship
as rule updates, without rebuilding FuckAds:

* The uBlock filters list (enabled by default) is typically updated within
  hours to days. FuckAds checks it every 12 hours, or immediately with
  `sudo fuck-ads update`.
* To update the YouTube compatibility rules independently, host a copy of
  `filters/youtube.txt` and point the list at it:

  ```toml
  [[filters.lists]]
  name = "youtube"
  bundled = "youtube.txt"           # used until the first download
  url = "https://your-host.example/youtube.txt"
  category = "ads"
  min_rules = 1
  ```

If a new rule needs a scriptlet that doesn't exist yet, the extension
must be updated. That is a deliberate safety boundary: lists cannot add
code.

## Reliability notes

* On Chrome, turn on **Allow User Scripts** for the extension. Without it,
  scriptlets are injected at navigation commit, which can occasionally lose
  the race against YouTube's own scripts on the first page load.
* Safari: only network and cosmetic rules apply (no scriptlets), so
  in-stream ads may still play.
* Mobile apps and smart TVs: DNS filtering cannot remove YouTube ads there.
