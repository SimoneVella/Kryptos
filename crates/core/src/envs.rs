//! Projects: `.env` files kept in the vault, one per project and environment
//! ("oikos" / "production"). The text is stored as it is, comments and order
//! included, so exporting gives back the file that was imported. A project is
//! just the name its environments share.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longest `.env` accepted. Also keeps a file well under the app socket's 1 MiB.
pub const MAX_LEN: usize = 256 * 1024;

const MAX_NAME: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvFile {
    pub id: Uuid,
    pub project: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub history: Vec<EnvVersion>,
    /// AI assistants may request it (never for production, see [`is_production`]).
    #[serde(default)]
    pub ai_access: bool,
    pub created_at: u64,
    pub updated_at: u64,
    /// Set while in the trash; kept in place so it syncs like any other edit.
    #[serde(default)]
    pub deleted_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVersion {
    pub content: String,
    pub replaced_at: u64,
}

/// An environment as listed: variable names are counted, values never leave.
#[derive(Debug, Clone, Serialize)]
pub struct EnvSummary {
    pub id: Uuid,
    pub project: String,
    pub name: String,
    pub vars: usize,
    pub versions: usize,
    pub ai_access: bool,
    pub production: bool,
    pub updated_at: u64,
    pub deleted_at: u64,
}

impl From<&EnvFile> for EnvSummary {
    fn from(e: &EnvFile) -> Self {
        Self {
            id: e.id,
            project: e.project.clone(),
            name: e.name.clone(),
            vars: parse(&e.content).len(),
            versions: e.history.len(),
            ai_access: e.ai_access,
            production: is_production(&e.name),
            updated_at: e.updated_at,
            deleted_at: e.deleted_at,
        }
    }
}

/// Production secrets are never shared with AI and never skip a confirmation.
pub fn is_production(name: &str) -> bool {
    let n = name.to_lowercase();
    n.split(|c: char| !c.is_alphanumeric()).any(|w| w == "live" || w == "main" || w == "prd" || w.starts_with("prod"))
}

/// The trimmed name, if it's fit for a project or environment.
pub fn clean_name(name: &str) -> Option<String> {
    let n = name.trim();
    (!n.is_empty() && n.chars().count() <= MAX_NAME && !n.chars().any(char::is_control)).then(|| n.to_owned())
}

/// The variables of a `.env` file, in order; a later duplicate wins, as in dotenv.
/// Reads `export KEY=v`, single and double quotes (double ones with `\n`-style
/// escapes, over several lines if needed) and ` # comments` after a bare value.
pub fn parse(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim();
        if !is_key(key) {
            continue;
        }
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let mut raw = rest[1..].to_owned();
                while closing(&raw, q).is_none() {
                    let Some(next) = lines.next() else { break };
                    raw.push('\n');
                    raw.push_str(next);
                }
                let body = closing(&raw, q).map_or(raw.as_str(), |i| &raw[..i]);
                if q == '"' { unescape(body) } else { body.to_owned() }
            }
            _ => rest.split(" #").next().unwrap_or_default().trim_end().to_owned(),
        };
        match out.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => out.push((key.to_owned(), value)),
        }
    }
    out
}

