use kuehlmak::TextStats;
use kuehlmak::{
    layout_from_str, layout_distance, Layout,
    EvalModel, EvalScores,
    KuehlmakModel, KuehlmakParams,
    Anneal
};

use clap::{clap_app, ArgMatches};

use std::collections::VecDeque;
use std::path::{PathBuf, Path};
use std::str::FromStr;
use std::ffi::OsStr;
use std::process;
use std::env;
use std::io;
use std::fs;

static QWERTY: &str =
r#"q  w  e  r  t  y  u  i  o  p
   a  s  d  f  g  h  j  k  l ;:
   z  x  c  v  b  n  m ,< .> /?"#;

fn layout_from_file<P>(path: P) -> Layout
    where P: AsRef<Path> + Copy
{
    let layout = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Failed to read layout file '{}': {}",
                  path.as_ref().display(), e);
        process::exit(1)
    });
    layout_from_str(&layout).unwrap_or_else(|e| {
        eprintln!("Failed to parse layout: {}", e);
        process::exit(1)
    })
}

fn config_from_file<P>(path: P) -> KuehlmakParams
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
    let config = toml::from_str(&c).unwrap_or_else(|e| {
        eprintln!("Failed to parse config file '{}': {}",
                  path.as_ref().display(), e);
        process::exit(1)
    });
    env::set_current_dir(&prev_dir).expect("Failed to set current dir");
    config
}

