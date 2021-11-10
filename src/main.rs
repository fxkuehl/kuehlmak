use kuehlmak::TextStats;
use std::str::FromStr;

static TEST_STRING : &str = "Hello, world! Be well.";

fn main() {
    let stats = TextStats::from_str(TEST_STRING).unwrap();
    let j = serde_json::to_string_pretty(&stats).expect("Serialization failed");
    println!("{}", j);
}
