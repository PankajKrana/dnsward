# Bundled filter lists

* `baseline.txt`: a small seed list so blocking works before the first
  download. The main protection comes from the downloaded lists configured
  in `[filters]`.
* `youtube.txt`: YouTube compatibility rules (see `docs/youtube.md`).

Both are plain filter-list files (data, not code). Installers copy them to
`<prefix>/share/fuck-ads/filters/`.
