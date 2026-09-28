use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>/?~";
const AMBIGUOUS: &str = "Il1O0o|`'\"";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratorOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self { length: 20, lowercase: true, uppercase: true, digits: true, symbols: true, exclude_ambiguous: true }
    }
}

/// Generates a password with at least one character from every enabled class,
/// using the OS CSPRNG and unbiased sampling.
pub fn generate(opts: &GeneratorOptions) -> Result<String> {
    if !(8..=256).contains(&opts.length) {
        return Err(Error::InvalidInput("length must be between 8 and 256"));
    }
    let classes: Vec<Vec<char>> = [
        (opts.lowercase, LOWER),
        (opts.uppercase, UPPER),
        (opts.digits, DIGITS),
        (opts.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, set)| set.chars().filter(|c| !(opts.exclude_ambiguous && AMBIGUOUS.contains(*c))).collect())
    .collect();
    if classes.is_empty() {
        return Err(Error::InvalidInput("select at least one character set"));
    }
    let all: Vec<char> = classes.iter().flatten().copied().collect();
    let mut rng = OsRng;
    let mut out: Vec<char> = classes.iter().map(|c| c[rng.gen_range(0..c.len())]).collect();
    while out.len() < opts.length {
        out.push(all[rng.gen_range(0..all.len())]);
    }
    out.shuffle(&mut rng);
    Ok(out.into_iter().collect())
}

/// Rough entropy estimate in bits for a generated password with these options.
pub fn entropy_bits(opts: &GeneratorOptions) -> f64 {
    let pool: usize = [(opts.lowercase, LOWER), (opts.uppercase, UPPER), (opts.digits, DIGITS), (opts.symbols, SYMBOLS)]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, s)| s.chars().filter(|c| !(opts.exclude_ambiguous && AMBIGUOUS.contains(*c))).count())
        .sum();
    if pool == 0 { 0.0 } else { opts.length as f64 * (pool as f64).log2() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_length_and_classes() {
        let o = GeneratorOptions::default();
        for _ in 0..200 {
            let p = generate(&o).unwrap();
            assert_eq!(p.chars().count(), 20);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
            assert!(!p.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn digits_only() {
        let o = GeneratorOptions { lowercase: false, uppercase: false, symbols: false, ..Default::default() };
        assert!(generate(&o).unwrap().chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn rejects_empty_charset() {
        let o = GeneratorOptions { lowercase: false, uppercase: false, digits: false, symbols: false, ..Default::default() };
        assert!(generate(&o).is_err());
    }
}
