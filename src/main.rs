use kuehlmak::TextStats;
use kuehlmak::{
    layout_from_str, Layout,
    EvalModel, EvalScores,
    KuehlmakModel, KuehlmakParams, KuehlmakScores,
    Anneal
};

use clap::{clap_app, ArgMatches};

use serde::{Serialize, Deserialize};

use std::path::{PathBuf, Path};
use std::str::FromStr;
use std::ffi::OsStr;
use std::process;
use std::env;
use std::io::{Read, self};
use std::fs;

static QWERTY: &str =
r#"q  w  e  r  t  y  u  i  o  p
   a  s  d  f  g  h  j  k  l ;:
   z  x  c  v  b  n  m ,< .> /?"#;

fn layout_from_file<P>(path: P) -> (Layout, usize)
    where P: AsRef<Path> + Copy
{
    let string = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Failed to read layout file '{}': {}",
                  path.as_ref().display(), e);
        process::exit(1)
    });
    let popularity = if let Some(last_line) = string.lines().last() {
        last_line.chars().filter(|&c| c == '#').count()
    } else {
        0usize
    };
    (layout_from_str(&string).unwrap_or_else(|e| {
        eprintln!("Failed to parse layout: {}", e);
        process::exit(1)
    }), popularity)
}

#[derive(Serialize, Deserialize)]
struct Config {
    text_file: Option<PathBuf>,
    #[serde(flatten)]
    params: KuehlmakParams,
}

fn config_from_file<P>(path: P) -> Config
    where P: AsRef<Path> + Copy
{
    let c = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Failed to read config file '{}': {}",
                  path.as_ref().display(), e);
        process::exit(1)
    });

    // Change current directory to make relative paths in the config behave
    let prev_dir = env::current_dir().expect("Failed to get current dir");
    if let Some(dir) = path.as_ref().parent() {
        env::set_current_dir(dir).expect("Failed to set current dir");
    }
    let mut config: Config = toml::from_str(&c).unwrap_or_else(|e| {
        eprintln!("Failed to parse config file '{}': {}",
                  path.as_ref().display(), e);
        process::exit(1)
    });
    if let Some(path) = config.text_file.as_mut() {
        *path = path.canonicalize().unwrap_or_else(|e| {
            eprintln!("Invalid path '{}': {}", path.display(), e);
            process::exit(1);
        });
    }
    env::set_current_dir(&prev_dir).expect("Failed to set current dir");
    config
}

fn text_from_file(path: Option<&Path>) -> TextStats {
    let mut is_json = false;
    let contents = if let Some(path) = path {
        is_json = path.extension().map(|e| e.to_ascii_lowercase() == "json")
                                  .unwrap_or(false);
        fs::read_to_string(path)
    } else {
        let mut s = String::new();
        match io::stdin().read_to_string(&mut s) {
            Ok(_size) => Ok(s),
            Err(e) => Err(e),
        }
    }.unwrap_or_else(|e| {
        eprintln!("Failed to read text file '{}': {}",
                  path.unwrap_or("<stdin>".as_ref()).display(), e);
        process::exit(1)
    });
    if is_json {
        serde_json::from_str::<TextStats>(&contents).unwrap_or_else(|e| {
            eprintln!("Failed to parse JSON file '{}': {}",
                      path.unwrap().display(), e);
            process::exit(1)
        })
    } else {
        // This shouldn't panic
        TextStats::from_str(&contents).unwrap()
    }
}

