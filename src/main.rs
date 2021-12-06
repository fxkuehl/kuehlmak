use kuehlmak::TextStats;
use kuehlmak::{
    layout_from_str, layout_to_str, layout_to_filename,
    EvalModel, EvalScores, KuehlmakModel,
    Anneal
};
//use std::str::FromStr;
use std::io;
use std::fs;

//static TEST_STRING: &str = "Hello, world! Be well.";

static COLEMAK: &str =
r#"q  w  f  p  g  j  l  u  y ;:
   a  r  s  t  d  h  n  e  i  o
   z  x  c  v  b  k  m ,< .> /?
Random garbage at the end of the string gets ignored.
"#;

fn main() {
    //let stats = TextStats::from_str(TEST_STRING).unwrap();
    let json = fs::read_to_string("benches/bench_text.json").unwrap();
    let stats = serde_json::from_str::<TextStats>(&json).unwrap();

    let kuehlmak_model = KuehlmakModel::new();

    let layout = layout_from_str(COLEMAK).unwrap();
    print!("{}", layout_to_str(&layout));
    println!("{}", layout_to_filename(&layout));
    let mut scores = kuehlmak_model.eval_layout(&layout, &stats, 1.0);

    let stdout = &mut io::stdout();
    scores.write(stdout).unwrap();

    let mut anneal = Anneal::new(&kuehlmak_model, &stats, layout, true,
                                 10000);
    loop {
        if let Some(s) = anneal.next() {
            // VT100: cursor up 8 rows
            print!("\x1b[8A");
            // VT100 clear line (top row of the last keymap)
            print!("\x1b[2K");
            anneal.write_stats(stdout).unwrap();
            s.write(stdout).unwrap();

            scores = s;
        } else {
            break;
        }
    }

    scores.write_to_db("./db").unwrap();
}
