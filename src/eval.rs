use super::{TextStats, Bigram, Trigram};
use std::fs::OpenOptions;
use std::io::{self, BufWriter};
use std::io::Write as IoWrite;
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;
use std::ops::Mul;
use serde::{Serialize, Deserialize};

// Layout: 2 chars per key (normal/shifted), 10 keys per row, 3 rows
pub type Layout = [[char; 2]; 30];

pub fn layout_from_str(text: &str) -> Result<Layout, String> {
    let mut layout: Layout = [[' '; 2]; 30];

    let mut last_line = 0;
    for (l, line) in text.lines().enumerate().take(3) {
        last_line = l;

        let mut last_key = 0;
        for (k, key) in line.split_whitespace().enumerate() {
            if k >= 10 {
                return Err(format!(
                    "Too many keys on row {}. Expected 10 keys per row",
                    l + 1));
            }
            last_key = k;

            let k = l * 10 + k;
            let mut last_char = 0;
            for (i, c) in key.chars().enumerate() {
                if i >= 2 {
                    return Err(format!(
                        "Too many characters on row {}, key {}. Expected 1 or 2 characters per key",
                       l, last_key));
                }
                last_char = i;

                layout[k][i] = c;
            }
            if last_char == 0 {
                let c = layout[k][0];
                if !c.is_alphabetic()
                    || c.to_lowercase().count() != 1
                    || c.to_uppercase().count() != 1 {
                    return Err(format!(
                        "Automatic case conversion failed for '{}' at row {}, key {}",
                        c, l, last_key));
                }
                layout[k][0] = c.to_lowercase().next().unwrap();
                layout[k][1] = c.to_uppercase().next().unwrap();
            } else {
                assert!(last_char == 1);
            }
        }
        if last_key+1 < 10 {
            return Err(format!(
                "Found only {} keys in row {}. Expected 10 keys per row",
                last_key+1, last_line));
        }
    }
    if last_line+1 < 3 {
        return Err(format!("Found only {} rows. Expected 3 rows",
                           last_line+1));
    }
    Ok(layout)
}

pub fn layout_to_str(layout: &Layout) -> String {
    let mut s = String::new();
    let mut keys = layout.iter();
    let mut write10keys = |s: &mut String|
        keys.by_ref().map(|&[a, b]| match b.to_lowercase().next() {
            Some(l) if l == a => write!(s, "  {}", a),
            _                 => write!(s, " {}{}", a, b),
        }).take(10).fold(Ok(()), fmt::Result::and).unwrap();

    write10keys(&mut s);
    writeln!(s).unwrap();
    write10keys(&mut s);
    writeln!(s).unwrap();
    write10keys(&mut s);
    writeln!(s).unwrap();
    s
}

pub fn layout_to_filename(layout: &Layout) -> PathBuf {
    let mut s = String::new();
    for &[a, _] in layout {
        // Some substitutions for characters that don't work well in
        // file names on some OSes.
        s.push(match a {
            '/' => 'Z',
            '?' => 'S',
            '<' => 'L',
            '>' => 'G',
            ':' => 'I',
            ';' => 'J',
            '\\' => 'X',
            '|' => 'T',
            '.' => 'O',
            ',' => 'Q',
            '\'' => 'V',
            '"' => 'W',
            _ => a,
        });
    }
    s.push_str(".kbl");
    PathBuf::from(s)
}

mod serde_layout {
    use std::fs;
    use std::fmt;
    use serde::{Serializer, Deserializer, de, de::Visitor, de::Unexpected};
    use super::{Layout, layout_to_str, layout_from_str};

    pub fn serialize<S>(layout: &Option<Layout>, ser: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match layout {
            Some(layout) => ser.serialize_str(&layout_to_str(layout)),
            None => ser.serialize_none(),
        }
    }

    struct LayoutVisitor;
    impl<'de> Visitor<'de> for LayoutVisitor {
        type Value = Option<Layout>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a layout filname or inline definition")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where E: de::Error {
            if s.lines().count() >= 3 { // Try to parse it as an inline layout
                match layout_from_str(s) {
                    Ok(layout) => Ok(Some(layout)),
                    Err(_) => Err(de::Error::invalid_value(Unexpected::Str(s),
                                                           &self)),
                }
            } else {
                match fs::read_to_string(s) {
                    Ok(string) => match layout_from_str(&string) {
                        Ok(layout) => Ok(Some(layout)),
                        Err(_) => Err(de::Error::invalid_value(
                                      Unexpected::Str(&string), &self)),
                    },
                    Err(_) => Err(de::Error::invalid_value(Unexpected::Str(s),
                                                           &self)),
                }
            }
        }
    }

    pub fn deserialize<'de, D>(des: D) -> Result<Option<Layout>, D::Error>
    where D: Deserializer<'de> {
        des.deserialize_str(LayoutVisitor)
    }
}

// How different are two layouts? Count how many symbols are on the same
// key, finger and hand to make up a score between 0 (identical) and
// 1 (as different as it gets).
#[allow(clippy::comparison_chain)]
fn layout_distance(a: &Layout, b: &Layout) -> f64 {
    // Build indexed arrays of the lower-case symbols of both layouts
    let mut i = 0usize;
    let mut c = || {i += 1; ((i-1) as u32, a[i-1][0])};
    let mut a = [c(), c(), c(), c(), c(), c(), c(), c(), c(), c(),
                 c(), c(), c(), c(), c(), c(), c(), c(), c(), c(),
                 c(), c(), c(), c(), c(), c(), c(), c(), c(), c()];
    let mut i = 0usize;
    let mut c = || {i += 1; ((i-1) as u32, b[i-1][0])};
    let mut b = [c(), c(), c(), c(), c(), c(), c(), c(), c(), c(),
                 c(), c(), c(), c(), c(), c(), c(), c(), c(), c(),
                 c(), c(), c(), c(), c(), c(), c(), c(), c(), c()];

    // Sort them by symbol. That makes the rest of this function O(n)
    a.sort_by_key(|x| x.1);
    b.sort_by_key(|x| x.1);

    // Iterate over both array, evaluate distance of matching symbols
    let mut i = 0;
    let mut j = 0;
    let mut distance = 120;
    while i < 30 && j < 30 {
        // If the symbols don't match, advance the array with the smaller
        // symbol to try to resync them and find all matches
        if a[i].1 < b[j].1 {
            i += 1;
            continue;
        } else if a[i].1 > b[j].1 {
            j += 1;
            continue;
        }
        // Symbols match, adjust distance based on the indexes
        if a[i].0 == b[j].0 {
            distance -= 4; // same key
        } else {
            let finger = |key| {
                let col = key % 10;
                if col < 4 {col} else if col < 6 {col - 1} else {col - 2}
            };
            if finger(a[i].0) == finger(b[j].0) {
                distance -= 2;
            } else {
                let hand = |k| if k % 10 < 5 {0} else {1};
                if hand(a[i].0) == hand(b[j].0) {
                    distance -= 1;
                }
            }
        }
        i += 1;
        j += 1;
    }
    distance as f64 / 120.0
}

