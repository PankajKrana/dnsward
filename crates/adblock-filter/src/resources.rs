//! Neutral stand-ins for blocked resources (`$redirect=noop.js`).
//!
//! A `$redirect` filter blocks the request and serves one of these files
//! instead, so pages that expect the script, image or frame to exist keep
//! working. The files live in `browser/common/resources/` and ship inside the
//! extension; the DNR export points at them by path. Only names listed here
//! are accepted, so a list can never redirect to anything else.

/// `(accepted names, file name under resources/)`. Names are the uBlock
/// Origin and Adblock Plus spellings.
const RESOURCES: &[(&[&str], &str)] = &[
    (&["noop.js", "noopjs"], "noop.js"),
    (&["noop.css", "noopcss"], "noop.css"),
    (&["noop.html", "noopframe"], "noop.html"),
    (&["noop.txt", "nooptext"], "noop.txt"),
    (&["1x1.gif", "1x1-transparent.gif"], "1x1.gif"),
    (&["2x2.png", "2x2-transparent.png"], "2x2.png"),
    (
        &[
            "google-analytics.com/ga.js",
            "google-analytics_ga.js",
            "ga.js",
        ],
        "ga.js",
    ),
    (
        &[
            "google-analytics.com/analytics.js",
            "google-analytics_analytics.js",
            "analytics.js",
        ],
        "analytics.js",
    ),
    (
        &[
            "googletagmanager.com/gtm.js",
            "googletagmanager_gtm.js",
            "gtm.js",
        ],
        "gtm.js",
    ),
    (
        &[
            "googletagservices.com/gpt.js",
            "googletagservices_gpt.js",
            "gpt.js",
        ],
        "gpt.js",
    ),
    (
        &[
            "amazon-adsystem.com/aax2/apstag.js",
            "amazon_apstag.js",
            "apstag.js",
        ],
        "apstag.js",
    ),
    (
        &[
            "scorecardresearch.com/beacon.js",
            "scorecardresearch_beacon.js",
            "beacon.js",
        ],
        "beacon.js",
    ),
];

/// Maps a `$redirect=` value to the packaged file name.
pub fn lookup(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    RESOURCES
        .iter()
        .find(|(names, _)| names.contains(&name.as_str()))
        .map(|(_, file)| *file)
}

/// Every packaged file name.
pub fn files() -> impl Iterator<Item = &'static str> {
    RESOURCES.iter().map(|(_, file)| *file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve() {
        assert_eq!(lookup("noopjs"), Some("noop.js"));
        assert_eq!(lookup("1X1-Transparent.GIF"), Some("1x1.gif"));
        assert_eq!(lookup("../evil.js"), None);
        assert_eq!(lookup("google-analytics.js"), None);
        assert_eq!(lookup("Google-Analytics.com/GA.js"), Some("ga.js"));
        assert_eq!(lookup("googletagservices_gpt.js"), Some("gpt.js"));
    }

    #[test]
    fn every_file_is_packaged() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../browser/common/resources");
        for file in files() {
            assert!(dir.join(file).is_file(), "missing {file}");
        }
    }
}