fn text_from_file(filename: &str) -> TextStats {
    let contents = fs::read_to_string(filename).unwrap_or_else(|e| {
        eprintln!("Failed to read TEXT file '{}': {}", filename, e);
        process::exit(1)
    });
    if filename.ends_with(".json") {
        serde_json::from_str::<TextStats>(&contents).unwrap_or_else(|e| {
            eprintln!("Failed to parse JSON file '{}': {}", filename, e);
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

    // Won't panic because TEXT is mandatory
    let text_filename = sub_m.value_of("TEXT").unwrap();
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

    let kuehlmak_model = KuehlmakModel::new(config);
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

    // Won't panic because TEXT is mandatory
    let text_filename = sub_m.value_of("TEXT").unwrap();
    let text = text_from_file(text_filename);
    // Not filtering with any alphabet because different layouts may use
    // different alphabets.

    let verbose = sub_m.is_present("verbose");

    let kuehlmak_model = KuehlmakModel::new(config);
    let stdout = &mut io::stdout();

    for filename in sub_m.values_of("LAYOUT").into_iter().flatten() {
        let layout = layout_from_file(filename);

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

struct ScoreGroup<'a, S> (VecDeque<&'a S>);

impl<'a, S> ScoreGroup<'a, S>
    where S: EvalScores
{
    // fn new() -> Self {Self(VecDeque::new(),)}
    fn new_with(s: &'a S) -> Self {Self(VecDeque::from([s]),)}

    fn len(&self) -> usize {self.0.len()}
    fn is_empty(&self) -> bool {self.0.is_empty()}

    fn push_back(&mut self, s: &'a S) {self.0.push_back(s)}
    fn pop_front(&mut self) -> Option<&'a S> {self.0.pop_front()}

    fn iter(&'a self) -> std::collections::vec_deque::Iter<'a, &'a S> {
        self.0.iter()
    }

    fn min_max_distance(&self, s: &'a S) -> [f64; 2] {
        let layout = s.layout_ref();
        let mut max = 0.0f64;
        let mut min = 1.0f64;
        for entry in self.0.iter() {
            // Ignore entry if it's the same layout. We're probably looking
            // for a better group for it
            let entry = entry.layout_ref();
            if entry == layout {
                continue;
            }
            let d = layout_distance(layout, entry);
            assert!(d > 0.0 && d <= 1.0);
            min = min.min(d);
            max = max.max(d);
        }
        [min, max]
    }
}

fn add_layout_to_closest_group<'a, S>(groups: &mut VecDeque<ScoreGroup<'a, S>>,
                                      s: &'a S) -> usize
where S: EvalScores {
    // distances enumerates the closest and farthest distance of s
    // to any member of group groups[i]
    let distances: Vec<_> = groups.iter()
                                  .map(|g| g.min_max_distance(s))
                                  .collect();

    // Find a group where the farthest distance to any member is smaller
    // than the closest distance to any other group. So first find the
    // group with the smallest max-distance. Then compare that with all
    // the other min-distances
    let mut min_d = 1.0;
    let mut min_i = distances.len();
    for i in 0..distances.len() {
        if distances[i][1] < min_d {
            min_d = distances[i][1];
            min_i = i;
        }
    }
    if min_i < distances.len() {
        for i in 0..distances.len() {
            if i == min_i {continue;}
            if distances[i][0] < min_d {
                min_i = distances.len();
                break;
            }
        }
    }

    // If there is a matching group, add s to that group. Otherwise create
    // a new group for s
    if min_i < distances.len() {
        groups[min_i].push_back(s);
    } else {
        groups.push_back(ScoreGroup::new_with(s))
    }

    min_i
}

fn scores_into_groups<'a, S: EvalScores>(scores: &'a Vec<S>)
-> VecDeque<ScoreGroup<'a, S>> {
    scores.iter().map(|s| ScoreGroup::new_with(s)).collect()
}

fn regroup_scores<'a, S: EvalScores>(groups: &mut VecDeque<ScoreGroup<'a, S>>)
-> usize {
    let n_groups = groups.len();
    let mut n_changes = 0;

    for _i in 0..n_groups {
        // Always work with the first group. Groups are rotated to the back
        // at the end of the loop
        let g = groups.front().unwrap();
        let n_scores = g.len();
        let mut g_is_empty = false;

        for _j in 0..n_scores {
            // Pop the first member of the group, remove group if empty
            let g = groups.front_mut().unwrap();
            let s = g.pop_front().unwrap();

            assert!(!g_is_empty);
            g_is_empty = g.is_empty();
            if g_is_empty { // Can't use g after this
                groups.pop_front();
            }

            // This may add s back to the same group, a different group,
            // or even a new group at the back of groups
            let new_index = add_layout_to_closest_group(groups, s);
            // Count changes. As the grouping converges that number should
            // approach 0
            if !g_is_empty && new_index > 0 ||
                g_is_empty && new_index < groups.len()-1 {
                n_changes += 1;
            }
        }

        // If the group was empty, it was already removed. Otherwise move
        // it to the back
        if !g_is_empty {
            groups.rotate_left(1);
        }
    }

    n_changes
}

fn choose_command(sub_m: &ArgMatches) {
    let mut config: Option<KuehlmakParams> = None;
    let mut layouts: Vec<Layout> = Vec::new();
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

    let percentile: usize = match sub_m.value_of("percentile") {
        Some(number) => number.parse().unwrap_or_else(|e| {
            eprintln!("Invalid number '{}': {}", number, e);
            process::exit(1)
        }),
        None => 100,
    };
    if percentile > 100 {
        eprintln!("Percentile must be <= 100.");
        process::exit(1);
    }

    // Won't panic because TEXT is mandatory
    let text_filename = sub_m.value_of("TEXT").unwrap();
    let text = text_from_file(text_filename);
    // Not filtering with any alphabet because different layouts may use
    // different alphabets.

    let kuehlmak_model = KuehlmakModel::new(config);

    let mut scores: Vec<_> = layouts.iter().map(
        |l| kuehlmak_model.eval_layout(l, &text, 1.0)).collect();

    // Sort scores and keep the <percentile>% best layouts
    scores.sort_by(|a, b| a.total().partial_cmp(&b.total()).unwrap());
    let keep = scores.len() * percentile / 100;
    scores.truncate(keep);

    // Group layouts
    let mut groups = scores_into_groups(&scores);
    regroup_scores(&mut groups);
    regroup_scores(&mut groups);
    regroup_scores(&mut groups);
    println!("{} layouts in {} groups", scores.len(), groups.len());

    let stdout = &mut io::stdout();
    for (i, g) in groups.iter().enumerate() {
        println!("=== Group {} =============================", i);
        for &s in g.iter() {
            s.write(stdout).unwrap();
            println!();
        }
    }
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
                "Initial layout filename [QWERTY]")
            (@arg noshuffle: -n --("no-shuffle")
                "Don't shuffle initial layout")
            (@arg steps: -s --steps +takes_value
                "Steps per annealing iteration [10000]")
            (@arg TEXT: +required
                "Text or JSON file to use as input")
        )
        (@subcommand choose =>
            (about: "Choose a layout")
            (version: "0.1")
            (@arg dir: -d --dir +takes_value +required
                "DB and configuration directory")
            (@arg percentile: -p --percentile +takes_value
                "Top percentile of layouts to consider")
            (@arg TEXT: +required
                "Text or JSON file to use as input")
        )
        (@subcommand eval =>
            (about: "Evaluate layouts")
            (version: "0.1")
            (@arg config: -c --config +takes_value
                "Configuration file")
            (@arg verbose: -v --verbose
                "Print extra information for each layout")
            (@arg TEXT: +required
                "Text or JSON file to use as input")
            (@arg LAYOUT: +multiple
                "Layout to evaluate")
        )
    ).get_matches();

    match app_m.subcommand_name() {
        Some("anneal") => anneal_command(app_m.subcommand_matches("anneal")
                                              .unwrap()),
        Some("choose") => choose_command(app_m.subcommand_matches("choose")
                                              .unwrap()),
        Some("eval") => eval_command(app_m.subcommand_matches("eval")
                                          .unwrap()),
        Some(unknown) => panic!("Unhandled subcommand: {}", unknown),
        None => {
            eprintln!("No subcommand given.\n{}", app_m.usage());
            process::exit(1)
        },
    }
}
