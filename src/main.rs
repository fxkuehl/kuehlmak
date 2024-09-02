use kuehlmak::TextStats;
use kuehlmak::{
    layout_from_str, Layout,
    EvalModel, EvalScores,
    KuehlmakModel, KuehlmakParams, KuehlmakScores,
    Anneal
};

use clap::{clap_app, ArgMatches};

use serde::{Serialize, Deserialize};

use threadpool::ThreadPool;
use std::sync::mpsc::channel;

use std::path::{PathBuf, Path};
use std::str::FromStr;
use std::ffi::OsStr;
use std::process;
use std::env;
use std::io::{Read, Write, self};
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
        last_line.chars().filter(|&c| c == '#').count().max(1)
    } else {
        1usize
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
        if dir != Path::new("") {
            env::set_current_dir(dir).expect("Failed to set current dir");
        }
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
        eprintln!("Reading text from stdin ...");
        let mut s = String::new();
        match io::stdin().read_to_string(&mut s) {
            Ok(_size) => Ok(s),
            Err(e) => Err(e),
        }
    }.unwrap_or_else(|e| {
        eprintln!("Failed to read text file '{}': {}",
                  path.unwrap_or_else(|| "<stdin>".as_ref()).display(), e);
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
    let dir: &Path = sub_m.value_of("dir").unwrap().as_ref();
    if !dir.is_dir() {
        eprintln!("Not a directory: '{}'", dir.display());
        process::exit(1);
    }
    let db_config: PathBuf = [dir,"config.toml".as_ref()].into_iter().collect();
    let config = sub_m.value_of("config").map(str::as_ref)
                      .or_else(|| Some(db_config.as_path())
                                    .filter(|p| p.is_file()))
                      .map(config_from_file);

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

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or_else(
                    || config.as_ref().and_then(|c| c.text_file.as_deref()));
    let text = text_from_file(text_filename);
    let mut alphabet: Vec<_> = layout.iter().flatten().copied().collect();
    alphabet.push(' ');
    alphabet.sort();
    let text = text.filter(|c| alphabet.binary_search(&c).is_ok(), 1);

    let kuehlmak_model = KuehlmakModel::new(config.map(|c| c.params));

    let shuffle = !sub_m.is_present("noshuffle");
    let steps: u64 = match sub_m.value_of("steps")
                                .unwrap_or("10000").parse() {
        Ok(num) => num,
        Err(e) => {
            eprintln!("Invalid value for --setps: {}\n{}", e, sub_m.usage());
            process::exit(1)
        }
    };
    let progress = sub_m.is_present("progress");
    let show_scores = sub_m.is_present("show_scores");

    let jobs: usize = match sub_m.value_of("jobs") {
        Some(number) => number.parse().unwrap_or_else(|e| {
            eprintln!("Invalid number '{}': {}", number, e);
            process::exit(1)
        }),
        None => 1,
    };

    let n: usize = match sub_m.value_of("number") {
        Some(number) => number.parse().unwrap_or_else(|e| {
            eprintln!("Invalid number '{}': {}", number, e);
            process::exit(1)
        }),
        None => 1,
    };

    // Generate n layouts using j worker threads
    let pool = ThreadPool::new(jobs);
    let (tx, rx) = channel();
    let stdout = &mut io::stdout();
    for _ in 0..n {
        // Clone stuff that gets moved into the worker closure
        let model = kuehlmak_model.clone();
        let text = text.clone();
        let tx = tx.clone();
        let dir = dir.to_owned();

        pool.execute(move || {
            let mut anneal = Anneal::new(&model, &text, layout, shuffle, steps);
            let mut scores = model.eval_layout(&layout, &text, 1.0);

            while let Some(s) = anneal.next() {
                if progress {
                    let mut w = Vec::new();
                    anneal.write_stats(&mut w).unwrap();
                    s.write(&mut w, show_scores).unwrap();
                    // VT100: cursor up 9 rows
                    write!(&mut w, "\x1b[9A").unwrap();
                    tx.send(w).unwrap();
                }

                scores = s;
            }

            let mut w = Vec::new();
            writeln!(&mut w).unwrap();
            scores.write(&mut w, show_scores).unwrap();
            tx.send(w).unwrap();

            scores.write_to_db(&dir, show_scores).unwrap();
        });

        // Process messages until the queue drops below a threshold. This
        // avoids unbounded memory allocations for the worker closures.
        // Assume that workers send messages before terminating, so we can
        // wait for messages without worrying that workers will go idle.
        while pool.queued_count() >= jobs {
            stdout.write(&rx.recv().unwrap()).unwrap();
        }
    }

    // Drop the original sender so the receiver will start failing once all
    // the Senders in the workers have hung up.
    drop(tx);

    // Drain any remaining messages. This implicitly waits for the workers
    // to finish.
    while let Ok(msg) = rx.recv() {
        stdout.write(&msg).unwrap();
    }
}

fn eval_command(sub_m: &ArgMatches) {
    let config = sub_m.value_of("config").map(config_from_file);

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or_else(
                    || config.as_ref().and_then(|c| c.text_file.as_deref()));
    let text = text_from_file(text_filename);
    // Not filtering with any alphabet because different layouts may use
    // different alphabets.

    let verbose = sub_m.is_present("verbose");
    let show_scores = sub_m.is_present("show_scores");

    let kuehlmak_model = KuehlmakModel::new(config.map(|c| c.params));
    let stdout = &mut io::stdout();

    for filename in sub_m.values_of("LAYOUT").into_iter().flatten() {
        let (layout, _) = layout_from_file(filename);

        let scores = kuehlmak_model.eval_layout(&layout, &text, 1.0);

        println!("=== {} ===================", filename);
        scores.write(stdout, show_scores).unwrap();
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
    let mut config = sub_m.value_of("config").map(config_from_file);
    let mut layouts: Vec<_> = Vec::new();
    let dir = sub_m.value_of("dir").unwrap();
    let paths = get_dir_paths(dir).unwrap();
    for path in paths.into_iter().filter(|p| p.is_file()) {
        match path.extension().and_then(OsStr::to_str) {
            Some("kbl") => {
                layouts.push(layout_from_file(&path));
            },
            Some("toml")
                    if config.is_none() &&
                       path.file_name().unwrap() == "config.toml" => {
                config = Some(config_from_file(&path));
            },
            _ => (), // ignore other files
        }
    }

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or_else(
                    || config.as_ref().and_then(|c| c.text_file.as_deref()));
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

    if scores.len() == 0 {
        println!("No layouts found.");
        return;
    }

    // Sort scores by different criteria and add up rankings per layout
    let score_names = sub_m.value_of("scores").unwrap_or("total");
    for name in score_names.split(',') {
        let raw_name = name.strip_prefix('+').unwrap_or(name);

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
    let show_scores = sub_m.is_present("show_scores");

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
            let raw_name = name.strip_prefix('+').unwrap_or(name);
            if let Some(&score) = score_name_map.get(raw_name) {
                print!("{}={} ", name, cr[score]);
            }
        }
        println!("===");
        s.write(stdout, show_scores).unwrap();
        println!();
    }
}

fn estimate_population_size(u: usize, k: usize) -> usize {
    if u >= k {
        return usize::MAX;
    }
    let mut n = u;
    let mut m = n;
    let unique = |n: f64, k: usize| n * (1.0 - ((n - 1.0) / n).powi(k as i32));
    while unique(m as f64, k) < u as f64 {
        if m == usize::MAX {
            return m;
        } else if m >= usize::MAX / 2 {
            m = usize::MAX;
        } else {
            m *= 2;
        }
    }
    while n+1 < m {
        let mid = (n + m) / 2;
        if unique(mid as f64, k) < u as f64 {
            n = mid;
        } else {
            m = mid;
        }
    }
    n
}

fn stats_command(sub_m: &ArgMatches) {
    let mut config = sub_m.value_of("config").map(config_from_file);
    let mut layouts: Vec<_> = Vec::new();
    let dir = sub_m.value_of("dir").unwrap();
    let paths = get_dir_paths(dir).unwrap();
    for path in paths.into_iter().filter(|p| p.is_file()) {
        match path.extension().and_then(OsStr::to_str) {
            Some("kbl") => {
                layouts.push(layout_from_file(&path));
            },
            Some("toml")
                    if config.is_none() &&
                       path.file_name().unwrap() == "config.toml" => {
                config = Some(config_from_file(&path));
            },
            _ => (), // ignore other files
        }
    }

    let text_filename = sub_m.value_of("text").map(|p| p.as_ref()).or_else(
                    || config.as_ref().and_then(|c| c.text_file.as_deref()));
    let text = text_from_file(text_filename);
    // Not filtering with any alphabet because different layouts may use
    // different alphabets.

    let kuehlmak_model = KuehlmakModel::new(config.map(|c| c.params));
    let mut score_name_map = KuehlmakScores::get_score_names();
    score_name_map.insert("popularity".to_string(), score_name_map.len());
    let mut sample_size = 0usize;

    let mut scores: Vec<_> = layouts.iter().map(|(l, p)| {
        let s = kuehlmak_model.eval_layout(l, &text, 1.0);
        let mut cs = s.get_scores();
        cs.push(*p as f64);
        sample_size += *p;
        (s, cs)
    }).collect();

    // To estimate the expected number of unique layouts, a random draw from
    // a finite population of solutions is not a good model because the
    // annealing algorithm heavily favors some solutions over others, while it
    // can infrequently draw less likely solutions from a total population
    // that's practically infinite.
    //
    // Split the set of unique layouts found by popularity into the top
    // quartile, middle half and bottom quartile. They can approximate
    // separate populations, each of which the annealing algorithm can draw
    // from with different probability. The top quartile represents the most
    // popular/likely solutions with a relatively small total population.
    // The bottom quartiles is a tail of one-off solutions that is drawn from
    // a practically infinite population. Given enough time it could grow
    // indefinitely, but it's not worth waiting for. The middle half is the
    // one that has the largest growth potential in any remotely reasonable
    // time frame.
    scores.sort_by(|(_, a), (_, b)| a.last().partial_cmp(&b.last()).unwrap());
    let mut part_pop = [(0usize, 0usize, 0usize); 3];
    for (i, (_, cs)) in scores.iter().enumerate() {
        let p = *cs.last().unwrap() as usize;
        let q = (i * 2 + scores.len() / 2) / scores.len();
        part_pop[q].0 += p;
        part_pop[q].1 += 1;
    }
    for (pop, uni, est) in part_pop.iter_mut() {
        *est = estimate_population_size(*uni,
                                        if *uni < *pop {*pop} else {*uni + 1});
    }

    println!();
    println!("Unique/total layouts found: {}/{}, >{} unique layouts expected",
             scores.len(), sample_size,
             part_pop[0].1*2 + part_pop[1].2 + part_pop[2].2);
    println!();

    if scores.len() == 0 {
        return;
    }

    // Sort scores by different criteria and compute stats
    println!("{:>12}: {:^10} {:^10} {:^6} {:^6} {:^6} {:^6} {:^6} {:^6}",
             "Score", "Popular", "Min", "Lower", "Median", "Upper", "Max", "IQR", "Range");
    println!("------------------------------------------------------------------------------");
    let score_names = sub_m.value_of("scores").unwrap_or("total");
    for name in score_names.split(',') {
        let raw_name = name.strip_prefix('+').unwrap_or(name);

        if let Some(&score) = score_name_map.get(raw_name) {
            let mut sorted_scores: Vec<_> = scores.iter_mut().collect();
            sorted_scores.sort_by(|(_, a), (_, b)|
                                  a[score].partial_cmp(&b[score]).unwrap());
            if name.starts_with('+') {
                sorted_scores.reverse();
            }
            let mut quartiles = [0f64; 5];
            quartiles[0] = sorted_scores[0].1[score];
            let mut c = 0usize;
            let top_pop = *sorted_scores[0].1.last().unwrap() as usize;
            let mut max_pop = 0;
            let mut max_pop_score = 0.0;

            for (_, cs) in sorted_scores {
                let p = *cs.last().unwrap() as usize;
                let q0 = c * 4 / sample_size;
                c += p;
                let q1 = c * 4 / sample_size;
                for q in q0..q1 {
                    quartiles[q+1] = cs[score];
                }
                if p > max_pop {
                    max_pop = p;
                    max_pop_score = cs[score];
                }
            }
            println!("{:>12}: {:6.1}×{:<3} {:6.1}×{:<3} {:6.1} {:6.1} {:6.1} {:6.1} {:6.1} {:6.1}",
                     name, max_pop_score, max_pop, quartiles[0], top_pop,
                     quartiles[1], quartiles[2], quartiles[3], quartiles[4],
                     (quartiles[3] - quartiles[1]).abs(), (quartiles[4] - quartiles[0]).abs());
        } else {
            eprintln!("Unknown score name {}. Valid names are:", name);
            for name in score_name_map.keys() {
                eprintln!("  {}", name);
            }
            process::exit(1);
        }
    }
    println!();
}

#[allow(clippy::comparison_chain)]
fn corpus_command(sub_m: &ArgMatches) {
    let text_filename = sub_m.value_of("input").map(|p| p.as_ref());
    let text = text_from_file(text_filename);
    let min: u64 = match sub_m.value_of("min") {
        Some(number) => number.parse().unwrap_or_else(|e| {
            eprintln!("Invalid number '{}': {}", number, e);
            process::exit(1)
        }),
        None => 1
    };

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
        text.filter(|c| alphabet.binary_search(&c).is_ok(), min)
    } else if min > 1 {
        text.filter(|_| true, min)
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
        (@subcommand corpus =>
            (about: "Compute corpus statistics, write JSON to stdout")
            (version: "0.1")
            (@arg alphabet: -a --alphabet +takes_value
                "Filter stats only for those symbols\n(e.g. '-_a-z;,./<>?:')")
            (@arg min: -m --min +takes_value
                "Drop symbols and n-grams with lower count")
            (@arg pretty: --pretty
                "Pretty-print JSON output")
            (@arg input: -i --input +takes_value
                "Text or JSON file to use as input [stdin]")
        )
        (@subcommand anneal =>
            (about: "Generate a layout with Simulated Annealing")
            (version: "0.1")
            (@arg dir: -d --dir +takes_value +required
                "DB and configuration directory")
            (@arg config: -c --config +takes_value
                "Configuration file [<dir>/config.toml]")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input\n[stdin if not specified here or in <config>]")
            (@arg layout: -l --layout +takes_value
                "Initial layout filename [QWERTY]")
            (@arg noshuffle: --("no-shuffle")
                "Don't shuffle initial layout")
            (@arg steps: -s --steps +takes_value
                "Steps per annealing iteration [10000]")
            (@arg number: -n --number +takes_value
                "Number of layouts to generate [1]")
            (@arg jobs: -j --jobs +takes_value
                "Number of jobs (threads) to run concurrently [1]")
            (@arg progress: -p --progress
                "Print layouts in progress")
            (@arg show_scores: --("show-scores")
                "Print scores instead of letter and n-gram counts")
        )
        (@subcommand eval =>
            (about: "Evaluate layouts")
            (version: "0.1")
            (@arg config: -c --config +takes_value
                "Configuration file")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input\n[stdin if not specified here or in <config>]")
            (@arg verbose: -v --verbose
                "Print extra information for each layout")
            (@arg LAYOUT: +multiple +required
                "Layout to evaluate")
            (@arg show_scores: --("show-scores")
                "Print scores instead of letter and n-gram counts")
        )
        (@subcommand rank =>
            (about: "Rank layouts")
            (version: "0.1")
            (@arg dir: -d --dir +takes_value +required
                "DB and configuration directory")
            (@arg config: -c --config +takes_value
                "Configuration file [<dir>/config.toml]")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input\n[stdin if not specified here or in <config>]")
            (@arg number: -n --number +takes_value
                "Number of top-ranked layouts to output")
            (@arg scores: -s --scores +takes_value
                "Comma-separated list of scores to rank layouts by")
            (@arg show_scores: --("show-scores")
                "Print scores instead of letter and n-gram counts")
        )
        (@subcommand stats =>
            (about: "Print population statistics")
            (version: "0.1")
            (@arg dir: -d --dir +takes_value +required
                "DB and configuration directory")
            (@arg config: -c --config +takes_value
                "Configuration file [<dir>/config.toml]")
            (@arg text: -t --text +takes_value
                "Text or JSON file to use as input\n[stdin if not specified here or in <config>]")
            (@arg scores: -s --scores +takes_value
                "Comma-separated list of scores to rank layouts by")
        )
    ).get_matches();

    match app_m.subcommand_name() {
        Some("anneal") => anneal_command(app_m.subcommand_matches("anneal")
                                              .unwrap()),
        Some("eval") => eval_command(app_m.subcommand_matches("eval")
                                          .unwrap()),
        Some("rank") => rank_command(app_m.subcommand_matches("rank")
                                              .unwrap()),
        Some("stats") => stats_command(app_m.subcommand_matches("stats")
                                              .unwrap()),
        Some("corpus") => corpus_command(app_m.subcommand_matches("corpus")
                                                    .unwrap()),
        Some(unknown) => panic!("Unhandled subcommand: {}", unknown),
        None => {
            eprintln!("No subcommand given.\n{}", app_m.usage());
            process::exit(1)
        },
    }
}
