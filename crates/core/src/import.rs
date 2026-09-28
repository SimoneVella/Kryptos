//! CSV import from other password managers. Columns are matched by header name,
//! which covers Google Password Manager / Chrome / Edge / Brave, Firefox,
//! Bitwarden, 1Password, Dashlane and most generic exports.

use crate::entry::EntryInput;
use crate::error::{Error, Result};

const TITLE: &[&str] = &["name", "title", "account", "login_name"];
const URL: &[&str] = &["url", "uri", "login_uri", "website", "web site", "origin", "hostname"];
const USER: &[&str] = &["username", "login_username", "login", "user", "email", "user name", "e-mail"];
const PASS: &[&str] = &["password", "login_password", "pass"];
const NOTES: &[&str] = &["note", "notes", "extra", "comment", "comments"];
const FAV: &[&str] = &["favorite", "favourite"];

/// Parses a CSV export into entries ready to add. Rows without a password are skipped.
pub fn parse_csv(data: &[u8]) -> Result<Vec<EntryInput>> {
    // Strip a UTF-8 BOM (Excel / some Windows exports).
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(data);
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|_| Error::InvalidInput("csv_unreadable"))?
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    let col = |names: &[&str]| names.iter().find_map(|n| headers.iter().position(|h| h == n));
    let (title, url, user, pass, notes, fav) = (col(TITLE), col(URL), col(USER), col(PASS), col(NOTES), col(FAV));
    let pass = pass.ok_or(Error::InvalidInput("csv_no_password_column"))?;

    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|_| Error::InvalidInput("csv_malformed"))?;
        let get = |i: Option<usize>| i.and_then(|i| rec.get(i)).unwrap_or("").to_owned();
        let password = get(Some(pass));
        if password.is_empty() {
            continue;
        }
        let url = get(url);
        let mut title = get(title);
        if title.is_empty() {
            title = host_of(&url).unwrap_or_else(|| "Untitled".into());
        }
        out.push(EntryInput {
            title,
            username: get(user),
            password,
            urls: if url.is_empty() { vec![] } else { vec![url] },
            notes: get(notes),
            favorite: matches!(get(fav).as_str(), "1" | "true" | "yes"),
        });
    }
    Ok(out)
}

pub(crate) fn host_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).or_else(|_| url::Url::parse(&format!("https://{url}"))).ok()?;
    let host = parsed.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_password_manager() {
        let csv = "\u{feff}name,url,username,password,note\n\
                   github.com,https://github.com/login,me,pw1,\n\
                   ,https://www.netflix.com/,me@x.it,pw2,family\n\
                   empty,https://a.com,me,,\n";
        let v = parse_csv(csv.as_bytes()).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title, "github.com");
        assert_eq!(v[1].title, "netflix.com");
        assert_eq!(v[1].notes, "family");
    }

    #[test]
    fn bitwarden() {
        let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                   ,1,login,Bank,,,0,https://bank.it,123,\"p,w\",\n";
        let v = parse_csv(csv.as_bytes()).unwrap();
        assert_eq!(v[0].title, "Bank");
        assert_eq!(v[0].password, "p,w");
        assert!(v[0].favorite);
    }

    #[test]
    fn firefox_and_missing_password_column() {
        let csv = "url,username,password,httpRealm,formActionOrigin,guid\nhttps://x.org,a,b,,,{1}\n";
        assert_eq!(parse_csv(csv.as_bytes()).unwrap()[0].title, "x.org");
        assert!(parse_csv(b"a,b\n1,2\n").is_err());
    }
}
