//! Files that carry 2FA data from elsewhere:
//! - exports of authenticator apps: Aegis and 2FAS (unencrypted JSON), Google
//!   Authenticator's `otpauth-migration://` transfer links, and any text with
//!   `otpauth://totp/...` links (andOTP, Raivo, Ente Auth, Bitwarden...);
//! - the backup-code files sites let you download when you turn 2FA on.

use base64::Engine;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::totp::{base32_encode, Totp};

/// One TOTP account found in an export, as an `otpauth://` link Kryptos stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpAccount {
    pub issuer: String,
    pub account: String,
    pub uri: String,
}

/// Every TOTP account in an authenticator export. HOTP (counter based) accounts
/// are left out. Encrypted exports are refused with "otp_export_encrypted":
/// export them again without a password.
pub fn parse_export(data: &[u8]) -> Result<Vec<OtpAccount>> {
    let text = std::str::from_utf8(data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)).map_err(|_| Error::InvalidInput("otp_export_unknown"))?;
    let mut out = match serde_json::from_str::<Value>(text) {
        Ok(json) => from_json(&json)?,
        Err(_) => from_links(text),
    };
    out.retain(|a| Totp::parse(&a.uri).is_ok());
    out.dedup_by(|a, b| a.uri == b.uri);
    if out.is_empty() {
        return Err(Error::InvalidInput("otp_export_unknown"));
    }
    Ok(out)
}

fn from_json(v: &Value) -> Result<Vec<OtpAccount>> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_owned();
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64);
    // Aegis: {"db": {"entries": [...]}}; an encrypted vault has "db" as a string.
    if let Some(db) = v.get("db") {
        let entries = db.get("entries").and_then(Value::as_array).ok_or(Error::InvalidInput("otp_export_encrypted"))?;
        return Ok(entries
            .iter()
            .filter(|e| s(e, "type").eq_ignore_ascii_case("totp"))
            .filter_map(|e| {
                let info = e.get("info")?;
                Some(account(&s(e, "issuer"), &s(e, "name"), &s(info, "secret"), &s(info, "algo"), n(info, "digits"), n(info, "period")))
            })
            .collect());
    }
    // 2FAS: {"services": [...]}; encrypted backups carry "servicesEncrypted" instead.
    if v.get("servicesEncrypted").is_some_and(|e| !e.is_null()) {
        return Err(Error::InvalidInput("otp_export_encrypted"));
    }
    if let Some(services) = v.get("services").and_then(Value::as_array) {
        return Ok(services
            .iter()
            .filter_map(|e| {
                let otp = e.get("otp").cloned().unwrap_or(Value::Null);
                let kind = s(&otp, "tokenType");
                if !kind.is_empty() && !kind.eq_ignore_ascii_case("totp") {
                    return None;
                }
                let issuer = Some(s(&otp, "issuer")).filter(|i| !i.is_empty()).unwrap_or_else(|| s(e, "name"));
                Some(account(&issuer, &s(&otp, "account"), &s(e, "secret"), &s(&otp, "algorithm"), n(&otp, "digits"), n(&otp, "period")))
            })
            .collect());
    }
    // Any other JSON: look for otpauth links anywhere in it.
    Ok(from_links(&v.to_string()))
}

fn account(issuer: &str, name: &str, secret: &str, algo: &str, digits: Option<u64>, period: Option<u64>) -> OtpAccount {
    let secret: String = secret.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    let mut uri = format!("otpauth://totp/{}?secret={secret}", enc(&label(issuer, name)));
    if !issuer.is_empty() {
        uri += &format!("&issuer={}", enc(issuer));
    }
    if !algo.is_empty() && !algo.eq_ignore_ascii_case("sha1") {
        uri += &format!("&algorithm={}", algo.to_ascii_uppercase());
    }
    if let Some(d) = digits.filter(|&d| d != 6) {
        uri += &format!("&digits={d}");
    }
    if let Some(p) = period.filter(|&p| p != 30) {
        uri += &format!("&period={p}");
    }
    OtpAccount { issuer: issuer.to_owned(), account: name.to_owned(), uri }
}

fn label(issuer: &str, name: &str) -> String {
    match (issuer.is_empty(), name.is_empty()) {
        (false, false) => format!("{issuer}:{name}"),
        (false, true) => issuer.to_owned(),
        _ => name.to_owned(),
    }
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>().replace('+', "%20")
}

