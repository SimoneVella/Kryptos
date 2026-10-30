//! Smart cleanup for vaults full of imported logins: readable titles instead of
//! host names ("account.live.com" -> "Microsoft"), and groups of duplicates.
//! Everything here only suggests; the user applies it (see vault.rs).

use std::collections::HashMap;

use serde::Serialize;
use uuid::Uuid;

use crate::entry::Entry;

/// Brand names that differ from their domain, or need a capital in the middle.
const NAMES: &[(&str, &str)] = &[
    ("live.com", "Microsoft"), ("microsoftonline.com", "Microsoft"), ("microsoft.com", "Microsoft"), ("outlook.com", "Outlook"),
    ("office.com", "Microsoft 365"), ("xbox.com", "Xbox"), ("icloud.com", "iCloud"), ("apple.com", "Apple"),
    ("google.com", "Google"), ("youtube.com", "YouTube"), ("gmail.com", "Gmail"), ("github.com", "GitHub"),
    ("gitlab.com", "GitLab"), ("paypal.com", "PayPal"), ("linkedin.com", "LinkedIn"), ("facebook.com", "Facebook"),
    ("instagram.com", "Instagram"), ("whatsapp.com", "WhatsApp"), ("x.com", "X"), ("twitter.com", "X"),
    ("tiktok.com", "TikTok"), ("reddit.com", "Reddit"), ("twitch.tv", "Twitch"), ("discord.com", "Discord"),
    ("netflix.com", "Netflix"), ("spotify.com", "Spotify"), ("amazon.com", "Amazon"), ("amazon.it", "Amazon"),
    ("ebay.com", "eBay"), ("ebay.it", "eBay"), ("booking.com", "Booking.com"), ("airbnb.com", "Airbnb"),
    ("dropbox.com", "Dropbox"), ("openai.com", "OpenAI"), ("chatgpt.com", "ChatGPT"), ("claude.ai", "Claude"),
    ("npmjs.com", "npm"), ("pypi.org", "PyPI"), ("stackoverflow.com", "Stack Overflow"), ("formula1.com", "Formula 1"),
    ("sonyentertainmentnetwork.com", "PlayStation"), ("playstation.com", "PlayStation"), ("blackmagicdesign.com", "Blackmagic Design"),
    ("aruba.it", "Aruba"), ("poste.it", "Poste Italiane"), ("intesasanpaolo.com", "Intesa Sanpaolo"), ("unicredit.it", "UniCredit"),
    ("trenitalia.com", "Trenitalia"), ("italotreno.it", "Italo"), ("studentbeans.com", "Student Beans"), ("broadcom.com", "Broadcom"),
    ("nabucasa.com", "Nabu Casa"), ("steampowered.com", "Steam"), ("epicgames.com", "Epic Games"), ("ea.com", "EA"),
];

/// A better title for `e`, if its current one is just a host name (as CSV
/// imports from browsers leave them). None when the title already reads well.
pub fn suggested_title(e: &Entry) -> Option<String> {
    let title = e.title.trim();
    let looks_like_host = !title.contains(' ') && title.contains('.') && !title.ends_with('.');
    if !looks_like_host {
        return None;
    }
    let host = crate::import::host_of(title).or_else(|| e.urls.first().and_then(|u| crate::import::host_of(u)))?;
    let host = host.to_ascii_lowercase();
    let domain = psl::domain_str(&host)?;
    let name = NAMES.iter().find(|(d, _)| *d == domain).map(|(_, n)| n.to_string()).unwrap_or_else(|| {
        let label = domain.split('.').next().unwrap_or(domain);
        let mut c = label.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    });
    (!name.is_empty() && name != title).then_some(name)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TitleSuggestion {
    pub id: Uuid,
    pub title: String,
    pub suggested: String,
}

pub fn title_suggestions(entries: &[Entry]) -> Vec<TitleSuggestion> {
    entries
        .iter()
        .filter_map(|e| suggested_title(e).map(|s| TitleSuggestion { id: e.id, title: e.title.clone(), suggested: s }))
        .collect()
}

/// Entries for the same site and account: merging keeps the newest.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub ids: Vec<Uuid>,
    pub title: String,
    pub username: String,
    /// All copies have the same password (an exact duplicate).
    pub same_password: bool,
}

/// Groups of entries with the same registrable domain (or, without a site, the
/// same title) and the same username.
pub fn duplicates(entries: &[Entry]) -> Vec<DuplicateGroup> {
    let mut groups: HashMap<(String, String), Vec<&Entry>> = HashMap::new();
    for e in entries {
        let site = e
            .urls
            .iter()
            .find_map(|u| crate::import::host_of(u))
            .map(|h| {
                let h = h.to_ascii_lowercase();
                psl::domain_str(&h).map(str::to_owned).unwrap_or(h)
            })
            .unwrap_or_else(|| e.title.trim().to_lowercase());
        groups.entry((site, e.username.trim().to_lowercase())).or_default().push(e);
    }
    let mut out: Vec<DuplicateGroup> = groups
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|g| {
            let newest = g.iter().max_by_key(|e| e.updated_at).expect("non-empty");
            DuplicateGroup {
                ids: g.iter().map(|e| e.id).collect(),
                title: newest.title.clone(),
                username: newest.username.clone(),
                same_password: g.iter().all(|e| e.password == g[0].password),
            }
        })
        .collect();
    out.sort_by_key(|g| g.title.to_lowercase());
    out
}

