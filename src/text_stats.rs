use std::str::FromStr;
use std::iter::FromIterator;
use std::collections::HashMap;
use std::ops::Index;
use std::cmp::max;
use serde::ser; // Can't use serde::ser::Serialize directly because I need the serde::Serialize macro
use serde::ser::{Serializer, SerializeMap};
use serde::Serialize;

type Symbol = [char; 1];
type Bigram = [char; 2];
type Trigram = [char; 3];

#[derive(Debug)]
struct NGramStats<T> {
    map: HashMap<T, (usize, usize)>, // n-Gram counters+tokens in a hashmap
    list: Vec<(T, usize, usize)>,    // n-Gram list sorted by descending count
    total: usize,                    // Sum of all n-Grams counts
}

impl<T: Copy> NGramStats<T> {
    fn from_map(map: HashMap<T, (usize, usize)>) -> Self {
        let mut total = 0usize; // Gets updated by the closure below

        // Construct list of all n-grams, calculate sum of all counts
        let mut list: Vec<(T, usize, usize)> =
            map.iter().map(|(&ngram, &(count, token))| {
                total += count;
                (ngram, count, token)
            }).collect();
        // Sort by count, highest first
        list.sort_by_key(|(_, count, _)| usize::MAX - count);

        Self {map, list, total}
    }

    fn iter(&self) -> std::slice::Iter<(T, usize, usize)> {
        self.list.iter()
    }
}

impl<T: Copy + IntoIterator<Item = char>> ser::Serialize for NGramStats<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.list.len()))?;
        for (k, v, _) in self.iter() {
            let k = String::from_iter(*k);
            map.serialize_entry(&k, v)?;
        }
        map.end()
    }
}

#[derive(Debug, Serialize)]
pub struct TextStats {
    #[serde(rename = "symbols")]
    s: NGramStats<Symbol>,
    #[serde(rename = "bigrams")]
    b: NGramStats<Bigram>,
    #[serde(rename = "trigrams")]
    t: NGramStats<Trigram>,
    #[serde(skip)]
    token_base: usize,
    #[serde(skip)]
    token_map: Vec<usize>,
}

impl FromStr for TextStats {
    type Err = ();

    fn from_str(text: &str) -> Result <Self, Self::Err> {
        let len = text.chars().count();
        let mut i = 0usize;
        let mut bigram = [' '; 2];
        let mut trigram = [' '; 3];
        let mut s_map = HashMap::new();
        let mut b_map = HashMap::new();
        let mut t_map = HashMap::new();
        let mut token = 0usize;

        // Build maps of symbols, bigrams and 3-grams of lower-case
        // characters in the text. Assign tokens to characters.
        for c in text.chars() {
            i += 1;
            if i % 1000000 == 0 {
                println!("Processing text ngrams: {}",
                         i as f64 / len as f64 * 100.0);
            }

            for c in c.to_lowercase() {
                let symbol = [c];
                trigram[0..2].copy_from_slice(&bigram[..]);
                trigram[2] = c;
                bigram[0..2].copy_from_slice(&trigram[1..3]);

                let (count, _) = s_map.entry(symbol).or_insert_with(
                    || {token += 1; (0, token)});
                *count += 1;
                if !(bigram[0].is_whitespace() || bigram[1].is_whitespace()) {
                    let (count, _) = b_map.entry(bigram).or_insert((0, 0));
                    *count += 1;
                    if !trigram[0].is_whitespace() {
                        let (count, _) = t_map.entry(trigram).or_insert((0, 0));
                        *count += 1;
                    }
                }
            }
        }

        let token_base = token + 1;
        let mut max_token = token;

        // Derive token values for bigrams and 3-grams
        for (&bigram, (_, token)) in b_map.iter_mut() {
            let t0 = s_map[&[bigram[0]]].1;
            let t1 = s_map[&[bigram[1]]].1;
            *token = t1 * token_base + t0;
            max_token = max(max_token, *token);
        }
        for (&trigram, (_, token)) in t_map.iter_mut() {
            let t0 = s_map[&[trigram[0]]].1;
            let t1 = s_map[&[trigram[1]]].1;
            let t2 = s_map[&[trigram[2]]].1;
            *token = (t2 * token_base + t1) * token_base + t0;
            max_token = max(max_token, *token);
        }

        // Fill token map
        let mut token_map: Vec<usize> = Vec::new();
        token_map.resize(max_token+1, 0);
        for &(count, token) in
                s_map.values().chain(b_map.values()).chain(t_map.values()) {
            token_map[token] = count;
        }

        Ok(TextStats {
            s: NGramStats::from_map(s_map),
            b: NGramStats::from_map(b_map),
            t: NGramStats::from_map(t_map),
            token_base,
            token_map,
        })
    }
}

