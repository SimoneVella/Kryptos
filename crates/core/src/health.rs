//! Offline password health check: weak, reused and stale passwords.

use std::collections::HashMap;

use serde::Serialize;
use uuid::Uuid;

use crate::entry::{now, Entry};

const STALE_AFTER_SECS: u64 = 365 * 24 * 3600;

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub total: usize,
    pub weak: Vec<Uuid>,
    pub reused: Vec<Uuid>,
    pub old: Vec<Uuid>,
    /// 0-100: share of entries with no issue.
    pub score: u8,
}

pub fn is_weak(pw: &str) -> bool {
    let len = pw.chars().count();
    let classes = [
        pw.chars().any(|c| c.is_lowercase()),
        pw.chars().any(|c| c.is_uppercase()),
        pw.chars().any(|c| c.is_ascii_digit()),
        pw.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    len < 10 || (len < 16 && classes < 3)
}

pub fn analyze(entries: &[Entry]) -> HealthReport {
    let mut by_pw: HashMap<&str, Vec<Uuid>> = HashMap::new();
    for e in entries.iter().filter(|e| !e.password.is_empty()) {
        by_pw.entry(e.password.as_str()).or_default().push(e.id);
    }
    let reused: Vec<Uuid> = by_pw.into_values().filter(|v| v.len() > 1).flatten().collect();
    let weak: Vec<Uuid> = entries.iter().filter(|e| !e.password.is_empty() && is_weak(&e.password)).map(|e| e.id).collect();
    let cutoff = now().saturating_sub(STALE_AFTER_SECS);
    let old: Vec<Uuid> = entries.iter().filter(|e| e.updated_at < cutoff).map(|e| e.id).collect();

    let flagged = entries
        .iter()
        .filter(|e| weak.contains(&e.id) || reused.contains(&e.id) || old.contains(&e.id))
        .count();
    let score = if entries.is_empty() { 100 } else { (100 * (entries.len() - flagged) / entries.len()) as u8 };
    HealthReport { total: entries.len(), weak, reused, old, score }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(pw: &str, updated_at: u64) -> Entry {
        Entry {
            id: Uuid::new_v4(),
            title: "t".into(),
            username: String::new(),
            password: pw.into(),
            urls: vec![],
            notes: String::new(),
            favorite: false,
            created_at: updated_at,
            updated_at,
        }
    }

    #[test]
    fn flags_weak_reused_old() {
        let t = now();
        let entries = vec![e("password", t), e("Xk9#mP2$vL8@qR4!", t), e("Xk9#mP2$vL8@qR4!", t), e("Tq7!vB3#nM9$wZ5%", 0)];
        let r = analyze(&entries);
        assert_eq!(r.weak, vec![entries[0].id]);
        assert_eq!(r.reused.len(), 2);
        assert_eq!(r.old, vec![entries[3].id]);
        assert_eq!(r.score, 0);
    }

    #[test]
    fn empty_vault_is_perfect() {
        assert_eq!(analyze(&[]).score, 100);
    }
}