// Mirror a key from left to right hand or vice versa
fn mirror_key(k: u8) -> u8
{
    k + 9 - 2 * (k % 10)
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum KeyboardType {
    Ortho,
    ColStag,
    Hex,
    HexStag,
    ANSI,
    ISO,
}

#[derive(Clone, Copy)]
struct KeyProps {
    hand: u8,
    finger: u8,
    is_stretch: bool,
    d_abs: f32,
    d_rel: [f32; 30],
    cost: u16,
}

pub trait EvalScores {
    fn write<W>(&self, w: &mut W) -> io::Result<()>
        where W: IoWrite;
    fn write_extra<W>(&self, w: &mut W) -> io::Result<()>
        where W: IoWrite;
    fn layout(&self) -> Layout;
    fn total(&self) -> f64;

    fn get_scores(&self) -> Vec<f64>;
    fn get_score_names() -> BTreeMap<String, usize>;

    fn write_to_db(&self, dir: &Path) -> io::Result<()> {
        let path: PathBuf =
            [dir, &layout_to_filename(&self.layout())].iter().collect();
        if let Ok(file) = OpenOptions::new()
                .append(true).create_new(true).open(&path) {
            // The file didn't exist. Write the layout and scores.
            // The number of #'s on the last line counts how often the
            // layout was found.
            let mut w = BufWriter::new(file);

            w.write_all(layout_to_str(&self.layout()).as_bytes())?;
            self.write(&mut w)?;
            self.write_extra(&mut w)?;
            write!(w, "#")?;

            w.flush()
        } else {
            // The file exists. Append one more #.
            let mut file = OpenOptions::new().append(true).open(&path)?;

            write!(file, "#")
        }
    }
}

// Keyboard evaluation model that can be reused for evaluating multiple
// keyboard layouts of the same type.
pub trait EvalModel<'a> {
    type Scores: EvalScores + Clone;

    fn eval_layout(&'a self, layout: &Layout, ts: &TextStats,
                   precision: f64) -> Self::Scores;
    fn key_cost_ranking(&'a self) -> &'a [usize; 30];
    fn is_symmetrical(&'a self) -> bool;
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KuehlmakParams {
    board_type: KeyboardType,
    weights: KuehlmakWeights,
    constraints: ConstraintParams,
}

impl Default for KuehlmakParams {
    fn default() -> Self {
        KuehlmakParams {
            board_type: KeyboardType::Ortho,
            weights: KuehlmakWeights::default(),
            constraints: ConstraintParams::default(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default,deny_unknown_fields)]
pub struct KuehlmakWeights {
    index_finger: u8,
    middle_finger: u8,
    ring_finger: u8,
    pinky_finger: u8,
    effort: f64,
    travel: f64,
    imbalance: f64,
    drolls: f64,
    urolls: f64,
    #[serde(rename = "WLSBs")]
    wlsbs: f64,
    scissors: f64,
    #[serde(rename = "SFBs")]
    sfbs: f64,
    d_drolls: f64,
    d_urolls: f64,
    #[serde(rename = "dWLSBs")]
    d_wlsbs: f64,
    d_scissors: f64,
    #[serde(rename = "dSFBs")]
    d_sfbs: f64,
    rrolls: f64,
    redirects: f64,
    contorts: f64,
}

impl Default for KuehlmakWeights {
    fn default() -> Self {
        KuehlmakWeights {
            index_finger:  1,
            middle_finger: 1,
            ring_finger:   2,
            pinky_finger:  5,
            effort:        0.1,
            travel:        1.0,
            imbalance:     0.05,
            drolls:       -0.5, // better than hand alternation
            urolls:        0.0, // same as alternation (which is not scored)
            wlsbs:         1.0,
            scissors:      5.0,
            sfbs:          5.0,
            d_drolls:     -0.5,
            d_urolls:      0.0,
            d_wlsbs:       1.0,
            d_scissors:    5.0,
            d_sfbs:        5.0,
            rrolls:       -0.5,
            redirects:     5.0,
            contorts:      5.0,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default,deny_unknown_fields)]
pub struct ConstraintParams {
    #[serde(with = "serde_layout")]
    ref_layout: Option<Layout>,
    ref_weight: f64,
    ref_threshold: f64,
    top_keys: Option<String>,
    mid_keys: Option<String>,
    bot_keys: Option<String>,
    homing_keys: Option<String>,
    homing_only_keys: Option<String>,
    top_weight: f64,
    mid_weight: f64,
    bot_weight: f64,
    homing_weight: f64,
    zxcv: f64,
    nonalpha: f64,
}

fn eval_constraints(layout: &Layout, params: &ConstraintParams) -> f64 {
    let mut score = match params.ref_layout.as_ref() {
        Some(ref_layout) if params.ref_weight != 0.0 =>
            (layout_distance(layout, ref_layout) - params.ref_threshold)
            .max(0.0) * (1.0 - params.ref_threshold) * params.ref_weight,
        _ => 0.0,
    };
    score += match params.top_keys.as_ref() {
        Some(keys) if params.top_weight != 0.0 =>
            eval_row(layout, 0, keys) * params.top_weight,
        _ => 0.0,
    };
    score += match params.mid_keys.as_ref() {
        Some(keys) if params.mid_weight != 0.0 =>
            eval_row(layout, 1, keys) * params.mid_weight,
        _ => 0.0,
    };
    score += match params.bot_keys.as_ref() {
        Some(keys) if params.bot_weight != 0.0 =>
            eval_row(layout, 2, keys) * params.bot_weight,
        _ => 0.0,
    };
    score += match params.homing_keys.as_ref() {
        Some(keys) if params.homing_weight != 0.0 =>
            eval_homing(layout, keys, params.homing_only_keys.as_ref()
                                                             .map(|s| &s[..]))
            * params.homing_weight,
        _ => 0.0,
    };
    if params.zxcv != 0.0 {
        score += params.zxcv * eval_zxcv(layout);
    }
    if params.nonalpha != 0.0 {
        score += params.nonalpha * eval_nonalpha(layout);
    }
    score
}

// ZXCV-constraint: Penalize xzcv keys that are not in the left hand
// bottom row. Being complete and in the right order gives one bonus point
fn eval_zxcv(layout: &Layout) -> f64 {
    let zxcv = ['z', 'x', 'c', 'v'];
    let mut found = [' ', ' ', ' ', ' '];
    let mut n = 0;

    for [c, _] in &layout[20..25] {
        if zxcv.contains(c) {
            found[n] = *c;
            n += 1;
        }
    }
    if zxcv == found {
        n += 1;
    }
    (5 - n) as f64 / 5.0
}

// Non-alpha constraint: Penalize alpha-keys in Colemak non-alpha positions.
// Using Colemak rather than QWERTY because non-alpha keys make no sense on
// the home row
fn eval_nonalpha(layout: &Layout) -> f64 {
    let mut n = if layout[9][0].is_alphabetic() {1} else {0};

    n += layout[27..30].iter().filter(|[c, _]| c.is_alphabetic()).count();
    n as f64 / 4.0
}

// Per-row keycap constraints to evaluate, whether a layout can be built with
// a given set of keycaps
fn eval_row(layout: &Layout, row: usize, keys: &str) -> f64 {
    layout[row*10..(row+1)*10].iter().filter(|&[c, _]| keys.contains(*c))
                              .count() as f64 / -10.0 + 1.0
}
// Homing key constraint. Checks whether homing keys are available for either
// the index or middle finger and returns the better of the two options.
// Optionally a set of homing-only keys can be given. These keys must be on
// a homing position if they are on the home row because they are only
// available as homing keys.
fn eval_homing(layout: &Layout, keys: &str, homing_only_keys: Option<&str>)
        -> f64 {
    let index  = keys.contains(layout[13][0]) as u8
               + keys.contains(layout[16][0]) as u8;
    let middle = keys.contains(layout[12][0]) as u8
               + keys.contains(layout[17][0]) as u8;
    let mut homing_finger = 0u8;
    let mut homing_only_wrong = false;

    if let Some(keys) = homing_only_keys {
        for key in keys.chars() {
            if let Some(p) = layout[10..20].iter()
                                           .position(|&[c, _]| c == key) {
                if p == 3 || p == 6 {
                    if homing_finger == 0 {
                        homing_finger = 1;
                    } else if homing_finger != 1 {
                        homing_only_wrong = true;
                        break;
                    }
                } else if p == 2 || p == 7 {
                    if homing_finger == 0 {
                        homing_finger = 2;
                    } else if homing_finger != 2 {
                        homing_only_wrong = true;
                        break;
                    }
                } else {
                    homing_only_wrong = true;
                    break;
                }
            }
        }
    }

    (if homing_finger == 0 {
        2 - index.max(middle)
    } else if homing_finger == 1 {
        2 - index
    } else {
        2 - middle
    } + homing_only_wrong as u8) as f64 / 3.0
}

#[derive(Clone)]
pub struct KuehlmakScores<'a> {
    model: &'a KuehlmakModel,
    layout: Layout,
    token_keymap: Vec<u8>,
    strokes: u64,
    heatmap: [u64; 30],
    bigram_counts: [u64; BIGRAM_NUM_TYPES],
    trigram_counts: [u64; TRIGRAM_NUM_TYPES],
    bigram_lists: [Option<Vec<(Bigram, u64)>>; BIGRAM_NUM_TYPES],
    trigram_lists: [Option<Vec<(Trigram, u64)>>; TRIGRAM_NUM_TYPES],
    finger_travel: [f64; 8],
    urolls: f64,
    wlsbs: f64,
    d_urolls: f64,
    d_wlsbs: f64,
    redirects: u64,
    contorts: u64,
    effort: f64,
    travel: f64,
    imbalance: f64,
    hand_runs: [f64; 2],
    total: f64,
    constraints: f64,
}

#[derive(Clone)]
pub struct KuehlmakModel {
    params: KuehlmakParams,
    key_props: [KeyProps; 30],
    bigram_types: [[u8; 30]; 30],
    trigram_types: [[[u8; 30]; 30]; 30],
    key_cost_ranking: [usize; 30],
}

impl<'a> EvalScores for KuehlmakScores<'a> {
    fn write<W>(&self, w: &mut W) -> io::Result<()>
    where W: IoWrite {
        let norm = 1000.0 / self.strokes as f64;
        let mut fh = [0u64; 8];
        for (&count, props) in
                self.heatmap.iter().zip(self.model.key_props.iter()) {
            fh[props.finger as usize] += count;
        }
        let mut fh_iter = fh.iter().map(|&h| h as f64 * norm);
        let mut hh_iter = fh.chunks(4)
                            .map(|s| s.iter().sum::<u64>() as f64 * norm);
        let mut ft_iter = self.finger_travel.iter().map(|&t| t * norm);
        let mut ht_iter = self.finger_travel.chunks(4)
                              .map(|s| s.iter().sum::<f64>() * norm);
        let travel = self.finger_travel.iter().sum::<f64>() * norm;

        let key_space = match self.model.params.board_type {
                KeyboardType::Ortho | KeyboardType::ColStag =>
                    [["", "  |  ", ""]; 3],
                KeyboardType::Hex | KeyboardType::HexStag  =>
                    [["", "  |  ", ""],
                     ["  ", "|", "  "],
                     ["", "  |  ", ""]],
                KeyboardType::ANSI =>
                    [[" ", "", "    "],
                     ["  ", "", "   "],
                     ["    ", "", " "]],
                KeyboardType::ISO =>
                    [[" ", " ", "   "],
                     ["  ", " ", "  "],
                     ["", "     ", ""]],
            };

        let mut layout_iter = self.layout().into_iter();
        let mut write_5keys = |w: &mut W|
            layout_iter.by_ref().take(5)
                       .map(|[a, b]| match b.to_lowercase().next() {
                           Some(l) if l == a => write!(w, "[ {}]", a),
                           _                 => write!(w, "[{}{}]", a, b),
                       }).fold(Ok(()), io::Result::and);
        let mut write_key_row = |w: &mut W, [prefix,sep,suffix]: [&str; 3]| {
            w.write_all(prefix.as_bytes())?;
            write_5keys(w)?;
            w.write_all(sep.as_bytes())?;
            write_5keys(w)?;
            writeln!(w, "{}", suffix)
        };

        let mut heat_iter = self.heatmap.iter();
        let mut write_5heats = |w: &mut W|
            heat_iter.by_ref().take(5)
                     .map(|&h| write!(w, "{:3.0} ", h as f64 * norm))
                     .fold(Ok(()), io::Result::and);
        let mut write_heat_row = |w: &mut W, [prefix,sep,suffix]: [&str; 3]| {
            w.write_all(prefix.as_bytes())?;
            write_5heats(w)?;
            w.write_all(sep.as_bytes())?;
            write_5heats(w)?;
            writeln!(w, "{}", suffix)
        };

        write!(w, "Effort {:6.4} Imbalance {:6.2}%   |",
               self.effort, self.imbalance * 100.0)?;
        write!(w, "{:3.0}+{:3.0}+{:3.0}+{:3.0}=  {:3.0} |",
               fh_iter.next().unwrap(), fh_iter.next().unwrap(),
               fh_iter.next().unwrap(), fh_iter.next().unwrap(),
               hh_iter.next().unwrap())?;
        writeln!(w, " {:3.0} ={:3.0}+{:3.0}+{:3.0}+{:3.0} ",
                 hh_iter.next().unwrap(),
                 fh_iter.next().unwrap(), fh_iter.next().unwrap(),
                 fh_iter.next().unwrap(), fh_iter.next().unwrap())?;

        write!(w, "Travel {:6.4} ({:7.2})           |",
               self.travel, travel)?;
        write!(w, "{:3.0}+{:3.0}+{:3.0}+{:3.0}= {:4.0} |",
               ft_iter.next().unwrap(), ft_iter.next().unwrap(),
               ft_iter.next().unwrap(), ft_iter.next().unwrap(),
               ht_iter.next().unwrap())?;
        writeln!(w, "{:4.0} ={:3.0}+{:3.0}+{:3.0}+{:3.0} ",
                 ht_iter.next().unwrap(),
                 ft_iter.next().unwrap(), ft_iter.next().unwrap(),
                 ft_iter.next().unwrap(), ft_iter.next().unwrap())?;

        write!(w, "    DRoll URoll  WLSB Scissor SFB |")?;
        write_key_row(w, key_space[0])?;

        write!(w, " AB {:5.1} {:5.1} {:5.1} {:5.1} {:5.1} |",
               self.bigram_counts[BIGRAM_DROLL] as f64 * norm,
               self.urolls * norm,
               self.wlsbs * norm,
               self.bigram_counts[BIGRAM_SCISSOR] as f64 * norm,
               self.bigram_counts[BIGRAM_SFB] as f64 * norm)?;
        write_heat_row(w, key_space[0])?;

        write!(w, "A_B {:5.1} {:5.1} {:5.1} {:5.1} {:5.1} |",
               self.trigram_counts[TRIGRAM_D_DROLL] as f64 * norm,
               self.d_urolls * norm,
               self.d_wlsbs * norm,
               self.trigram_counts[TRIGRAM_D_SCISSOR] as f64 * norm,
               self.trigram_counts[TRIGRAM_D_SFB] as f64 * norm)?;
        write_key_row(w, key_space[1])?;

        write!(w, "    RRoll Redir Contort  Runs L:R |")?;
        write_heat_row(w, key_space[1])?;

        write!(w, "ABC {:5.1} {:5.1} {:5.1}   {:4.2}:{:4.2} |",
               self.trigram_counts[TRIGRAM_RROLL] as f64 * norm,
               self.redirects as f64 * norm,
               self.contorts as f64 * norm,
               self.hand_runs[0], self.hand_runs[1])?;
        write_key_row(w, key_space[2])?;

        write!(w, "Score+Con{:7.4}{:+8.4} ={:7.4} |",
               self.total, self.constraints, self.total + self.constraints)?;
        write_heat_row(w, key_space[2])?;

        Ok(())
    }

    fn write_extra<W>(&self, w: &mut W) -> io::Result<()>
    where W: IoWrite {
        let norm = 1000.0 / self.strokes as f64;
        let is_side = |side, c| self.layout().iter()
                                    .position(|&[l, u]| l == c || u == c)
                                    .unwrap() % 10 / 5 == side;
        let write_2gram_freqs = |w: &mut W, vec: &Vec<(Bigram, u64)>, side|
                -> io::Result<f64> {
            let mut sum = 0.0;
            for &(ngram, num) in vec.iter().filter(|&(ngram, _)|
                                                   is_side(side, ngram[0])) {
                let p = num as f64 * norm;
                sum += p;
                if p >= 0.005 {
                    write!(w, " {}{}:{:.2}", ngram[0], ngram[1], p)?;
                }
            }
            Ok(sum)
        };

        let bigram_names = ["", "SameKey", "DRolls", "URolls",
            "LSB3s (count as 1/3 WLSBs, 2/3 URolls)",
            "LSB2s (count as 1/2 WLSBs, 1/2 URolls)",
            "LSB1s", "Scissors", "SFBs"];
        for (vec, name) in self.bigram_lists.iter()
                               .zip(bigram_names.into_iter())
                               .filter_map(|(vec, name)|
                                    vec.as_ref().map(|vec| (vec, name))) {
            writeln!(w)?;
            writeln!(w, "{}:", name)?;
            write!(w, " Left hand:")?;
            let left_sum = write_2gram_freqs(w, vec, 0)?;
            writeln!(w)?;
            write!(w, "Right hand:")?;
            let right_sum = write_2gram_freqs(w, vec, 1)?;
            writeln!(w)?;
            write!(w, "Balance: {:.2}:{:.2}", left_sum, right_sum)?;
            writeln!(w)?;
        }

        let write_3gram_freqs = |w: &mut W, vec: &Vec<(Trigram, u64)>, side|
                -> io::Result<f64> {
            let mut sum = 0.0;
            for &(ngram, num) in vec.iter().filter(|&(ngram, _)|
                                                   is_side(side, ngram[0])) {
                let p = num as f64 * norm;
                sum += p;
                if p >= 0.005 {
                    write!(w, " {}{}{}:{:.2}",
                           ngram[0], ngram[1], ngram[2], p)?;
                }
            }
            Ok(sum)
        };

        let trigram_names = ["",
            "dSameKey", "shdSameKey (count as Redirects)",
            "dSFBs", "shdSFBs (count as Contorts)", "dDRolls", "dURolls",
            "dLSB3s (count as 1/3 dWLSBs, 2/3 dUROLLS)",
            "dLSB2s (count as 1/2 dWLSBs, 1/2 dURolls)",
            "dLSB1s", "dScissors", "RRolls", "Redirects", "Contortions"];
        for (vec, name) in self.trigram_lists.iter()
                               .zip(trigram_names.into_iter())
                               .filter_map(|(vec, name)|
                                    vec.as_ref().map(|vec| (vec, name))) {
            writeln!(w)?;
            writeln!(w, "{}:", name)?;
            write!(w, " Left hand:")?;
            let left_sum = write_3gram_freqs(w, vec, 0)?;
            writeln!(w)?;
            write!(w, "Right hand:")?;
            let right_sum = write_3gram_freqs(w, vec, 1)?;
            writeln!(w)?;
            write!(w, "Balance: {:.2}:{:.2}", left_sum, right_sum)?;
            writeln!(w)?;
        }

        Ok(())
    }

    fn layout(&self) -> Layout {
        if self.model.is_symmetrical() {
            if let Some(i) = self.layout.iter()
                                 .position(|&[l, u]| l == '.' || u == '.') {
                if i % 10 < 5 {
                    let mut layout = self.layout;

                    layout[0..10].reverse();
                    layout[10..20].reverse();
                    layout[20..30].reverse();

                    return layout;
                }
            }
        }
        self.layout
    }
    fn total(&self) -> f64 {self.total + self.constraints}

    fn get_scores(&self) -> Vec<f64> {
        vec![
            self.total,
            self.constraints,
            self.effort,
            self.travel,
            self.imbalance,
            self.bigram_counts[BIGRAM_DROLL] as f64,
            self.urolls,
            self.wlsbs,
            self.bigram_counts[BIGRAM_SCISSOR] as f64,
            self.bigram_counts[BIGRAM_SFB] as f64,
            self.trigram_counts[TRIGRAM_D_DROLL] as f64,
            self.d_urolls,
            self.d_wlsbs,
            self.trigram_counts[TRIGRAM_D_SCISSOR] as f64,
            self.trigram_counts[TRIGRAM_D_SFB] as f64,
            self.trigram_counts[TRIGRAM_RROLL] as f64,
            self.redirects as f64,
            self.contorts as f64,
        ]
    }
    fn get_score_names() -> BTreeMap<String, usize> {
        BTreeMap::from([
            ("total".to_string(), 0),
            ("constraints".to_string(), 1),
            ("effort".to_string(), 2),
            ("travel".to_string(), 3),
            ("imbalance".to_string(), 4),
            ("drolls".to_string(), 5),
            ("urolls".to_string(), 6),
            ("WLSBs".to_string(), 7),
            ("scissors".to_string(), 8),
            ("SFBs".to_string(), 9),
            ("d_drolls".to_string(), 10),
            ("d_urolls".to_string(), 11),
            ("dWLSBs".to_string(), 12),
            ("d_scissors".to_string(), 13),
            ("dSFBs".to_string(), 14),
            ("rrolls".to_string(), 15),
            ("redirects".to_string(), 16),
            ("contorts".to_string(), 17),
        ])
    }
}

impl<'a> EvalModel<'a> for KuehlmakModel {
    type Scores = KuehlmakScores<'a>;

    fn eval_layout(&'a self, layout: &Layout, ts: &TextStats,
                   precision: f64) -> Self::Scores {
        let bl = || if precision >= 1.0 {Some(vec![])} else {None};
        let tl = || if precision >= 1.0 {Some(vec![])} else {None};
        let mut scores = KuehlmakScores {
            model: self,
            layout: *layout,
            constraints: eval_constraints(layout, &self.params.constraints),
            token_keymap: Vec::new(),
            strokes: 0,
            heatmap: [0; 30],
            bigram_counts: [0; BIGRAM_NUM_TYPES],
            trigram_counts: [0; TRIGRAM_NUM_TYPES],
            bigram_lists: [None, bl(), bl(), bl(), bl(), bl(), bl(), bl(), bl()],
            trigram_lists: [None, tl(), tl(), tl(), tl(), tl(), tl(), tl(), tl(), tl(), tl(), tl(), tl(), tl()],
            finger_travel: [0.0; 8],
            urolls: 0.0,
            wlsbs: 0.0,
            d_urolls: 0.0,
            d_wlsbs: 0.0,
            redirects: 0,
            contorts: 0,
            effort: 0.0,
            travel: 0.0,
            imbalance: 0.0,
            hand_runs: [0.0; 2],
            total: 0.0,
        };

        scores.token_keymap.resize(ts.token_base(), u8::MAX);
        for (k, symbols) in layout.iter().enumerate() {
            for &(count, token) in
                    symbols.iter().filter_map(|s| ts.get_symbol([*s])) {
                scores.token_keymap[token] = k as u8;
                scores.heatmap[k] += count;
                scores.strokes += count;
            }
        }

        self.calc_effort(&mut scores);
        self.calc_ngrams(ts, &mut scores, 0.9 + precision * 0.1);
        self.score_travel(&mut scores);
        self.score_imbalance(&mut scores);

        let strokes = scores.strokes as f64;
        scores.total = [
            (self.params.weights.effort, scores.effort),
            (self.params.weights.travel, scores.travel),
            (self.params.weights.imbalance, scores.imbalance),
            (self.params.weights.drolls / strokes,
             scores.bigram_counts[BIGRAM_DROLL] as f64),
            (self.params.weights.urolls / strokes, scores.urolls),
            (self.params.weights.wlsbs / strokes, scores.wlsbs),
            (self.params.weights.scissors / strokes,
             scores.bigram_counts[BIGRAM_SCISSOR] as f64),
            (self.params.weights.sfbs / strokes,
             scores.bigram_counts[BIGRAM_SFB] as f64),
            (self.params.weights.d_drolls / strokes,
             scores.trigram_counts[TRIGRAM_D_DROLL] as f64),
            (self.params.weights.d_urolls / strokes, scores.d_urolls),
            (self.params.weights.d_wlsbs / strokes, scores.d_wlsbs),
            (self.params.weights.d_scissors / strokes,
             scores.trigram_counts[TRIGRAM_D_SCISSOR] as f64),
            (self.params.weights.d_sfbs / strokes,
             scores.trigram_counts[TRIGRAM_D_SFB] as f64),
            (self.params.weights.rrolls / strokes,
             scores.trigram_counts[TRIGRAM_RROLL] as f64),
            (self.params.weights.redirects / strokes,
             scores.redirects as f64),
            (self.params.weights.contorts / strokes,
             scores.contorts as f64),
        ].into_iter().map(|(score, weight)| score * weight).sum::<f64>();

        scores
    }
    fn key_cost_ranking(&'a self) -> &'a [usize; 30] {&self.key_cost_ranking}
    fn is_symmetrical(&'a self) -> bool {
        match self.params.board_type {
            KeyboardType::ISO => false,
            KeyboardType::ANSI => false,
            _ => self.params.constraints.ref_layout == None &&
                 self.params.constraints.zxcv == 0.0 &&
                 self.params.constraints.nonalpha == 0.0,
        }
    }
}

impl KuehlmakModel {
    fn calc_effort(&self, scores: &mut KuehlmakScores) {
        // Simple effort model
        //
        // Keys have a cost of use (depending on the strength of the
        // finger, key reachability).
        //
        // The effort for each finger is calculated by summing the key
        // costs multiplied by their usage frequncy from the heatmap.
        //
        // To simulate finger fatigue, the effort for each finger is
        // squared. 2x the finger use means 4x the effort.
        //
        // The total effort is calculated by summing up the effort of all
        // fingers. The Square root is taken to undo the fatique-square.
        // This brings the numbers into a more manageable range and
        // increases sensitivity of the fitness function. In an imbalanced
        // keyboard layout, the effort will be dominated by the most
        // heavily overused fingers.
        let mut finger_cost = [0.0; 8];
        for (&count, props) in
                scores.heatmap.iter().zip(self.key_props.iter()) {
            let f = props.finger as usize;
            finger_cost[f] += (count as f64) * (props.cost as f64);
        }
        scores.effort = finger_cost.into_iter()
                                   .map(|c| c * c)
                                   .sum::<f64>()
                                   .sqrt() / scores.strokes as f64;
    }

    fn calc_ngrams(&self, ts: &TextStats, scores: &mut KuehlmakScores,
                   precision: f64) {
        // Initial estimate of finger travel: from home position to key
        // neglecting the way back to home position, since that is just
        // relaxing the finger.
        //
        // For same-finger bigrams and 3-grams, correct this because there
        // is not enough time for the finger to return to the home position.
        // For bigrams, travel distance is from A to B. The same applies to
        // same-finger 3-grams if the middle key uses a different finger.
        //
        // Multiply the travel distance for same-finger bigrams and 3-grams
        // with a penalty factor that represents the finger travel speed
        // required.
        let mut hand_total = [0u64; 2];
        for (&count, props) in
                scores.heatmap.iter().zip(self.key_props.iter()) {
            scores.finger_travel[props.finger as usize] +=
                props.d_abs as f64 * count as f64;

            hand_total[props.hand as usize] += count;
        }
        let orig_finger_travel = scores.finger_travel;

        let percentile = (ts.total_bigrams() as f64 * precision) as u64;
        let mut total = 0;
        let mut same_hand = [0u64; 2];
        for &(bigram, count, token) in ts.iter_bigrams() {
            if total > percentile {
                break;
            }
            total += count;

            let [t0, t1, _] = ts.token_to_ngram(token);
            let k0 = scores.token_keymap[t0] as usize;
            let k1 = scores.token_keymap[t1] as usize;

            if k0 >= 30 || k1 >= 30 {
                continue;
            }

            let bigram_type = self.bigram_types[k0][k1] as usize;

            scores.bigram_counts[bigram_type] += count;
            if let Some(v) = scores.bigram_lists[bigram_type].as_mut() {
                v.push((bigram, count))
            }

            if bigram_type == BIGRAM_SFB || bigram_type == BIGRAM_SAMEKEY {
                // Correct travel estimate: going to k1 not from home
                // position but from k0 instead.
                let props = &self.key_props[k1];

                scores.finger_travel[props.finger as usize] +=
                    (props.d_rel[k0]*4.0 - props.d_abs) as f64 * count as f64;
            }

            if bigram_type != BIGRAM_ALTERNATE {
                same_hand[k0 % 10 / 5] += count;
            }
        }
        for count in scores.bigram_counts.iter_mut() {
            *count = *count * ts.total_bigrams() / total;
        }
        for (travel, orig) in scores.finger_travel.iter_mut()
                                    .zip(orig_finger_travel) {
            *travel += (*travel - orig) * (1.0 - precision);
        }
        let orig_finger_travel = scores.finger_travel;

        scores.urolls = scores.bigram_counts[BIGRAM_UROLL] as f64 +
                        scores.bigram_counts[BIGRAM_LSB2] as f64 / 2.0 +
                        scores.bigram_counts[BIGRAM_LSB3] as f64 * 2.0 / 3.0;
        scores.wlsbs = scores.bigram_counts[BIGRAM_LSB1] as f64 +
                       scores.bigram_counts[BIGRAM_LSB2] as f64 / 2.0 +
                       scores.bigram_counts[BIGRAM_LSB3] as f64 / 3.0;

        // Estimate same-hand runs as expected value of the geometic
        // distribution, which is 1 / "probability of switching hands".
        scores.hand_runs[0] = hand_total[0] as f64 /
                             (hand_total[0] - same_hand[0]) as f64;
        scores.hand_runs[1] = hand_total[1] as f64 /
                             (hand_total[1] - same_hand[1]) as f64;

        let precision = precision.powi(2);
        let percentile = (ts.total_trigrams() as f64 * precision) as u64;
        let mut total = 0;
        for &(trigram, count, token) in ts.iter_trigrams() {
            if total > percentile {
                break;
            }
            total += count;

            let [t0, t1, t2] = ts.token_to_ngram(token);
            let k0 = scores.token_keymap[t0] as usize;
            let k1 = scores.token_keymap[t1] as usize;
            let k2 = scores.token_keymap[t2] as usize;

            if k0 >= 30 || k1 >= 30 || k2 >= 30 {
                continue;
            }

            let trigram_type = self.trigram_types[k0][k1][k2] as usize;

            scores.trigram_counts[trigram_type] += count;
            if let Some(v) = scores.trigram_lists[trigram_type].as_mut() {
                v.push((trigram, count))
            }

            if trigram_type >= TRIGRAM_D_SAMEKEY &&
                    trigram_type <= TRIGRAM_SHD_SFB {
                // Correct travel estimate: going to k2 not from home
                // position but from k0 instead.
                let props = &self.key_props[k2];

                scores.finger_travel[props.finger as usize] +=
                    (props.d_rel[k0]*2.0 - props.d_abs) as f64 * count as f64;
            }
        }
        for count in scores.trigram_counts.iter_mut() {
            *count = *count * ts.total_trigrams() / total;
        }
        for (travel, orig) in scores.finger_travel.iter_mut()
                                    .zip(orig_finger_travel) {
            *travel += (*travel - orig) * (1.0 - precision);
        }

        scores.d_urolls = scores.trigram_counts[TRIGRAM_D_UROLL] as f64 +
                          scores.trigram_counts[TRIGRAM_D_LSB2] as f64 / 2.0 +
                          scores.trigram_counts[TRIGRAM_D_LSB3] as f64 * 2.0 / 3.0;
        scores.d_wlsbs = scores.trigram_counts[TRIGRAM_D_LSB1] as f64 +
                         scores.trigram_counts[TRIGRAM_D_LSB2] as f64 / 2.0 +
                         scores.trigram_counts[TRIGRAM_D_LSB3] as f64 / 3.0;
        scores.redirects = scores.trigram_counts[TRIGRAM_REDIRECT] +
                           scores.trigram_counts[TRIGRAM_SHD_SAMEKEY];
        scores.contorts = scores.trigram_counts[TRIGRAM_CONTORT] +
                          scores.trigram_counts[TRIGRAM_SHD_SFB];
    }

    fn score_travel(&self, scores: &mut KuehlmakScores) {
        // Weigh travel per finger by its finger weight. This penalizes travel
        // more heavily on weak fingers.
        //
        // Square the per-finger travel so the score is dominated by the
        // fingers that travel most. The square root of the sum brings
        // the value range back down and makes the score more sensitive.
        // (steeper slope for small values).
        //
        // The score is normalized so that on a perfectly balanced layout
        // it is close to the average per-key travel distance.
        let finger_weight = [
            self.params.weights.pinky_finger,
            self.params.weights.ring_finger,
            self.params.weights.middle_finger,
            self.params.weights.index_finger,
            self.params.weights.index_finger,
            self.params.weights.middle_finger,
            self.params.weights.ring_finger,
            self.params.weights.pinky_finger
        ];
        let norm = finger_weight.iter().map(|&w| (w as f64).recip().powi(2)).sum::<f64>();
        scores.travel = scores.finger_travel.iter().zip(finger_weight)
                              .map(|(&travel, w)| {
                                  let t = travel * w as f64;
                                  t * t
                              }).sum::<f64>().mul(norm).sqrt() / scores.strokes as f64;
    }

    fn score_imbalance(&self, scores: &mut KuehlmakScores) {
        let mut hand_weight = [0, 0];
        for (&count, props) in
                scores.heatmap.iter().zip(self.key_props.iter()) {
            hand_weight[props.hand as usize] += count;
        }
        let balance = if hand_weight[0] > hand_weight[1] {
            hand_weight[1] as f64 / hand_weight[0] as f64
        } else {
            hand_weight[0] as f64 / hand_weight[1] as f64
        };
        scores.imbalance = balance.max(0.001).recip() - 1.0;
    }

    pub fn new(params: Option<KuehlmakParams>) -> KuehlmakModel {
        let params = params.unwrap_or_default();
        let mut i = 0;
        let mut k = || Self::key_props({i += 1; i - 1}, &params);
        let key_props = [
            k(), k(), k(), k(), k(), k(), k(), k(), k(), k(),
            k(), k(), k(), k(), k(), k(), k(), k(), k(), k(),
            k(), k(), k(), k(), k(), k(), k(), k(), k(), k(),
        ];

        // Scissors are symmetrical in two ways:
        // 1. If the bigram AB is a scissor, so is BA
        // 2. Left and right hand are symmetrical (approx. with row-stagger)
        // Enumerate scissors on left hand going left->right. Compute the rest
        // from the symmetries.
        let mut scissors_lr = vec![
            (0u8, 11u8), (0, 21), (0, 12), (0, 22), (0, 23), (10, 21), (20, 1), (20, 2), (20, 3),
            (1, 22), (1, 23), (21, 2), (21, 3), (2, 23), (22, 3),
            (0, 24), (1, 24), (2, 24)];
        // Adjust top row for KeyboardType::Hex
        if let KeyboardType::Hex | KeyboardType::HexStag = params.board_type {
            for b in scissors_lr.iter_mut() {
                match b.0 {
                    0..=3 => b.0 += 1,
                    _ => (),
                }
                match b.1 {
                    0..=3 => b.1 += 1,
                    _ => (),
                }
            }
            scissors_lr.extend([(0u8, 11u8), (0, 21), (0, 12), (0, 22), (0, 23), (0, 24)]);
        } else {
            scissors_lr.extend([(20u8, 4u8), (21, 4), (22, 4)]);
        }
        let mut scissors = Vec::new();
        scissors.extend(&scissors_lr);
        scissors.extend(scissors_lr.iter()
                                .map(|b| (b.1, b.0)));
        scissors.extend(scissors_lr.iter()
                                .map(|b| (mirror_key(b.0), mirror_key(b.1))));
        scissors.extend(scissors_lr.iter()
                                .map(|b| (mirror_key(b.1), mirror_key(b.0))));
        scissors.sort();

        let mut bigram_types = [[BIGRAM_ALTERNATE as u8; 30]; 30];
        for (i, &KeyProps {hand: h0, finger: f0, is_stretch: s0, ..})
                in key_props.iter().enumerate() {
            for (j, &KeyProps {hand: h1, finger: f1, is_stretch: s1, ..})
                    in key_props.iter().enumerate() {
                if h0 != h1 {
                    continue;
                }

                let b = (i as u8, j as u8);

                if i == j {
                    bigram_types[i][j] = BIGRAM_SAMEKEY as u8;
                } else if f0 == f1 {
                    bigram_types[i][j] = BIGRAM_SFB as u8;
                } else if s0 || s1 {
                    let d = (f0 as i8 - f1 as i8).abs() as u8;
                    bigram_types[i][j] = match d {
                        _ if s0 && s1 || scissors.binary_search(&b).is_ok()
                            => BIGRAM_LSB1,
                        1 => BIGRAM_LSB1,
                        2 => BIGRAM_LSB2,
                        _ => BIGRAM_LSB3,
                    } as u8;
                } else if scissors.binary_search(&b).is_ok() {
                    bigram_types[i][j] = BIGRAM_SCISSOR as u8;
                } else if f0 == 1 || f0 == 6 || // Rolling away from ring finger or
                          f0 == 3 || f0 == 4 || // Involving index finger
                          f1 == 3 || f1 == 4 {
                    bigram_types[i][j] = BIGRAM_DROLL as u8;
                } else {
                    bigram_types[i][j] = BIGRAM_UROLL as u8;
                }
            }
        }

        let mut trigram_types = [[[TRIGRAM_NONE as u8; 30]; 30]; 30];
        for (i, &KeyProps {hand: h0, finger: f0, ..})
                in key_props.iter().enumerate() {
            for (j, &KeyProps {hand: h1, finger: f1, ..})
                    in key_props.iter().enumerate() {
                for (k, &KeyProps {hand: h2, finger: f2, ..})
                        in key_props.iter().enumerate() {
                    if h0 == h2 && h0 != h1 { // Disjointed same-hand bigrams
                        trigram_types[i][j][k] = match bigram_types[i][k] as usize {
                            BIGRAM_SAMEKEY => TRIGRAM_D_SAMEKEY,
                            BIGRAM_SFB     => TRIGRAM_D_SFB,
                            BIGRAM_DROLL   => TRIGRAM_D_DROLL,
                            BIGRAM_UROLL   => TRIGRAM_D_UROLL,
                            BIGRAM_LSB1    => TRIGRAM_D_LSB1,
                            BIGRAM_LSB2    => TRIGRAM_D_LSB2,
                            BIGRAM_LSB3    => TRIGRAM_D_LSB3,
                            BIGRAM_SCISSOR => TRIGRAM_D_SCISSOR,
                            _              => panic!("Unexpected disjointed same-hand trigram")
                        } as u8;
                    } else if h0 == h1 && h1 == h2 { // Same-hand trigrams
                        if i == k && f0 != f1 { // Disjointed same-key
                            trigram_types[i][j][k] = TRIGRAM_SHD_SAMEKEY as u8;
                        } else if f0 == f2 && f0 != f1 { // Disjointed same-finger bigrams
                            trigram_types[i][j][k] = TRIGRAM_SHD_SFB as u8;
                        } else if bigram_types[i][j] >= BIGRAM_LSB1 as u8 && // Sequence of two bad bigrams
                                  bigram_types[j][k] >= BIGRAM_LSB1 as u8 {
                            trigram_types[i][j][k] = TRIGRAM_CONTORT as u8;
                        } else if f0 != f1 && f1 != f2 && // Same-hand disjointed scissors count as contortions
                                  bigram_types[i][k] == BIGRAM_SCISSOR as u8 {
                            trigram_types[i][j][k] = TRIGRAM_CONTORT as u8;
                        } else if f0 != f1 && f1 != f2 && // Reversing direction
                                  ((f2 > f1) ^ (f1 > f0)) {
                            trigram_types[i][j][k] = TRIGRAM_REDIRECT as u8;
                        } else if bigram_types[i][j] >= BIGRAM_DROLL as u8 && // Sequences of two rolls
                                  bigram_types[i][j] <  BIGRAM_LSB1  as u8 && // in the same direction
                                  bigram_types[j][k] >= BIGRAM_DROLL as u8 &&
                                  bigram_types[j][k] <  BIGRAM_LSB1  as u8 {
                            trigram_types[i][j][k] = TRIGRAM_RROLL as u8;
                        }
                        // What's left are non-reversing same-hand trigrams
                        // that start or end with a roll. Left as TRIGRAM_NONE
                        // and not scored.
                    }
                    // What's left are same-hand bigrams followed or preceded by
                    // hand changes. Left as TRIGRAM_NONE and not scored.
                }
            }
        }

        let mut key_cost_ranking = [0; 30];
        for (i, ranking) in key_cost_ranking.iter_mut().enumerate() {
            *ranking = i;
        }
        key_cost_ranking.sort_by_key(|&k| key_props[k].cost);

        KuehlmakModel {
            params,
            key_props,
            bigram_types,
            trigram_types,
            key_cost_ranking,
        }
    }

    fn key_props(key: u8, params: &KuehlmakParams) -> KeyProps {
        let key = key as usize;
        let row = key / 10;
        let col = key % 10;
        assert!(row < 3);

        let (hand, finger, weight, home_col, is_stretch) = match params.board_type {
            KeyboardType::Hex | KeyboardType::HexStag if row == 0 => match col {
                0     => (LEFT,  L_PINKY,  params.weights.pinky_finger,  0.0, true),
                1     => (LEFT,  L_PINKY,  params.weights.pinky_finger,  0.0, false),
                2     => (LEFT,  L_RING,   params.weights.ring_finger,   1.0, false),
                3     => (LEFT,  L_MIDDLE, params.weights.middle_finger, 2.0, false),
                4     => (LEFT,  L_INDEX,  params.weights.index_finger,  3.0, false),
                5     => (RIGHT, R_INDEX,  params.weights.index_finger,  6.0, false),
                6     => (RIGHT, R_MIDDLE, params.weights.middle_finger, 7.0, false),
                7     => (RIGHT, R_RING,   params.weights.ring_finger,   8.0, false),
                8     => (RIGHT, R_PINKY,  params.weights.pinky_finger,  9.0, false),
                9     => (RIGHT, R_PINKY,  params.weights.pinky_finger,  9.0, true),
                _     => panic!("col out of range"),
            },
            _ => match col {
                0     => (LEFT,  L_PINKY,  params.weights.pinky_finger,  0.0, false),
                1     => (LEFT,  L_RING,   params.weights.ring_finger,   1.0, false),
                2     => (LEFT,  L_MIDDLE, params.weights.middle_finger, 2.0, false),
                3     => (LEFT,  L_INDEX,  params.weights.index_finger,  3.0, false),
                4     => (LEFT,  L_INDEX,  params.weights.index_finger,  3.0, true),
                5     => (RIGHT, R_INDEX,  params.weights.index_finger,  6.0, true),
                6     => (RIGHT, R_INDEX,  params.weights.index_finger,  6.0, false),
                7     => (RIGHT, R_MIDDLE, params.weights.middle_finger, 7.0, false),
                8     => (RIGHT, R_RING,   params.weights.ring_finger,   8.0, false),
                9     => (RIGHT, R_PINKY,  params.weights.pinky_finger,  9.0, false),
                _     => panic!("col out of range"),
            },
        };
        let (key_offsets, key_cost) = match params.board_type {
            KeyboardType::Ortho   => (&KEY_OFFSETS_ORTHO, &KEY_COST_ORTHO),
            KeyboardType::ColStag => (&KEY_OFFSETS_ORTHO, &KEY_COST_COL_STAG),
            KeyboardType::Hex     => (&KEY_OFFSETS_HEX, &KEY_COST_HEX),
            KeyboardType::HexStag => (&KEY_OFFSETS_HEX, &KEY_COST_HEX_STAG),
            KeyboardType::ANSI    => (&KEY_OFFSETS_ANSI, &KEY_COST_ANSI),
            KeyboardType::ISO     => (&KEY_OFFSETS_ISO, &KEY_COST_ISO),
        };

        // Weigh horizontal offset more severely (factor 1.5).
        let x = col as f32 - home_col + key_offsets[row][hand];
        let y = row as f32 - 1.0;
        let d_abs = (x*x + y*y).sqrt();

        // Calculate relative distance to other keys on the same finger.
        // Used for calculating finger travel distances.
        let mut d_rel = [-1.0; 30];
        d_rel[key] = 0.0;

        let mut calc_d_rel = |r: usize, c: usize| {
            let dx = c as f32 - col as f32 + key_offsets[r][hand] - key_offsets[row][hand];
            let dy = r as f32 - row as f32;
            d_rel[(r * 10 + c)] = (dx*dx + dy*dy).sqrt();
        };
        for r in 0..3 {
            for c in 0..10 {
                if r != row || c != col {
                    calc_d_rel(r, c);
                }
            }
        }

        KeyProps {
            hand: hand as u8,
            finger: finger as u8,
            is_stretch,
            d_abs, d_rel,
            cost: key_cost[key] as u16 * weight as u16,
        }
    }
}

// Constants for indexing some arrays, so can't use enum variants
const LEFT:     usize = 0;
const RIGHT:    usize = 1;

const L_PINKY:  usize = 0;
const L_RING:   usize = 1;
const L_MIDDLE: usize = 2;
const L_INDEX:  usize = 3;
const R_INDEX:  usize = 4;
const R_MIDDLE: usize = 5;
const R_RING:   usize = 6;
const R_PINKY:  usize = 7;

const BIGRAM_ALTERNATE:  usize = 0;
const BIGRAM_SAMEKEY:    usize = 1;
const BIGRAM_DROLL:      usize = 2;
const BIGRAM_UROLL:      usize = 3;
const BIGRAM_LSB3:       usize = 4;
const BIGRAM_LSB2:       usize = 5;
const BIGRAM_LSB1:       usize = 6;
const BIGRAM_SCISSOR:    usize = 7;
const BIGRAM_SFB:        usize = 8;
const BIGRAM_NUM_TYPES:  usize = 9;

const TRIGRAM_NONE:        usize = 0;
const TRIGRAM_D_SAMEKEY:   usize = 1;
const TRIGRAM_SHD_SAMEKEY: usize = 2;
const TRIGRAM_D_SFB:       usize = 3;
const TRIGRAM_SHD_SFB:     usize = 4;
const TRIGRAM_D_DROLL:     usize = 5;
const TRIGRAM_D_UROLL:     usize = 6;
const TRIGRAM_D_LSB3:      usize = 7;
const TRIGRAM_D_LSB2:      usize = 8;
const TRIGRAM_D_LSB1:      usize = 9;
const TRIGRAM_D_SCISSOR:   usize = 10;
const TRIGRAM_RROLL:       usize = 11;
const TRIGRAM_REDIRECT:    usize = 12;
const TRIGRAM_CONTORT:     usize = 13;
const TRIGRAM_NUM_TYPES:   usize = 14;


type KeyOffsets = [[f32; 2]; 3];

const KEY_OFFSETS_ORTHO: KeyOffsets = [[ 0.0,   0.0 ], [0.0, 0.0], [ 0.0, 0.0]];
const KEY_OFFSETS_HEX:   KeyOffsets = [[-1.0,   1.0 ], [0.0, 0.0], [ 0.0, 0.0]];
const KEY_OFFSETS_ANSI:  KeyOffsets = [[-0.25, -0.25], [0.0, 0.0], [ 0.5, 0.5]];
const KEY_OFFSETS_ISO:   KeyOffsets = [[-0.25, -0.25], [0.0, 0.0], [-0.5, 0.5]];
const KEY_COST_ORTHO: [u8; 30] = [
    4,  2,  2,  4, 12, 12,  4,  2,  2,  4,
    1,  1,  1,  1,  3,  3,  1,  1,  1,  1,
    2,  4,  4,  2,  6,  6,  2,  4,  4,  2,
];
const KEY_COST_COL_STAG: [u8; 30] = [
    2,  2,  2,  2,  6,  6,  2,  2,  2,  2,
    1,  1,  1,  1,  3,  3,  1,  1,  1,  1,
    2,  2,  2,  2,  6,  6,  2,  2,  2,  2,
];
const KEY_COST_HEX: [u8; 30] = [
    3,  4,  2,  2,  4,      4,  2,  2,  4,  3,
      1,  1,  1,  1,  3,  3,  1,  1,  1,  1,
    2,  4,  4,  2,  6,      6,  2,  4,  4,  2,
];
const KEY_COST_HEX_STAG: [u8; 30] = [
    2,  3,  2,  2,  2,      2,  2,  2,  3,  2,
      1,  1,  1,  1,  3,  3,  1,  1,  1,  1,
    2,  2,  2,  2,  6,      6,  2,  2,  2,  2,
];
const KEY_COST_ANSI: [u8; 30] = [
    4,  2,  2,  4,  6, 12,  4,  2,  2,  4,
     1,  1,  1,  1,  3,  3,  1,  1,  1,  1,
       2,  4,  4,  2,  9,  3,  2,  4,  4,  2,
];
const KEY_COST_ISO: [u8; 30] = [
     4,  2,  2,  4,  6, 12,  4,  2,  2,  4,
      1,  1,  1,  1,  3,  3,  1,  1,  1,  1,
    2,  4,  4,  2,  3,      3,  2,  4,  4,  2,
];