impl Index<Symbol> for TextStats {
    type Output = (usize, usize);

    fn index(&self, index: Symbol) -> &(usize, usize) {
        self.s.map.index(&index)
    }
}

impl Index<Bigram> for TextStats {
    type Output = (usize, usize);

    fn index(&self, index: Bigram) -> &(usize, usize) {
        self.b.map.index(&index)
    }
}

impl Index<Trigram> for TextStats {
    type Output = (usize, usize);

    fn index(&self, index: Trigram) -> &(usize, usize) {
        self.t.map.index(&index)
    }
}

impl Index<usize> for TextStats {
    type Output = usize;

    fn index(&self, index: usize) -> &usize {
        self.token_map.index(index)
    }
}

impl TextStats {
    pub fn iter_symbols(&self)
        -> std::slice::Iter<(Symbol, usize, usize)> {self.s.iter()}
    pub fn iter_bigrams(&self)
        -> std::slice::Iter<(Bigram, usize, usize)> {self.b.iter()}
    pub fn iter_trigrams(&self)
        -> std::slice::Iter<(Trigram, usize, usize)> {self.t.iter()}

    pub fn get_symbol(&self, index: Symbol) -> Option<&(usize, usize)> {
        self.s.map.get(&index)
    }
    pub fn get_bigram(&self, index: Bigram) -> Option<&(usize, usize)> {
        self.b.map.get(&index)
    }
    pub fn get_trigram(&self, index: Trigram) -> Option<&(usize, usize)> {
        self.t.map.get(&index)
    }

