#[macro_use]
extern crate bencher;

use kuehlmak::TextStats;
use kuehlmak::{Layout, EvalModel, KuehlmakModel};
use std::str::FromStr;
use std::fs;

use bencher::Bencher;

static TEST_STRING : &str = "Hello, world! Be well.
Some more text to make the hash tables a bit bigger/+-sized.
Extremely conspicuous excess of foreign words: Producing copious
variety of n-grams*. (Just zero questions.)";

static QWERTY: Layout = [
    ['q','Q'],['w','W'],['e','E'],['r','R'],['t','T'],['y','Y'],['u','U'],['i','I'],['o','O'],['p','P'],
    ['a','A'],['s','S'],['d','D'],['f','F'],['g','G'],['h','H'],['j','J'],['k','K'],['l','L'],[';',':'],
    ['z','Z'],['x','X'],['c','C'],['v','V'],['b','B'],['n','N'],['m','M'],[',','<'],['.','>'],['/','?']
];

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

fn eval_layout(bench: &mut Bencher) {
    let alphabet: String = QWERTY.iter().flatten().collect();
    let stats = TextStats::from_str(TEST_STRING).unwrap()
        .filter(|c| alphabet.contains(c));
    let kuehlmak_model = KuehlmakModel::new();
    bench.iter( || {
        let _scores = kuehlmak_model.eval_layout(&QWERTY, &stats, 1.0);
    })
}

fn eval_layout_json(bench: &mut Bencher) {
    if let Ok(json) = fs::read_to_string("benches/bench_text.json") {
        if let Ok(stats) = serde_json::from_str::<TextStats>(&json) {
            let alphabet: String = QWERTY.iter().flatten().collect();
            let stats = stats.filter(|c| alphabet.contains(c));
            let kuehlmak_model = KuehlmakModel::new();
            bench.iter( || {
                let _scores = kuehlmak_model.eval_layout(&QWERTY, &stats, 1.0);
            });
        } else {
            eprintln!("Deserialization failed");
        }
    } else {
        eprintln!("Reading JSON file failed");
    }
}

benchmark_group!(text, get_symbol, get_bigram, get_trigram, index_token);
benchmark_group!(eval, eval_layout, eval_layout_json);
benchmark_main!(text, eval);
