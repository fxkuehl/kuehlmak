use kuehlmak::TextStats;
use kuehlmak::{Layout, EvalModel, KuehlmakModel};
use std::str::FromStr;

static TEST_STRING: &str = "Hello, world! Be well.";

static QWERTY: Layout = [
    ['q','Q'],['w','W'],['e','E'],['r','R'],['t','T'],['y','Y'],['u','U'],['i','I'],['o','O'],['p','P'],
    ['a','A'],['s','S'],['d','D'],['f','F'],['g','G'],['h','H'],['j','J'],['k','K'],['l','L'],[';',':'],
    ['z','Z'],['x','X'],['c','C'],['v','V'],['b','B'],['n','N'],['m','M'],[',','<'],['.','>'],['/','?']
];

fn main() {
    let stats = TextStats::from_str(TEST_STRING).unwrap();
    let j = serde_json::to_string_pretty(&stats).expect("Serialization failed");
    println!("{}", j);

    let kuehlmak_model = KuehlmakModel::new();
    println!("Model size: {}", std::mem::size_of_val(&kuehlmak_model));

    let scores = kuehlmak_model.eval_layout(&QWERTY, &stats);
    println!("Scores size: {}", std::mem::size_of_val(&scores));
}