    pub fn token_to_ngram(&self, mut token: usize) -> [usize; 3] {
        let mut ngram = [0; 3];

        ngram[0] = token % self.token_base;
        token /= self.token_base;

        ngram[1] = token % self.token_base;
        token /= self.token_base;

        ngram[2] = token;
        assert!(token < self.token_base);

        ngram
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    static TEST_STRING : &str = "Hello, world! Be well.";

    // Check that all symbols in the iterator are in the original text and sorted by count
    #[test]
    fn symbol_iter() {
        let lower = TEST_STRING.to_lowercase();
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        let mut prev = usize::MAX;
        for (symbol, counter, token) in stats.iter_symbols() {
            println!("  '{}': {} #{}", symbol[0], counter, token);
            assert!(lower.contains(symbol[0]));
            assert_ne!(counter.cmp(&prev), Ordering::Greater);
            prev = *counter;

            // also check the symbol counts, while we're at it
            let matching = lower.chars().filter(|c| *c == symbol[0]).count();
            assert_eq!(matching, *counter);
        }
    }

    // Check that all bigrams in the iterator are in the original text and sorted by count
    #[test]
    fn bigram_iter() {
        let lower = TEST_STRING.to_lowercase();
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        let mut prev = usize::MAX;
        for (bigram, counter, token) in stats.iter_bigrams() {
            println!("  '{}{}': {} #{}", bigram[0], bigram[1], counter, token);
            assert!(lower.contains(&bigram[..]));
            assert_ne!(counter.cmp(&prev), Ordering::Greater);
            prev = *counter;
        }
    }

    // Check that all trigrams in the iterator are in the original text and sorted by count
    #[test]
    fn trigram_iter() {
        let lower = TEST_STRING.to_lowercase();
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        let mut prev = usize::MAX;
        for (trigram, counter, token) in stats.iter_trigrams() {
            println!("  '{}{}{}': {} #{}", trigram[0], trigram[1], trigram[2],
                     counter, token);
            assert!(lower.contains(&trigram[..]));
            assert_ne!(counter.cmp(&prev), Ordering::Greater);
            prev = *counter;
        }
    }

    // Check that get_symbol works for existing and non-existing symbols
    #[test]
    fn get_symbol() {
        let lower = TEST_STRING.to_lowercase();
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        // also check that all symbols in the string have entries
        for c in lower.chars().filter(|c| c.is_whitespace()) {
            assert_ne!(stats.get_symbol([c]), None);
        }

        assert_eq!(stats.get_symbol(['a']), None);
        assert_eq!(stats.get_symbol(['?']), None);
        assert_eq!(stats.get_symbol(['e']).unwrap().0, 3);
        assert_eq!(stats.get_symbol([',']).unwrap().0, 1);
    }

    // Check that get_bigram works for existing and non-existing symbols
    #[test]
    fn get_bigram() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats.get_bigram(['h', 'a']), None);
        assert_eq!(stats.get_bigram(['e', ',']), None);
        assert_eq!(stats.get_bigram(['h', 'e']).unwrap().0, 1);
        assert_eq!(stats.get_bigram(['e', 'l']).unwrap().0, 2);
    }

    // Check that get_trigram works for existing and non-existing symbols
    #[test]
    fn get_trigram() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats.get_trigram(['h', 'a', 'h']), None);
        assert_eq!(stats.get_trigram(['o', ',', 'w']), None);
        assert_eq!(stats.get_trigram(['h', 'e', 'l']).unwrap().0, 1);
        assert_eq!(stats.get_trigram(['e', 'l', 'l']).unwrap().0, 2);
    }

    // Check that indexing with symbols works
    #[test]
    fn index_symbol() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats[['h']].0, 1);
        assert_eq!(stats[['l']].0, 5);
        assert_eq!(stats[['!']].0, 1);
    }

    // Check that indexing with bigrams works
    #[test]
    fn index_bigram() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats[['l', 'l']].0, 2);
        assert_eq!(stats[['l', 'o']].0, 1);
        assert_eq!(stats[['o', ',']].0, 1);
    }

    // Check that indexing with trigrams works
    #[test]
    fn index_trigram() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats[['l', 'l', 'o']].0, 1);
        assert_eq!(stats[['l', 'o', ',']].0, 1);
        assert_eq!(stats[['w', 'o', 'r']].0, 1);
    }

    // Check that indexing with tokens works
    #[test]
    fn index_token() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats[stats[['w']].1], 2);
        assert_eq!(stats[stats[['b']].1], 1);

        assert_eq!(stats[stats[['w', 'o']].1], 1);
        assert_eq!(stats[stats[['b', 'e']].1], 1);

        assert_eq!(stats[stats[['o', 'r', 'l']].1], 1);
        assert_eq!(stats[stats[['r', 'l', 'd']].1], 1);
    }

    #[test]
    fn token_to_ngram() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        assert_eq!(stats.token_to_ngram(stats[['o']].1), [stats[['o']].1, 0, 0]);
        assert_eq!(stats.token_to_ngram(stats[['.']].1), [stats[['.']].1, 0, 0]);

        assert_eq!(stats.token_to_ngram(stats[['o', 'r']].1), [stats[['o']].1, stats[['r']].1, 0]);
        assert_eq!(stats.token_to_ngram(stats[['d', '!']].1), [stats[['d']].1, stats[['!']].1, 0]);

        assert_eq!(stats.token_to_ngram(stats[['l', 'd', '!']].1),
                   [stats[['l']].1, stats[['d']].1, stats[['!']].1]);
        assert_eq!(stats.token_to_ngram(stats[['w', 'e', 'l']].1),
                   [stats[['w']].1, stats[['e']].1, stats[['l']].1]);
    }

    // Test serialization as json
    #[test]
    fn to_json() {
        let stats = TextStats::from_str(TEST_STRING).unwrap();

        let j = serde_json::to_string_pretty(&stats).expect("Serialization failed");
        println!("{}", j);
    }
}