fn anneal_command(sub_m: &ArgMatches) {
    let config = sub_m.value_of("config").map(config_from_file);

    let layout = match sub_m.value_of("layout") {
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

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or(
                        config.as_ref().and_then(|c| c.text_file.as_deref()));
    let text = text_from_file(text_filename);
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

    let kuehlmak_model = KuehlmakModel::new(config.map(|c| c.params));
    let mut anneal = Anneal::new(&kuehlmak_model, &text, layout, shuffle,
                                 steps);

    let mut scores = kuehlmak_model.eval_layout(&layout, &text, 1.0);
    let stdout = &mut io::stdout();
    anneal.write_stats(stdout).unwrap();
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

fn eval_command(sub_m: &ArgMatches) {
    let config = sub_m.value_of("config").map(config_from_file);

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or(
                        config.as_ref().and_then(|c| c.text_file.as_deref()));
    let text = text_from_file(text_filename);
    // Not filtering with any alphabet because different layouts may use
    // different alphabets.

    let verbose = sub_m.is_present("verbose");

    let kuehlmak_model = KuehlmakModel::new(config.map(|c| c.params));
    let stdout = &mut io::stdout();

    for filename in sub_m.values_of("LAYOUT").into_iter().flatten() {
        let (layout, _) = layout_from_file(filename);

        let scores = kuehlmak_model.eval_layout(&layout, &text, 1.0);

        println!("=== {} ===================", filename);
        scores.write(stdout).unwrap();
        if verbose {
            scores.write_extra(stdout).unwrap();
        }
    }
}

fn get_dir_paths(dir: &str) -> io::Result<Vec<PathBuf>> {
    fs::read_dir(dir)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()
}

fn rank_command(sub_m: &ArgMatches) {
    let mut config: Option<Config> = None;
    let mut layouts: Vec<_> = Vec::new();
    let dir = sub_m.value_of("dir").unwrap();
    let paths = get_dir_paths(dir).unwrap();
    for path in paths.into_iter().filter(|p| p.is_file()) {
        match path.extension().and_then(OsStr::to_str) {
            Some("kbl") => {
                layouts.push(layout_from_file(&path));
            },
            Some("toml")
                    if path.file_name().unwrap() == "config.toml" => {
                config = Some(config_from_file(&path));
            },
            _ => (), // ignore other files
        }
    }

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or(
                        config.as_ref().and_then(|c| c.text_file.as_deref()));
    let text = text_from_file(text_filename);
    // Not filtering with any alphabet because different layouts may use
    // different alphabets.

    let kuehlmak_model = KuehlmakModel::new(config.map(|c| c.params));
    let mut score_name_map = KuehlmakScores::get_score_names();
    score_name_map.insert("popularity".to_string(), score_name_map.len());

    let mut scores: Vec<_> = layouts.iter().map(|(l, p)| {
        let s = kuehlmak_model.eval_layout(l, &text, 1.0);
        let mut cs = s.get_scores();
        cs.push(*p as f64);
        (s, cs, 0usize, vec![0usize; score_name_map.len()])
    }).collect();

    // Sort scores by different criteria and add up rankings per layout
    let score_names = sub_m.value_of("scores").unwrap_or("total");
    for name in score_names.split(',') {
        let raw_name = if name.starts_with('+') {&name[1..]} else {name};

        if let Some(&score) = score_name_map.get(raw_name) {
            let mut sorted_scores: Vec<_> = scores.iter_mut().collect();
            sorted_scores.sort_by(|(_, a, _, _), (_, b, _, _)|
                                  a[score].partial_cmp(&b[score]).unwrap());
            if name.starts_with('+') {
                sorted_scores.reverse();
            }
            let mut r = 0;
            let mut inc = *sorted_scores[0].1.last().unwrap() as usize;
            let mut prev = sorted_scores[0].1[score];
            for (_, comp_score, rank, comp_rank) in sorted_scores.into_iter()
                                                                 .skip(1) {
                // Give the same rank to layouts with equal score
                if prev != comp_score[score] {
                    r += inc;
                    inc = 0;
                    prev = comp_score[score];
                }
                inc += *comp_score.last().unwrap() as usize;
                comp_rank[score] = r;
                *rank += r;
            }
        } else {
            eprintln!("Unknown score name {}. Valid names are:", name);
            for name in score_name_map.keys() {
                eprintln!("  {}", name);
            }
            process::exit(1);
        }
    }

    // Sort scores by cumulative ranking
    let mut ranked_scores: Vec<_> = scores.iter().collect();
    ranked_scores.sort_by_key(|&(_, _, r, _)| r);

    // Print the first n layouts
    let n: usize = match sub_m.value_of("number") {
        Some(number) => number.parse().unwrap_or_else(|e| {
            eprintln!("Invalid number '{}': {}", number, e);
            process::exit(1)
        }),
        None => scores.len(),
    };
    let stdout = &mut io::stdout();
    for (s, cs, _, cr) in ranked_scores.into_iter().take(n) {
        print!("=== {:.0}x ", cs.last().unwrap());
        for name in score_names.split(',') {
            let raw_name = if name.starts_with('+') {&name[1..]} else {name};
            if let Some(&score) = score_name_map.get(raw_name) {
                print!("{}={} ", name, cr[score]);
            }
        }
        println!("===");
        s.write(stdout).unwrap();
        println!();
    }
}

fn textstats_command(sub_m: &ArgMatches) {
    let text_filename = sub_m.value_of("text").map(|p| p.as_ref());
    let text = text_from_file(text_filename);

    let text = if let Some(alpha) = sub_m.value_of("alphabet") {
        let mut alphabet = vec![];
        let mut last_char = '\0';
        let mut in_range = false;

        for c in alpha.chars() {
            if in_range {
                if c > last_char {
                    for c in (last_char..=c).into_iter().skip(1) {
                        alphabet.push(c)
                    }
                } else if c < last_char {
                    for c in c..last_char {
                        alphabet.push(c)
                    }
                }
                in_range = false;
            } else if c == '-' && last_char != '\0' {
                in_range = true;
            } else {
                alphabet.push(c);
                last_char = c;
            }
        }

        alphabet.sort();
        text.filter(|c| alphabet.binary_search(&c).is_ok())
    } else {
        text
    };

    let j = if sub_m.is_present("pretty") {
        serde_json::to_string_pretty(&text)
    } else {
        serde_json::to_string(&text)
    }.expect("Serialization failed");
    println!("{}", j);
}

fn main() {
    let app_m = clap_app!(kuehlmak =>
        (version: "0.1")
        (author: "Felix Kuehling <felix.kuehling@gmail.com>")
        (about: "Keyboard layout generator and analyzer")
        (@subcommand textstats =>
            (about: "Compute text statistics, write JSON to stdout")
            (version: "0.1")
            (@arg alphabet: -a --alphabet +takes_value
                "Filter stats only for those symbols\n(e.g. '-_a-z;,./<>?:')")
            (@arg pretty: --pretty
                "Pretty-print JSON output")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input, stdin if not specified")
        )
        (@subcommand anneal =>
            (about: "Generate a layout with Simulated Annealing")
            (version: "0.1")
            (@arg config: -c --config +takes_value
                "Configuration file")
            (@arg layout: -l --layout +takes_value
                "Initial layout filename [QWERTY]")
            (@arg noshuffle: -n --("no-shuffle")
                "Don't shuffle initial layout")
            (@arg steps: -s --steps +takes_value
                "Steps per annealing iteration [10000]")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input, stdin if not specified here or in config")
        )
        (@subcommand eval =>
            (about: "Evaluate layouts")
            (version: "0.1")
            (@arg config: -c --config +takes_value
                "Configuration file")
            (@arg verbose: -v --verbose
                "Print extra information for each layout")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input, stdin if not specified here or in config")
            (@arg LAYOUT: +multiple
                "Layout to evaluate")
        )
        (@subcommand rank =>
            (about: "Rank layouts")
            (version: "0.1")
            (@arg dir: -d --dir +takes_value +required
                "DB and configuration directory")
            (@arg number: -n --number +takes_value
                "Number of top-ranked layouts to output")
            (@arg scores: -s --scores +takes_value
                "Comma-separated list of scores to rank layouts by")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input, stdin if not specified here or in config")
        )
    ).get_matches();

    match app_m.subcommand_name() {
        Some("anneal") => anneal_command(app_m.subcommand_matches("anneal")
                                              .unwrap()),
        Some("eval") => eval_command(app_m.subcommand_matches("eval")
                                          .unwrap()),
        Some("rank") => rank_command(app_m.subcommand_matches("rank")
                                              .unwrap()),
        Some("textstats") => textstats_command(app_m.subcommand_matches("textstats")
                                                    .unwrap()),
        Some(unknown) => panic!("Unhandled subcommand: {}", unknown),
        None => {
            eprintln!("No subcommand given.\n{}", app_m.usage());
            process::exit(1)
        },
    }
}
