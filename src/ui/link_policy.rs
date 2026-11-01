//! Decides which URLs in a message get a link-preview card and what the preview
//! fetcher is allowed to contact.
//!
//! Three tiers:
//! * `Skip` – URL is part of a shell/HTTP-tool command or code block (`curl …`),
//!   or carries credentials. No card at all.
//! * `OpenOnly` – ordinary link. Card with domain + "Open link", never fetched.
//! * `Preview`  – https link on a known social/media host. Card with "Load preview".

use reqwest::Url;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

/// Hosts (and all their subdomains) allowed to be fetched for a preview.
const PREVIEW_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "x.com",
    "twitter.com",
    "facebook.com",
    "fb.watch",
    "instagram.com",
    "threads.net",
    "threads.com",
    "tiktok.com",
    "reddit.com",
    "redd.it",
    "bsky.app",
    "linkedin.com",
    "pinterest.com",
    "twitch.tv",
    "vimeo.com",
    "soundcloud.com",
    "spotify.com",
    "github.com",
    "wikipedia.org",
    "imgur.com",
    "line.me",
    "lin.ee",
    "zalo.me",
];

/// Command-line HTTP clients; any URL after one of these on the same line is an
/// argument to a command, not a link someone is sharing.
const HTTP_TOOLS: &[&str] = &["curl", "wget", "httpie", "xh", "aria2c"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinkKind {
    Skip,
    OpenOnly,
    Preview,
}

/// URLs in `text` (in order) with the treatment each one gets.
pub(super) fn classify_links(text: &str) -> Vec<(String, LinkKind)> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        let mut after_tool = false;
        for token in line.split_whitespace() {
            let bare = token.trim_start_matches(['$', '>', '#']);
            if HTTP_TOOLS.contains(&bare) {
                after_tool = true;
                continue;
            }
            let Some(url) = clean_url(token) else {
                continue;
            };
            let kind = if in_fence || after_tool {
                LinkKind::Skip
            } else {
                classify_url(&url)
            };
            out.push((url, kind));
        }
    }
    out
}

fn clean_url(token: &str) -> Option<String> {
    let t = token
        .trim_matches(|c: char| matches!(c, ')' | '(' | ']' | '[' | '.' | ',' | ';' | '"' | '\''));
    (t.starts_with("http://") || t.starts_with("https://")).then(|| t.to_string())
}

fn classify_url(url: &str) -> LinkKind {
    let Ok(parsed) = Url::parse(url) else {
        return LinkKind::Skip;
    };
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_none()
    {
        return LinkKind::Skip;
    }
    if is_preview_url(&parsed) {
        LinkKind::Preview
    } else {
        LinkKind::OpenOnly
    }
}

/// True when `url` may be fetched for a preview: https, default port, no
/// credentials, and a host on the allowlist (never an IP literal).
pub(super) fn is_preview_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.domain().is_some()
        && url.host_str().is_some_and(host_allowed)
}

/// Looser check for a thumbnail on a CDN: https, default port, a real domain name.
/// Private/loopback targets are rejected by [`PublicOnlyResolver`] at connect time.
pub(super) fn is_image_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.domain().is_some()
}

fn host_allowed(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    PREVIEW_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// DNS resolver that refuses to hand out loopback, private, link-local and other
/// non-public addresses, so an allowlisted or CDN name cannot be pointed (or
/// rebound) at the local network.
pub(super) struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let (tx, rx) = async_channel::bounded(1);
            std::thread::spawn(move || {
                let result = (host.as_str(), 0).to_socket_addrs().map(|addrs| {
                    addrs
                        .filter(|a| is_public_ip(&a.ip()))
                        .collect::<Vec<SocketAddr>>()
                });
                let _ = tx.send_blocking(result);
            });
            let addrs = rx
                .recv()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            if addrs.is_empty() {
                return Err("no public address".into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

pub(super) fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_public_v4(&mapped);
            }
            is_public_v6(v6)
        }
    }
}

fn is_public_v4(ip: &Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..=127).contains(&b)) // CGNAT 100.64.0.0/10
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || a >= 240) // reserved
}

fn is_public_v6(ip: &Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00 // unique local fc00::/7
        || (first & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (first == 0x2001 && ip.segments()[1] == 0x0db8)) // documentation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<LinkKind> {
        classify_links(text).into_iter().map(|(_, k)| k).collect()
    }

    #[test]
    fn social_links_get_previews() {
        assert_eq!(
            kinds("xem https://www.youtube.com/watch?v=abc và https://x.com/u/status/1"),
            [LinkKind::Preview, LinkKind::Preview]
        );
        assert_eq!(kinds("https://youtu.be/abc"), [LinkKind::Preview]);
    }

    #[test]
    fn curl_and_code_urls_are_skipped() {
        assert_eq!(
            kinds("curl -X POST https://api.example.com/v1"),
            [LinkKind::Skip]
        );
        assert_eq!(kinds("$ wget https://youtube.com/x"), [LinkKind::Skip]);
        assert_eq!(kinds("sudo curl -s https://github.com/a"), [LinkKind::Skip]);
        assert_eq!(
            kinds("```\nhttps://youtube.com/watch?v=a\n```\nhttps://youtube.com/watch?v=b"),
            [LinkKind::Skip, LinkKind::Preview]
        );
        // The next line is not part of the curl command.
        assert_eq!(
            kinds("curl https://a.example\nhttps://github.com/x"),
            [LinkKind::Skip, LinkKind::Preview]
        );
    }

    #[test]
    fn unknown_and_unsafe_hosts_are_never_fetched() {
        assert_eq!(kinds("https://example.com/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("http://youtube.com/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("https://youtube.com:8443/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("https://127.0.0.1/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("https://[::1]/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("https://youtube.com.evil.io/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("https://evilyoutube.com/a"), [LinkKind::OpenOnly]);
        assert_eq!(kinds("https://user:pw@youtube.com/a"), [LinkKind::Skip]);
    }

    #[test]
    fn non_public_addresses_are_rejected() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(&ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "142.250.80.46", "2606:4700::1111"] {
            assert!(is_public_ip(&ip.parse().unwrap()), "{ip}");
        }
    }
}
