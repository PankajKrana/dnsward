//! Request descriptions used for URL-level matching.

use crate::domain::{normalize_hostname, registrable_domain, url_host_range};
use serde::{Deserialize, Serialize};

/// Resource types understood by the filter syntax. The bit values form a
/// [`ResourceTypes`] mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Document,
    Subdocument,
    Script,
    Stylesheet,
    Image,
    Font,
    Media,
    Object,
    XmlHttpRequest,
    Websocket,
    Ping,
    Other,
}

impl ResourceType {
    pub const ALL: [ResourceType; 12] = [
        ResourceType::Document,
        ResourceType::Subdocument,
        ResourceType::Script,
        ResourceType::Stylesheet,
        ResourceType::Image,
        ResourceType::Font,
        ResourceType::Media,
        ResourceType::Object,
        ResourceType::XmlHttpRequest,
        ResourceType::Websocket,
        ResourceType::Ping,
        ResourceType::Other,
    ];

    pub const fn bit(self) -> u16 {
        1 << (self as u16)
    }

    /// Name used by the Chromium/Firefox `declarativeNetRequest` API.
    pub const fn dnr_name(self) -> &'static str {
        match self {
            ResourceType::Document => "main_frame",
            ResourceType::Subdocument => "sub_frame",
            ResourceType::Script => "script",
            ResourceType::Stylesheet => "stylesheet",
            ResourceType::Image => "image",
            ResourceType::Font => "font",
            ResourceType::Media => "media",
            ResourceType::Object => "object",
            ResourceType::XmlHttpRequest => "xmlhttprequest",
            ResourceType::Websocket => "websocket",
            ResourceType::Ping => "ping",
            ResourceType::Other => "other",
        }
    }

    /// Name used by Safari content blockers, if Safari distinguishes the type.
    pub const fn safari_name(self) -> Option<&'static str> {
        match self {
            ResourceType::Document => Some("document"),
            ResourceType::Subdocument => Some("document"),
            ResourceType::Script => Some("script"),
            ResourceType::Stylesheet => Some("style-sheet"),
            ResourceType::Image => Some("image"),
            ResourceType::Font => Some("font"),
            ResourceType::Media => Some("media"),
            ResourceType::Object => None,
            ResourceType::XmlHttpRequest => Some("fetch"),
            ResourceType::Websocket => Some("websocket"),
            ResourceType::Ping => Some("ping"),
            ResourceType::Other => Some("other"),
        }
    }

    /// Parses the name of a filter option (`script`, `xhr`, `frame`, ...).
    pub fn from_option(name: &str) -> Option<Self> {
        Some(match name {
            "document" | "doc" => ResourceType::Document,
            "subdocument" | "frame" => ResourceType::Subdocument,
            "script" => ResourceType::Script,
            "stylesheet" | "css" => ResourceType::Stylesheet,
            "image" => ResourceType::Image,
            "font" => ResourceType::Font,
            "media" => ResourceType::Media,
            "object" | "object-subrequest" => ResourceType::Object,
            "xmlhttprequest" | "xhr" => ResourceType::XmlHttpRequest,
            "websocket" => ResourceType::Websocket,
            "ping" | "beacon" => ResourceType::Ping,
            "other" => ResourceType::Other,
            _ => return None,
        })
    }
}

/// Bit mask of resource types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceTypes(pub u16);

impl ResourceTypes {
    /// Every type except `document`: the default for network filters without
    /// a type option (top-level navigations are only blocked when requested
    /// explicitly, matching uBlock Origin / Adblock Plus semantics).
    pub const DEFAULT: ResourceTypes =
        ResourceTypes(!ResourceType::Document.bit() & ((1 << ResourceType::ALL.len()) - 1));
    pub const ALL: ResourceTypes = ResourceTypes((1 << ResourceType::ALL.len()) - 1);
    pub const NONE: ResourceTypes = ResourceTypes(0);

    pub fn contains(self, t: ResourceType) -> bool {
        self.0 & t.bit() != 0
    }

    pub fn iter(self) -> impl Iterator<Item = ResourceType> {
        ResourceType::ALL
            .into_iter()
            .filter(move |t| self.contains(*t))
    }
}

/// A network request to evaluate against URL filters.
#[derive(Debug, Clone)]
pub struct Request {
    /// The URL, lowercased for case-insensitive matching.
    pub url_lower: String,
    /// The URL exactly as requested, used by `$match-case` filters.
    pub url: String,
    pub host_range: (usize, usize),
    pub hostname: String,
    /// Hostname of the page (or frame) that issued the request.
    pub source_hostname: Option<String>,
    pub resource_type: ResourceType,
    pub third_party: bool,
}

impl Request {
    /// Builds a request. `source_url` is the URL of the initiating document,
    /// if known.
    pub fn new(url: &str, source_url: Option<&str>, resource_type: ResourceType) -> Option<Self> {
        let host_range = url_host_range(url)?;
        let hostname = normalize_hostname(url[host_range.0..host_range.1].trim_matches(['[', ']']))
            .unwrap_or_else(|| url[host_range.0..host_range.1].to_ascii_lowercase());
        let source_hostname = source_url
            .and_then(|s| url_host_range(s).map(|(a, b)| s[a..b].to_ascii_lowercase()))
            .filter(|h| !h.is_empty());
        let third_party = match &source_hostname {
            Some(src) => registrable_domain(src) != registrable_domain(&hostname),
            None => false,
        };
        Some(Request {
            url_lower: url.to_ascii_lowercase(),
            url: url.to_string(),
            host_range,
            hostname,
            source_hostname,
            resource_type,
            third_party,
        })
    }
}
