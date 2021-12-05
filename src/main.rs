use kuehlmak::TextStats;
use kuehlmak::{Layout, EvalModel, EvalScores, KuehlmakModel, Anneal};
//use std::str::FromStr;
use std::io;
use std::fs;

//static TEST_STRING: &str = "Hello, world! Be well.";

static QWERTY: Layout = [
    ['q','Q'],['w','W'],['e','E'],['r','R'],['t','T'],['y','Y'],['u','U'],['i','I'],['o','O'],['p','P'],
    ['a','A'],['s','S'],['d','D'],['f','F'],['g','G'],['h','H'],['j','J'],['k','K'],['l','L'],[';',':'],
    ['z','Z'],['x','X'],['c','C'],['v','V'],['b','B'],['n','N'],['m','M'],[',','<'],['.','>'],['/','?']
];

fn main() {
    //let stats = TextStats::from_str(TEST_STRING).unwrap();
    let json = fs::read_to_string("benches/bench_text.json").unwrap();
    let stats = serde_json::from_str::<TextStats>(&json).unwrap();

    let kuehlmak_model = KuehlmakModel::new();

    let scores = kuehlmak_model.eval_layout(&QWERTY, &stats);

    let stdout = &mut io::stdout();
    scores.write(stdout).unwrap();

    let mut anneal = Anneal::new(&kuehlmak_model, &stats, &QWERTY, false,
                                 10000);
    anneal.write_stats(stdout).unwrap();
    loop {
        if let Some(scores) = anneal.next() {
            anneal.write_stats(stdout).unwrap();
            scores.write(stdout).unwrap();
        } else {
            break;
        }
    }
}
