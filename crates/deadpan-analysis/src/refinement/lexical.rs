use super::{LABELS, MAX_DATA_BYTES, MAX_PHONES, MAX_WORDS, RefinementError};
use serde::Serialize;
use std::collections::BTreeMap;

#[path = "unicode.rs"]
mod unicode;

/// Raw CMUDict data provenance, in original file order. Stress is deliberately
/// removed. No generated pronunciation or spelling repair is permitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pronunciation {
    pub labels: Vec<u8>,
    pub dictionary_line: u32,
    pub dictionary_spelling: String,
}

/// Parsed, bounded raw data; callers own pinning the bytes before parsing.
#[derive(Debug, Clone)]
pub struct PronunciationLexicon {
    entries: BTreeMap<String, Vec<Pronunciation>>,
}

fn whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn fold(value: &str) -> String {
    let mut result = String::new();
    for c in value.chars() {
        if let Ok(i) = unicode::CASEFOLD.binary_search_by_key(&u32::from(c), |(key, _)| *key) {
            result.push_str(unicode::CASEFOLD[i].1);
        } else {
            result.push(c);
        }
    }
    result
}

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let value = u32::from(c);
    let i = ranges.partition_point(|(start, _)| *start <= value);
    i > 0 && value <= ranges[i - 1].1
}

/// Frozen Python-compatible casefold, alphabetic filtering and whitespace.
/// Digits and punctuation disappear inside tokens; they never split a token.
pub fn normalize_words(text: &str) -> Result<Vec<String>, RefinementError> {
    if text.len() > 16_384 {
        return Err(RefinementError::Limit("text bytes"));
    }
    let mut words = Vec::new();
    for token in text.split(whitespace).filter(|s| !s.is_empty()) {
        if words.len() >= MAX_WORDS {
            return Err(RefinementError::Limit("words"));
        }
        let word: String = fold(token)
            .chars()
            .map(|c| if c == '’' { '\'' } else { c })
            .filter(|c| *c == '\'' || in_ranges(*c, unicode::ALPHA))
            .collect();
        if word.is_empty() {
            return Err(RefinementError::Invalid("empty lexical word"));
        }
        if word.len() > 256 {
            return Err(RefinementError::Limit("word bytes"));
        }
        words.push(word);
    }
    if words.is_empty() {
        return Err(RefinementError::Invalid("empty transcript"));
    }
    Ok(words)
}

impl PronunciationLexicon {
    pub fn parse(dictionary: &str, phones: &str) -> Result<Self, RefinementError> {
        if dictionary
            .len()
            .checked_add(phones.len())
            .is_none_or(|n| n > MAX_DATA_BYTES)
        {
            return Err(RefinementError::Limit("dictionary data bytes"));
        }
        let parsed: Vec<_> = phones
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        if parsed != LABELS[1..40] {
            return Err(RefinementError::Invalid("phone label order"));
        }
        let mut entries: BTreeMap<String, Vec<Pronunciation>> = BTreeMap::new();
        for (number, line) in dictionary.lines().enumerate() {
            if line.is_empty() || line.starts_with(";;;") {
                continue;
            }
            if line.len() > 16_384 {
                return Err(RefinementError::Limit("dictionary line bytes"));
            }
            let mut parts = line.split(whitespace).filter(|s| !s.is_empty());
            let Some(spelling) = parts.next() else {
                continue;
            };
            if spelling.len() > 256 {
                return Err(RefinementError::Limit("dictionary spelling bytes"));
            }
            let mut stem = spelling;
            if let Some((prefix, suffix)) = spelling.rsplit_once('(')
                && suffix.strip_suffix(')').is_some_and(|s| {
                    !s.is_empty() && s.chars().all(|c| in_ranges(c, unicode::DECIMAL))
                })
            {
                stem = prefix;
            }
            let word = fold(stem);
            let mut labels = Vec::new();
            let mut valid = true;
            for part in parts {
                if part.starts_with('#') {
                    continue;
                }
                let phone: String = part
                    .chars()
                    .filter(|c| !in_ranges(*c, unicode::DECIMAL))
                    .collect();
                let Some(id) = LABELS[1..40].iter().position(|p| *p == phone) else {
                    valid = false;
                    break;
                };
                if labels.len() >= MAX_PHONES {
                    return Err(RefinementError::Limit("pronunciation phones"));
                }
                labels.push((id + 1) as u8);
            }
            if !valid || labels.is_empty() {
                continue;
            }
            let alternatives = entries.entry(word).or_default();
            if alternatives.iter().any(|p| p.labels == labels) {
                continue;
            }
            if alternatives.len() >= 32 {
                return Err(RefinementError::Limit("word pronunciation alternatives"));
            }
            alternatives.push(Pronunciation {
                labels,
                dictionary_line: (number + 1) as u32,
                dictionary_spelling: spelling.to_owned(),
            });
        }
        if entries.is_empty() {
            return Err(RefinementError::Invalid("empty pronunciation dictionary"));
        }
        Ok(Self { entries })
    }

    pub(super) fn lookup(
        &self,
        words: &[String],
    ) -> Result<Vec<&[Pronunciation]>, RefinementError> {
        let mut result = Vec::with_capacity(words.len());
        let mut maximum = 0usize;
        for word in words {
            let alternatives = self
                .entries
                .get(word)
                .ok_or_else(|| RefinementError::UnsupportedWord(word.clone()))?;
            maximum = maximum
                .checked_add(
                    alternatives
                        .iter()
                        .map(|p| p.labels.len())
                        .max()
                        .unwrap_or(0),
                )
                .ok_or(RefinementError::Limit("phone path"))?;
            if maximum > MAX_PHONES {
                return Err(RefinementError::Limit("phone path"));
            }
            result.push(alternatives.as_slice());
        }
        Ok(result)
    }
}