/// `otpauth://totp/...` and `otpauth-migration://...` links anywhere in `text`.
fn from_links(text: &str) -> Vec<OtpAccount> {
    let mut out = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ',' | ';')) {
        let lower = word.to_ascii_lowercase();
        if lower.starts_with("otpauth-migration://") {
            out.extend(migration(word));
        } else if lower.starts_with("otpauth://totp/") {
            if let Ok(url) = url::Url::parse(word) {
                let label = url.path().trim_start_matches('/');
                let label = url::form_urlencoded::parse(format!("l={label}").as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_default();
                let issuer_param = url.query_pairs().find(|(k, _)| k.eq_ignore_ascii_case("issuer")).map(|(_, v)| v.into_owned());
                let (issuer, account) = match label.split_once(':') {
                    Some((i, a)) => (issuer_param.unwrap_or_else(|| i.trim().to_owned()), a.trim().to_owned()),
                    None => (issuer_param.unwrap_or_default(), label.trim().to_owned()),
                };
                out.push(OtpAccount { issuer, account, uri: word.to_owned() });
            }
        }
    }
    out
}

/// Google Authenticator's "Transfer accounts" QR: `otpauth-migration://offline?data=<base64 protobuf>`.
fn migration(link: &str) -> Vec<OtpAccount> {
    let Ok(url) = url::Url::parse(link) else { return vec![] };
    let Some(data) = url.query_pairs().find(|(k, _)| k == "data").map(|(_, v)| v.into_owned()) else { return vec![] };
    let b64 = base64::engine::general_purpose::STANDARD;
    let Ok(bytes) = b64.decode(data.trim()) else { return vec![] };
    let mut out = Vec::new();
    for (field, value) in Proto(&bytes) {
        let (1, Wire::Bytes(params)) = (field, value) else { continue };
        let (mut secret, mut name, mut issuer, mut algo, mut digits, mut totp) = (Vec::new(), String::new(), String::new(), "", None, true);
        for (f, v) in Proto(params) {
            match (f, v) {
                (1, Wire::Bytes(b)) => secret = b.to_vec(),
                (2, Wire::Bytes(b)) => name = String::from_utf8_lossy(b).into_owned(),
                (3, Wire::Bytes(b)) => issuer = String::from_utf8_lossy(b).into_owned(),
                (4, Wire::Varint(a)) => algo = match a { 2 => "SHA256", 3 => "SHA512", 4 => "MD5", _ => "" },
                (5, Wire::Varint(d)) => digits = (d == 2).then_some(8),
                (6, Wire::Varint(t)) => totp = t != 1, // 1 = HOTP
                _ => {}
            }
        }
        if !totp || secret.is_empty() || algo == "MD5" {
            continue;
        }
        // Names often come as "Issuer:account".
        let account_name = name.split_once(':').filter(|(i, _)| i.trim() == issuer).map_or(name.as_str(), |(_, a)| a).trim().to_owned();
        out.push(account(&issuer, &account_name, &base32_encode(&secret), algo, digits, None));
    }
    out
}

enum Wire<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

/// Just enough protobuf to walk (field, value) pairs; stops at anything unexpected.
struct Proto<'a>(&'a [u8]);

impl<'a> Proto<'a> {
    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let (&b, rest) = self.0.split_first()?;
            self.0 = rest;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
}

impl<'a> Iterator for Proto<'a> {
    type Item = (u64, Wire<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        let key = self.varint()?;
        let value = match key & 7 {
            0 => Wire::Varint(self.varint()?),
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                if len > self.0.len() {
                    return None;
                }
                let (b, rest) = self.0.split_at(len);
                self.0 = rest;
                Wire::Bytes(b)
            }
            _ => return None,
        };
        Some((key >> 3, value))
    }
}

/// The codes in a site's backup-code file (or pasted text): one or more per
/// line, optionally numbered ("1. 1234 5678"). Sentences around them are skipped.
pub fn parse_backup_codes(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        // Several codes on one line are separated by tabs or runs of spaces.
        for chunk in line.split(['\t', ',', ';']).flat_map(|c| c.split("  ")) {
            let mut c = chunk.trim();
            // Leading numbering: "1.", "1)", "#1", "- ".
            c = c.trim_start_matches(['-', '*', '•', '#']).trim_start();
            if let Some(i) = c.find(['.', ')']).filter(|&i| i > 0 && i <= 3 && c[..i].chars().all(|d| d.is_ascii_digit())) {
                c = c[i + 1..].trim_start();
            }
            if is_code(c) && !out.iter().any(|o| o == c) {
                out.push(c.to_owned());
            }
        }
    }
    out
}

