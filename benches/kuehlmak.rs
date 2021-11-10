#[macro_use]
extern crate bencher;

use kuehlmak::TextStats;
use std::str::FromStr;

use bencher::Bencher;

static TEST_STRING : &str = "Hello, world! Be well.
Some more text to make the hash tables a bit bigger/+-sized.
Extremely conspicuous excess of foreign words: Producing copious
variety of n-grams*. (Just zero questions.)";

fn get_symbol(bench: &mut Bencher) {
    let stats = TextStats::from_str(TEST_STRING).unwrap();
    let test_symbols = [['h'], ['e'], ['l'], ['o'], [','],
                        [' '], ['w'], ['r'], ['d'], ['!']];

    bench.iter( || {
        test_symbols.iter().
            filter_map(|s| stats.get_symbol(*s)).
            count()
    })
}

fn get_bigram(bench: &mut Bencher) {
    let stats = TextStats::from_str(TEST_STRING).unwrap();
    let test_bigrams = [['h', 'e'], ['e', 'l'], ['l', 'l'], ['l', 'o'],
                        ['o', ','], ['w', 'o'], ['o', 'r'], ['r', 'l'],
                        ['l', 'd'], ['d', '!']];

    bench.iter( || {
        test_bigrams.iter().
            filter_map(|b| stats.get_bigram(*b)).
            count()
    })
}

fn get_trigram(bench: &mut Bencher) {
    let stats = TextStats::from_str(TEST_STRING).unwrap();
    let test_trigrams = [['h', 'e', 'l'], ['e', 'l', 'l'], ['l', 'l', 'o'],
                         ['l', 'o', ','], ['w', 'o', 'r'], ['o', 'r', 'l'],
                         ['r', 'l', 'd'], ['l', 'd', '!'], ['w', 'e', 'l'],
                         ['l', 'l', '.']];

    bench.iter( || {
        test_trigrams.iter().
            filter_map(|t| stats.get_trigram(*t)).
            count()
    })
}

fn index_token(bench: &mut Bencher) {
    let stats = TextStats::from_str(TEST_STRING).unwrap();
    let test_tokens = [
        stats[['h']].1, stats[['e']].1, stats[['l']].1,
        stats[['l','l']].1, stats[['l','o']].1, stats[['o',',']].1,
        stats[['w','o','r']].1, stats[['o','r','l']].1,
        stats[['r','l','d']].1, stats[['l','d','!']].1];

    bench.iter( || {
        test_tokens.iter().
            filter(|t| stats[**t] > 1).
            count()
    })
}

benchmark_group!(benches, get_symbol, get_bigram, get_trigram, index_token);
benchmark_main!(benches);
