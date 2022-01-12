use kuehlmak::TextStats;
use kuehlmak::{
    layout_from_str, EvalModel, EvalScores, KuehlmakModel, KuehlmakParams,
    Anneal
};

use clap::{clap_app, ArgMatches};

use std::str::FromStr;
use std::process;
use std::io;
use std::fs;

static QWERTY: &str =
r#"q  w  e  r  t  y  u  i  o  p
   a  s  d  f  g  h  j  k  l ;:
   z  x  c  v  b  n  m ,< .> /?"#;

static COLEMAK: &str =
r#"q  w  f  p  g  j  l  u  y ;:
   a  r  s  t  d  h  n  e  i  o
   z  x  c  v  b  k  m ,< .> /?"#;

static DVORAK: &str =
r#"'" ,< .>  p  y  f  g  c  r  l
    a  o  e  u  i  d  h  t  n  s
   ;:  q  j  k  x  b  m  w  v  z"#;

fn anneal_command(sub_m: &ArgMatches) {
    let mut config: Option<KuehlmakParams> = None;
    if let Some(filename) = sub_m.value_of("config") {
        let c = fs::read_to_string(filename).unwrap_or_else(|e| {
            eprintln!("Failed to read config file '{}': {}", filename, e);
            process::exit(1)
        });
        config = Some(toml::from_str(&c).unwrap_or_else(|e| {
            eprintln!("Failed to parse config file '{}': {}", filename, e);
            process::exit(1)
        }));
    }

    let layout = match sub_m.value_of("layout") {
        Some("QWERTY") => String::from(QWERTY),
        Some("Colemak") => String::from(COLEMAK),
        Some("Dvorak") => String::from(DVORAK),
        Some(filename) => fs::read_to_string(filename).unwrap_or_else(|e| {
            eprintln!("Failed to read layout file '{}': {}", filename, e);
            process::exit(1)
        }),
        None => String::from(QWERTY),
    };
    let layout = layout_from_str(&layout).unwrap_or_else(|e| {
        eprintln!("Failed to parse layout: {}", e);
        process::exit(1)
    });

    if let Some(config) = config.as_mut() {
        config.set_ref_layout(&layout);
    }

    // Won't panic because TEXT is mandatory
    let text_filename = sub_m.value_of("TEXT").unwrap();
    let text_contents = fs::read_to_string(text_filename).unwrap_or_else(|e| {
        eprintln!("Failed to read TEXT file '{}': {}", text_filename, e);
        process::exit(1)
    });
    let text = if text_filename.ends_with(".json") {
        serde_json::from_str::<TextStats>(&text_contents).unwrap_or_else(|e| {
            eprintln!("Failed to parse JSON file '{}': {}", text_filename, e);
            process::exit(1)
        })
    } else {
        // This shouldn't panic
        TextStats::from_str(&text_contents).unwrap()
    };
    let mut alphabet: Vec<_> = layout.iter().flatten().copied().collect();
    alphabet.sort();
    let text = text.filter(|c| alphabet.binary_search(&c).is_ok());

    let shuffle = !sub_m.is_present("noshuffle");
    let steps: u64 = match sub_m.value_of("steps")
                                .unwrap_or("10000").parse() {
        Ok(num) => num,
        Err(e) => {
            eprintln!("Invalid value for --setps: {}\n{}", e, sub_m.usage());
            process::exit(1)
        }
    };

    let kuehlmak_model = KuehlmakModel::new(config);
    let mut anneal = Anneal::new(&kuehlmak_model, &text, layout, shuffle,
                                 steps);

    let mut scores = kuehlmak_model.eval_layout(&layout, &text, 1.0);
    let stdout = &mut io::stdout();
    scores.write(stdout).unwrap();

    loop {
        if let Some(s) = anneal.next() {
            // VT100: cursor up 9 rows
            print!("\x1b[9A");
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

fn main() {
    let app_m = clap_app!(kuehlmak =>
        (version: "0.1")
        (author: "Felix Kuehling <felix.kuehling@gmail.com>")
        (about: "Keyboard layout generator and analyzer")
        (@subcommand anneal =>
            (about: "Generate a layout with Simulated Annealing")
            (version: "0.1")
            (@arg config: -c --config +takes_value
                "Configuration file")
            (@arg layout: -l --layout +takes_value
                "Initial layout name or filename [QWERTY]")
            (@arg noshuffle: -n --("no-shuffle")
                "Don't shuffle initial layout")
            (@arg steps: -s --steps +takes_value
                "Steps per annealing iteration [10000]")
            (@arg TEXT: +required
                "Text or JSON file to use as input")
        )
    ).get_matches();

    match app_m.subcommand_name() {
        Some("anneal") => anneal_command(app_m.subcommand_matches("anneal")
                                              .unwrap()),
        Some(unknown) => panic!("Unhandled subcommand: {}", unknown),
        None => {
            eprintln!("No subcommand given.\n{}", app_m.usage());
            process::exit(1)
        },
    }
}
