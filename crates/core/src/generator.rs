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
        return Err(Error::InvalidInput("invalid_length"));
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
        return Err(Error::InvalidInput("no_charset"));
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

/// The EFF large wordlist (7776 words, about 12.9 bits each), bundled.
const WORDS: &str = include_str!("eff_wordlist.txt");

/// A passphrase of `words` random words (4 to 12), joined by `separator`, with
/// one digit added to a random word when `digit` is set: easy to type and to
/// remember, as a master password should be.
pub fn passphrase(words: usize, separator: &str, capitalize: bool, digit: bool) -> Result<String> {
    if !(4..=12).contains(&words) || separator.chars().count() > 3 {
        return Err(Error::InvalidInput("invalid_length"));
    }
    let list: Vec<&str> = WORDS.lines().filter(|w| !w.is_empty()).collect();
    let mut rng = OsRng;
    let mut picked: Vec<String> = (0..words)
        .map(|_| {
            let w = list[rng.gen_range(0..list.len())];
            if capitalize {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                w.to_owned()
            }
        })
        .collect();
    if digit {
        let i = rng.gen_range(0..picked.len());
        picked[i].push(char::from(b'0' + rng.gen_range(0..10u8)));
    }
    Ok(picked.join(separator))
}

/// Entropy of a passphrase made by [`passphrase`].
pub fn passphrase_bits(words: usize, digit: bool) -> f64 {
    let n = WORDS.lines().filter(|w| !w.is_empty()).count() as f64;
    words as f64 * n.log2() + if digit { (10.0 * words as f64).log2() } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrases() {
        let p = passphrase(5, "-", false, false).unwrap();
        assert_eq!(p.split('-').count(), 5);
        assert!(p.split('-').all(|w| WORDS.lines().any(|l| l == w)));
        let p = passphrase(6, " ", true, true).unwrap();
        assert_eq!(p.split(' ').count(), 6);
        assert!(p.chars().any(|c| c.is_ascii_digit()));
        assert!(p.split(' ').all(|w| w.chars().next().unwrap().is_uppercase()));
        assert!(passphrase(3, "-", false, false).is_err());
        assert!((passphrase_bits(5, false) - 64.6).abs() < 0.1);
    }

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
