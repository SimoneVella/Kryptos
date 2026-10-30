//! Sites that support two-step verification with an authenticator app (TOTP),
//! bundled so the security check can suggest turning it on, without any network.
//!
//! Matched by the name part of the registrable domain ("amazon" for amazon.it
//! and amazon.co.uk), plus a few domains whose name alone would be ambiguous.
//! Only services known to offer authenticator-app codes are listed (not just SMS).

const NAMES: &[&str] = &[
    "1password", "adobe", "amazon", "apple", "atlassian", "autodesk", "backblaze", "binance", "bitbucket", "bitwarden",
    "box", "cloudflare", "coinbase", "digitalocean", "discord", "docker", "dropbox", "ebay", "epicgames", "evernote",
    "facebook", "fastmail", "figma", "gemini", "github", "gitlab", "godaddy", "google", "hetzner", "heroku", "hubspot",
    "huggingface", "icloud", "instagram", "kickstarter", "kraken", "lastpass", "linkedin", "linode", "live", "mailchimp",
    "mega", "microsoft", "mongodb", "namecheap", "netlify", "notion", "npmjs", "okta", "openai", "outlook", "ovh",
    "patreon", "paypal", "pinterest", "proton", "protonmail", "pypi", "reddit", "revolut", "robinhood", "salesforce",
    "sendgrid", "shopify", "slack", "snapchat", "squarespace", "stripe", "tiktok", "trello", "tumblr", "twilio", "twitch",
    "twitter", "vercel", "wise", "wix", "wordpress", "yahoo", "zoho", "zoom",
];

/// Whole registrable domains, for names too generic to match on their own.
const DOMAINS: &[&str] = &["x.com", "ea.com", "asana.com", "sonyentertainmentnetwork.com", "playstation.com", "nintendo.com"];

/// Whether the site at `url` (or "example.com") offers authenticator-app 2FA.
pub fn supported(url: &str) -> bool {
    let Some(host) = crate::import::host_of(url) else { return false };
    let host = host.to_ascii_lowercase();
    let Some(domain) = psl::domain_str(&host) else { return false };
    DOMAINS.contains(&domain) || domain.split('.').next().is_some_and(|name| NAMES.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::supported;

    #[test]
    fn known_sites() {
        assert!(supported("https://github.com/login"));
        assert!(supported("https://www.amazon.it"));
        assert!(supported("accounts.google.com"));
        assert!(supported("https://x.com"));
        assert!(!supported("https://example.com"));
        assert!(!supported("https://github.io"));
        assert!(!supported("not a url"));
    }
}
