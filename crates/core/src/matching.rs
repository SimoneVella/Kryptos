//! Decides which saved logins may be offered to a given page.
//!
//! Rule: same registrable domain (eTLD+1, via the bundled Public Suffix List,
//! no network), so `login.example.com` matches `example.com` but
//! `example.com.evil.io` and `github.io` siblings do not. A login saved for
//! https is never offered to a plain http page.

use url::{Host, Url};

use crate::entry::Entry;

pub fn url_matches(saved: &str, page: &str) -> bool {
    let (Some(saved), Ok(page)) = (parse_lenient(saved), Url::parse(page)) else {
        return false;
    };
    if !matches!(page.scheme(), "https" | "http") {
        return false;
    }
    if saved.scheme() == "https" && page.scheme() != "https" {
        return false;
    }
    match (saved.host(), page.host()) {
        (Some(Host::Domain(a)), Some(Host::Domain(b))) => {
            let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
            match (psl::domain_str(&a), psl::domain_str(&b)) {
                (Some(ra), Some(rb)) => ra == rb,
                // Single-label hosts like "localhost" or intranet names: exact match only.
                _ => a == b && saved.port_or_known_default() == page.port_or_known_default(),
            }
        }
        // IP addresses: exact host and port.
        (Some(a), Some(b)) => a == b && saved.port_or_known_default() == page.port_or_known_default(),
        _ => false,
    }
}

pub fn entry_matches(entry: &Entry, page: &str) -> bool {
    entry.urls.iter().any(|u| url_matches(u, page))
}

/// Accepts "example.com" as shorthand for "https://example.com".
fn parse_lenient(s: &str) -> Option<Url> {
    let s = s.trim();
    Url::parse(s).ok().filter(|u| u.has_host()).or_else(|| Url::parse(&format!("https://{s}")).ok())
}

#[cfg(test)]
mod tests {
    use super::url_matches as m;

    #[test]
    fn same_site() {
        assert!(m("https://example.com", "https://example.com/login"));
        assert!(m("https://example.com", "https://accounts.example.com/"));
        assert!(m("example.com", "https://www.example.com"));
        assert!(m("https://bbc.co.uk", "https://www.bbc.co.uk/signin"));
    }

    #[test]
    fn lookalikes_rejected() {
        assert!(!m("https://example.com", "https://example.com.evil.io"));
        assert!(!m("https://example.com", "https://notexample.com"));
        assert!(!m("https://alice.github.io", "https://mallory.github.io"));
        assert!(!m("https://a.co.uk", "https://b.co.uk"));
    }

    #[test]
    fn no_https_downgrade() {
        assert!(!m("https://example.com", "http://example.com"));
        assert!(m("http://example.com", "https://example.com"));
    }

    #[test]
    fn local_hosts_exact() {
        assert!(m("http://localhost:3000", "http://localhost:3000/x"));
        assert!(!m("http://localhost:3000", "http://localhost:4000"));
        assert!(m("http://192.168.1.1", "http://192.168.1.1/admin"));
        assert!(!m("http://192.168.1.1", "http://192.168.1.2"));
    }

    #[test]
    fn non_web_schemes_rejected() {
        assert!(!m("https://example.com", "file:///etc/passwd"));
        assert!(!m("https://example.com", "javascript:alert(1)"));
    }
}