/// Why a page looks like an imitation of a saved site.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Lookalike {
    /// The saved site it imitates ("paypal.com"), and that entry's title.
    pub domain: String,
    pub title: String,
}

/// The registrable domain's name part with look-alike characters folded
/// ("paypa1" -> "paypal", "rnicrosoft" -> "microsoft").
fn skeleton(name: &str) -> String {
    name.replace("rn", "m").replace("vv", "w").replace('0', "o").replace('1', "l").replace('3', "e").replace('5', "s").replace('-', "")
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(row[j + 1] + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// Whether the page at `url` imitates a site saved in `entries` without being
/// it: a near-identical name (one or two letters off, or look-alike characters),
/// or the saved domain used as a decoy prefix ("paypal.com.secure-login.io").
/// Pages that match a saved entry are never flagged.
pub fn lookalike(entries: &[Entry], url: &str) -> Option<Lookalike> {
    if entries.iter().any(|e| crate::matching::entry_matches(e, url)) {
        return None;
    }
    let host = crate::import::host_of(url)?.to_ascii_lowercase();
    let page = psl::domain_str(&host)?.to_owned();
    let page_name = page.split('.').next()?.to_owned();
    for e in entries {
        for u in &e.urls {
            let Some(saved_host) = crate::import::host_of(u) else { continue };
            let saved_host = saved_host.to_ascii_lowercase();
            let Some(saved) = psl::domain_str(&saved_host) else { continue };
            if saved == page {
                continue;
            }
            let name = saved.split('.').next().unwrap_or(saved);
            if name.len() < 4 {
                continue;
            }
            let decoy = host.strip_suffix(&page).is_some_and(|sub| sub.split('.').any(|l| l == name));
            let close = page_name != name
                && (skeleton(&page_name) == skeleton(name)
                    || (name.len() >= 6 && edit_distance(&page_name, name) <= 1)
                    || (name.len() >= 9 && edit_distance(&page_name, name) <= 2));
            if decoy || close {
                return Some(Lookalike { domain: saved.to_owned(), title: e.title.clone() });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(title: &str, url: &str, user: &str, pw: &str, updated_at: u64) -> Entry {
        Entry {
            id: Uuid::new_v4(),
            title: title.into(),
            username: user.into(),
            password: pw.into(),
            urls: vec![url.into()],
            notes: String::new(),
            favorite: false,
            totp: String::new(),
            backup_codes: Vec::new(),
            history: Vec::new(),
            ai_access: false,
            created_at: 0,
            updated_at,
        }
    }

    #[test]
    fn titles_from_host_names() {
        assert_eq!(suggested_title(&e("account.live.com", "https://account.live.com", "me", "x", 0)).as_deref(), Some("Microsoft"));
        assert_eq!(suggested_title(&e("access.broadcom.com", "", "me", "x", 0)).as_deref(), Some("Broadcom"));
        assert_eq!(suggested_title(&e("accessoriservato.fondometasalute.it", "", "me", "x", 0)).as_deref(), Some("Fondometasalute"));
        assert_eq!(suggested_title(&e("www.example.co.uk", "", "me", "x", 0)).as_deref(), Some("Example"));
        // Already readable: left alone.
        assert_eq!(suggested_title(&e("My bank", "https://bank.test", "me", "x", 0)), None);
        assert_eq!(suggested_title(&e("GitHub", "https://github.com", "me", "x", 0)), None);
    }

    #[test]
    fn lookalikes() {
        let saved = vec![e("PayPal", "https://www.paypal.com", "me", "x", 0), e("Microsoft", "https://login.live.com", "me", "x", 0)];
        let hit = |u: &str| lookalike(&saved, u).map(|l| l.domain);
        assert_eq!(hit("https://www.paypa1.com/signin").as_deref(), Some("paypal.com"));
        assert_eq!(hit("https://paypal.com.secure-login.io/").as_deref(), Some("paypal.com"));
        assert_eq!(hit("https://paypall.com").as_deref(), Some("paypal.com"));
        // The real site, an unrelated one, and a different name of the same length aren't flagged.
        assert_eq!(hit("https://www.paypal.com/it/signin"), None);
        assert_eq!(hit("https://github.com"), None);
        assert_eq!(hit("https://lives.com"), None);
    }

    #[test]
    fn duplicate_groups() {
        let list = vec![
            e("github.com", "https://github.com", "Me", "a", 1),
            e("GitHub", "https://github.com/login", "me", "a", 2),
            e("GitHub", "https://github.com", "other", "a", 3),
            e("Mail", "https://mail.example.com", "me", "b", 4),
            e("Mail", "https://www.example.com", "me", "c", 5),
        ];
        let groups = duplicates(&list);
        assert_eq!(groups.len(), 2);
        let gh = groups.iter().find(|g| g.title == "GitHub").unwrap();
        assert_eq!((gh.ids.len(), gh.same_password), (2, true));
        let mail = groups.iter().find(|g| g.title == "Mail").unwrap();
        assert_eq!((mail.ids.len(), mail.same_password), (2, false));
    }
}