/// "a1b2c-3d4e5", "1234 5678", "ABCD-EFGH-1234", "48291037": 6 to 24 letters and
/// digits in at most four groups, with at least one digit.
fn is_code(s: &str) -> bool {
    let groups: Vec<&str> = s.split([' ', '-']).collect();
    let len: usize = groups.iter().map(|g| g.len()).sum();
    (6..=24).contains(&len)
        && groups.len() <= 4
        && groups.iter().all(|g| g.len() >= 3 && g.chars().all(|c| c.is_ascii_alphanumeric()))
        && s.chars().any(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "JBSWY3DPEHPK3PXP";

    #[test]
    fn aegis_plain_export() {
        let json = format!(
            r#"{{"version":1,"header":{{"slots":null,"params":null}},"db":{{"version":2,"entries":[
              {{"type":"totp","name":"me@example.com","issuer":"GitHub","info":{{"secret":"{SECRET}","algo":"SHA1","digits":6,"period":30}}}},
              {{"type":"hotp","name":"x","issuer":"Old","info":{{"secret":"{SECRET}","algo":"SHA1","digits":6,"counter":1}}}},
              {{"type":"totp","name":"","issuer":"Bank","info":{{"secret":"{SECRET}","algo":"SHA256","digits":8,"period":60}}}}]}}}}"#
        );
        let a = parse_export(json.as_bytes()).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].issuer.as_str(), a[0].account.as_str()), ("GitHub", "me@example.com"));
        let bank = Totp::parse(&a[1].uri).unwrap();
        assert_eq!((bank.digits, bank.period), (8, 60));
    }

    #[test]
    fn encrypted_exports_refused() {
        let aegis = r#"{"version":1,"header":{"slots":[{}]},"db":"c2VjcmV0"}"#;
        let twofas = r#"{"services":[],"servicesEncrypted":"abc","schemaVersion":4}"#;
        for e in [aegis, twofas] {
            assert!(matches!(parse_export(e.as_bytes()), Err(Error::InvalidInput("otp_export_encrypted"))));
        }
    }

    #[test]
    fn twofas_export() {
        let json = format!(
            r#"{{"services":[{{"name":"Google","secret":"{SECRET}","otp":{{"account":"me@gmail.com","issuer":"Google","digits":6,"period":30,"algorithm":"SHA1","tokenType":"TOTP"}}}}],"schemaVersion":4}}"#
        );
        let a = parse_export(json.as_bytes()).unwrap();
        assert_eq!((a[0].issuer.as_str(), a[0].account.as_str()), ("Google", "me@gmail.com"));
    }

    #[test]
    fn links_in_text() {
        let text = format!("otpauth://totp/Dropbox:me%40x.com?secret={SECRET}&issuer=Dropbox\nnot a link\notpauth://hotp/x?secret={SECRET}&counter=1\n");
        let a = parse_export(text.as_bytes()).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].issuer.as_str(), a[0].account.as_str()), ("Dropbox", "me@x.com"));
    }

    #[test]
    fn google_migration_link() {
        // One TOTP account: secret "Hello!" (JBSWY3DPEE======), name "me@x.com", issuer "Acme".
        let mut params = vec![0x0a, 6];
        params.extend(b"Hello!");
        params.extend([0x12, 8]);
        params.extend(b"me@x.com");
        params.extend([0x1a, 4]);
        params.extend(b"Acme");
        params.extend([0x20, 1, 0x28, 1, 0x30, 2]);
        let mut payload = vec![0x0a, params.len() as u8];
        payload.extend(params);
        let data = base64::engine::general_purpose::STANDARD.encode(payload);
        let link = format!("otpauth-migration://offline?data={}", enc(&data));
        let a = parse_export(link.as_bytes()).unwrap();
        assert_eq!((a[0].issuer.as_str(), a[0].account.as_str()), ("Acme", "me@x.com"));
        assert_eq!(Totp::parse(&a[0].uri).unwrap().code_at(59), Totp::parse("JBSWY3DPEE").unwrap().code_at(59));
    }

    #[test]
    fn unknown_files_refused() {
        assert!(parse_export(b"name,password\nx,y").is_err());
        assert!(parse_export(br#"{"hello":"world"}"#).is_err());
    }

    #[test]
    fn backup_code_files() {
        let github = "a1b2c-3d4e5\nf6g7h-8i9j0\n\nk1l2m-3n4o5\n";
        assert_eq!(parse_backup_codes(github), ["a1b2c-3d4e5", "f6g7h-8i9j0", "k1l2m-3n4o5"]);
        let google = "BACKUP VERIFICATION CODES\nme@gmail.com\n\n1. 1234 5678\t2. 2345 6789\n3. 3456 7890\n\n(Each code can be used once.)\nCreated on Oct 29, 2026\n";
        assert_eq!(parse_backup_codes(google), ["1234 5678", "2345 6789", "3456 7890"]);
        assert!(parse_backup_codes("Keep these codes somewhere safe.\nme@example.com").is_empty());
    }
}