fn is_key(key: &str) -> bool {
    !key.is_empty() && !key.starts_with(|c: char| c.is_ascii_digit()) && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// Byte index of the quote that closes `s`, skipping `\"` inside double quotes.
fn closing(s: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' && quote == '"' {
            escaped = true;
        } else if c == quote {
            return Some(i);
        }
    }
    None
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// A `.env.example`: same comments and order, every value emptied. Safe for git.
pub fn example(text: &str) -> String {
    let mut out = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let bare = trimmed.strip_prefix("export ").map_or(trimmed, str::trim_start);
        match bare.split_once('=').filter(|(k, _)| !trimmed.starts_with('#') && is_key(k.trim())) {
            Some((key, rest)) => {
                // Skip the rest of a value that spans several lines.
                let rest = rest.trim_start();
                if let Some(q @ ('"' | '\'')) = rest.chars().next() {
                    let mut raw = rest[1..].to_owned();
                    while closing(&raw, q).is_none() {
                        let Some(next) = lines.next() else { break };
                        raw.push('\n');
                        raw.push_str(next);
                    }
                }
                out.push_str(key.trim());
                out.push('=');
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// How two environments differ. Variable names only: values stay private, and
/// `different` just says that a value isn't the same.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct EnvDiff {
    pub only_a: Vec<String>,
    pub only_b: Vec<String>,
    pub different: Vec<String>,
}

pub fn diff(a: &str, b: &str) -> EnvDiff {
    let (a, b) = (parse(a), parse(b));
    let theirs: HashMap<&str, &str> = b.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let ours: HashSet<&str> = a.iter().map(|(k, _)| k.as_str()).collect();
    let mut d = EnvDiff::default();
    for (k, v) in &a {
        match theirs.get(k.as_str()) {
            None => d.only_a.push(k.clone()),
            Some(o) if o != v => d.different.push(k.clone()),
            Some(_) => {}
        }
    }
    d.only_b = b.iter().filter(|(k, _)| !ours.contains(k.as_str())).map(|(k, _)| k.clone()).collect();
    d
}

/// Whether the `.gitignore` of `dir` (or of a parent, up to the repository root)
/// ignores `file`. Understands the usual forms, not every gitignore pattern.
pub fn is_gitignored(dir: &Path, file: &str) -> bool {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if let Ok(text) = std::fs::read_to_string(d.join(".gitignore")) {
            let ignored = text.lines().map(str::trim).any(|l| {
                let l = l.trim_start_matches('/');
                l == file || l == ".env*" || l == ".env.*" && file != ".env" || l == "*.env" && file.ends_with(".env")
            });
            if ignored {
                return true;
            }
        }
        if d.join(".git").exists() {
            return false;
        }
        cur = d.parent();
    }
    false
}

/// A `.env` found on disk. The caller decides whether to import it.
#[derive(Debug, Clone, Serialize)]
pub struct FoundEnv {
    pub path: PathBuf,
    pub project: String,
    pub name: String,
    pub vars: usize,
}

const SKIP_DIRS: &[&str] = &["node_modules", "target", "vendor", "dist", "build", "venv", "__pycache__", "Library", "Applications", "Pictures", "Movies", "Music"];
const MAX_DEPTH: usize = 6;
const MAX_FOUND: usize = 500;

/// Whether `file` is a real `.env` (not a template, which holds no secrets).
pub fn is_env_file(file: &str) -> bool {
    (file == ".env" || file.starts_with(".env.")) && !file.ends_with(".example") && !file.ends_with(".sample") && !file.ends_with(".template")
}

/// The environment a file stands for: `.env` -> development, `.env.staging` -> staging.
pub fn env_name(file: &str) -> String {
    file.strip_prefix(".env.").filter(|s| !s.is_empty()).unwrap_or("development").to_owned()
}

/// Looks for `.env` files under `root`, skipping hidden and dependency folders.
pub fn scan(root: &Path) -> Vec<FoundEnv> {
    let mut found = Vec::new();
    walk(root, 0, &mut found);
    found.sort_by(|a, b| (&a.project, &a.name).cmp(&(&b.project, &b.name)));
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<FoundEnv>) {
    let Ok(items) = std::fs::read_dir(dir) else { return };
    for item in items.flatten() {
        if found.len() >= MAX_FOUND {
            return;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        // file_type() doesn't follow symlinks, so links can't cause loops.
        let Ok(kind) = item.file_type() else { continue };
        if kind.is_dir() {
            if depth < MAX_DEPTH && !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                walk(&item.path(), depth + 1, found);
            }
        } else if kind.is_file() && is_env_file(&name) {
            let Ok(meta) = item.metadata() else { continue };
            if meta.len() as usize > MAX_LEN {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(item.path()) else { continue };
            let project = dir.file_name().map_or_else(|| "project".to_owned(), |n| n.to_string_lossy().into_owned());
            found.push(FoundEnv { path: item.path(), project, name: env_name(&name), vars: parse(&text).len() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# database\nDB_URL=postgres://u:p@h/db\nexport API_KEY = \"a\\nb\"\nNAME='it''s'\nPORT=3000 # dev only\nPEM=\"line1\nline2\"\n\nnot a variable\n1BAD=x\n";

    #[test]
    fn parses_the_usual_forms() {
        let vars = parse(SAMPLE);
        let get = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("DB_URL"), Some("postgres://u:p@h/db"));
        assert_eq!(get("API_KEY"), Some("a\nb"));
        assert_eq!(get("PORT"), Some("3000"));
        assert_eq!(get("PEM"), Some("line1\nline2"));
        assert_eq!(get("1BAD"), None);
        assert_eq!(vars.len(), 5);
    }

    #[test]
    fn later_duplicates_win_and_hashes_inside_values_stay() {
        assert_eq!(parse("A=1\nA=2\nB=x#y"), vec![("A".to_owned(), "2".to_owned()), ("B".to_owned(), "x#y".to_owned())]);
    }

    #[test]
    fn example_keeps_comments_and_drops_every_value() {
        let e = example(SAMPLE);
        assert!(e.contains("# database\nDB_URL=\nAPI_KEY=\n"));
        assert!(e.contains("PEM=\n"));
        assert!(!e.contains("postgres") && !e.contains("line2") && !e.contains("3000"));
    }

    #[test]
    fn diff_names_keys_only() {
        let d = diff("A=1\nB=2\nC=3", "B=2\nC=9\nD=4");
        assert_eq!(d.only_a, ["A"]);
        assert_eq!(d.only_b, ["D"]);
        assert_eq!(d.different, ["C"]);
    }

    #[test]
    fn production_names() {
        assert!(is_production("production") && is_production(" Prod ") && is_production("live"));
        assert!(is_production("eu-prod") && is_production("prd") && is_production("us_production") && is_production("Main.2"));
        assert!(!is_production("development") && !is_production("staging") && !is_production("reproduce"));
    }

    #[test]
    fn scan_finds_env_files_and_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("oikos");
        std::fs::create_dir_all(app.join("node_modules/x")).unwrap();
        std::fs::write(app.join(".env"), "A=1\nB=2").unwrap();
        std::fs::write(app.join(".env.staging"), "A=1").unwrap();
        std::fs::write(app.join(".env.example"), "A=").unwrap();
        std::fs::write(app.join("node_modules/x/.env"), "NO=1").unwrap();
        let found = scan(dir.path());
        assert_eq!(found.iter().map(|f| (f.project.as_str(), f.name.as_str(), f.vars)).collect::<Vec<_>>(), [("oikos", "development", 2), ("oikos", "staging", 1)]);
    }

    #[test]
    fn gitignore_check() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(!is_gitignored(dir.path(), ".env"));
        std::fs::write(dir.path().join(".gitignore"), "node_modules\n.env\n").unwrap();
        assert!(is_gitignored(dir.path(), ".env"));
        assert!(!is_gitignored(dir.path(), ".env.local"));
    }
}
