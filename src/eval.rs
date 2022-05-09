use super::{TextStats, Bigram, Trigram};
use std::fs::OpenOptions;
use std::io::{self, BufWriter};
use std::io::Write as IoWrite;
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;
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

pub fn layout_to_filename(layout: &Layout) -> String {
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
    s
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
pub fn layout_distance(a: &Layout, b: &Layout) -> f64 {
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
    ANSI,
    ISO,
}

#[derive(Clone, Copy)]
struct KeyProps {
    hand: u8,
    finger: u8,
    d_abs: f32,
    d_rel: [f32; 30],
    cost: u8,
}

pub trait EvalScores {
    fn write<W>(&self, w: &mut W) -> io::Result<()>
        where W: IoWrite;
    fn write_extra<W>(&self, w: &mut W) -> io::Result<()>
        where W: IoWrite;
    fn layout(&self) -> Layout;
    fn layout_ref(&self) -> &Layout;
    fn total(&self) -> f64;

    fn write_to_db(&self, dir: &str) -> io::Result<()> {
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
}

#[derive(Clone, Copy, Serialize, Deserialize)]
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
#[serde(default)]
pub struct KuehlmakWeights {
    effort: f64,
    travel: f64,
    imbalance: f64,
    fast_bigrams: f64,
    same_finger_bigrams: f64,
    row_jumping_bigrams: f64,
    tiring_bigrams: f64,
    fast_trigrams: f64,
    same_finger_trigrams: f64,
    row_jumping_trigrams: f64,
    reversing_trigrams: f64,
}

impl Default for KuehlmakWeights {
    fn default() -> Self {
        KuehlmakWeights {
            effort:               0.2,
            travel:               0.1,
            imbalance:            0.05,
            fast_bigrams:        -0.1, // negative to maximize fast bigrams
            same_finger_bigrams: 10.0,
            row_jumping_bigrams:  5.0,
            tiring_bigrams:       1.0,
            fast_trigrams:       -0.1, // negative to maximize fast trigrams
            same_finger_trigrams: 1.0,
            row_jumping_trigrams: 1.0,
            reversing_trigrams:   5.0,
        }
    }
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConstraintParams {
    #[serde(with = "serde_layout")]
    ref_layout: Option<Layout>,
    ref_weight: f64,
    ref_threshold: f64,
    zxcv: f64,
    nonalpha: f64,
}

fn eval_constraints(layout: &Layout, params: &ConstraintParams) -> f64 {
    let ref_distance = match params.ref_layout.as_ref() {
        Some(ref_layout) if params.ref_weight != 0.0 =>
            (layout_distance(layout, ref_layout) - params.ref_threshold)
            .max(0.0) * (1.0 - params.ref_threshold) * params.ref_weight,
        _ => 0.0,
    };
    return params.zxcv * eval_zxcv(layout) +
           params.nonalpha * eval_nonalpha(layout) +
           ref_distance;
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
    trigram_lists: [Option<Vec<(Trigram, u64)>>; BIGRAM_NUM_TYPES],
    finger_travel: [f64; 8],
    effort: f64,
    travel: f64,
    imbalance: f64,
    total: f64,
    constraints: f64,
}

#[derive(Clone, Copy)]
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
                KeyboardType::Ortho => [["", "  |  ", ""]; 3],
                KeyboardType::ANSI =>
                    [[" ", "", "    "],
                     ["  ", "", "   "],
                     ["    ", "", " "]],
                KeyboardType::ISO =>
                    [[" ", " ", "   "],
                     ["  ", " ", "  "],
                     ["", "     ", ""]],
            };

        let mut layout_iter = self.layout.iter();
        let mut write_5keys = |w: &mut W|
            layout_iter.by_ref().take(5)
                       .map(|&[a, b]| match b.to_lowercase().next() {
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
        write!(w, "{:3.0}+{:3.0}+{:3.0}+{:3.0}=   {:3.0}|",
               fh_iter.next().unwrap(), fh_iter.next().unwrap(),
               fh_iter.next().unwrap(), fh_iter.next().unwrap(),
               hh_iter.next().unwrap())?;
        writeln!(w, "{:3.0}  ={:3.0}+{:3.0}+{:3.0}+{:3.0} ",
                 hh_iter.next().unwrap(),
                 fh_iter.next().unwrap(), fh_iter.next().unwrap(),
                 fh_iter.next().unwrap(), fh_iter.next().unwrap())?;

        write!(w, "Travel {:6.4} ({:6.2})            |",
               self.travel, travel)?;
        write!(w, "{:3.0}+{:3.0}+{:3.0}+{:3.0}=   {:3.0}|",
               ft_iter.next().unwrap(), ft_iter.next().unwrap(),
               ft_iter.next().unwrap(), ft_iter.next().unwrap(),
               ht_iter.next().unwrap())?;
        writeln!(w, "{:3.0}  ={:3.0}+{:3.0}+{:3.0}+{:3.0} ",
                 ht_iter.next().unwrap(),
                 ft_iter.next().unwrap(), ft_iter.next().unwrap(),
                 ft_iter.next().unwrap(), ft_iter.next().unwrap())?;

        write!(w, "  SameFing RowJump  Fast   Tiring |")?;
        write_key_row(w, key_space[0])?;

        write!(w, "2: {:6.2}  {:6.2}  {:6.2}  {:6.2} |",
               self.bigram_counts[BIGRAM_SAME_FINGER] as f64 * norm,
               self.bigram_counts[BIGRAM_ROW_JUMPING] as f64 * norm,
               self.bigram_counts[BIGRAM_FAST] as f64 * norm,
               self.bigram_counts[BIGRAM_TIRING] as f64 * norm)?;
        write_heat_row(w, key_space[0])?;

        write!(w, "3: {:6.2}  {:6.2}  {:6.2}  {:6.2} |",
               self.trigram_counts[TRIGRAM_SAME_FINGER] as f64 * norm,
               self.trigram_counts[TRIGRAM_ROW_JUMPING] as f64 * norm,
               self.trigram_counts[TRIGRAM_FAST] as f64 * norm,
               self.trigram_counts[TRIGRAM_REVERSING] as f64 * norm)?;
        write_key_row(w, key_space[1])?;

        write!(w, "                        Reversing |")?;
        write_heat_row(w, key_space[1])?;

        write!(w, "Total+Constraints   {:6.4}{:+7.4} |",
               self.total, self.constraints)?;
        write_key_row(w, key_space[2])?;

        write!(w, "Hand runs TODO                    |")?;
        write_heat_row(w, key_space[2])?;

        Ok(())
    }

    fn write_extra<W>(&self, w: &mut W) -> io::Result<()>
    where W: IoWrite {
        let norm = 1000.0 / self.strokes as f64;
        let is_side = |side, c| self.layout.iter()
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

        let bigram_names = ["", "Fast", "Same finger", "Row jumping", "Tiring"];
        for (vec, name) in self.bigram_lists.iter()
                               .zip(bigram_names.into_iter())
                               .filter_map(|(vec, name)|
                                            if let Some(vec) = vec.as_ref() {
                                                Some((vec, name))
                                            } else {
                                                None
                                            }) {
            writeln!(w)?;
            writeln!(w, "{} bigrams:", name)?;
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

        let trigram_names = ["", "Fast", "Same finger", "Row jumping", "Reversing"];
        for (vec, name) in self.trigram_lists.iter()
                               .zip(trigram_names.into_iter())
                               .filter_map(|(vec, name)|
                                            if let Some(vec) = vec.as_ref() {
                                                Some((vec, name))
                                            } else {
                                                    None
                                            }) {
            writeln!(w)?;
            writeln!(w, "{} 3-grams:", name)?;
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

    fn layout(&self) -> Layout {self.layout}
    fn layout_ref(&self) -> &Layout {&self.layout}
    fn total(&self) -> f64 {self.total + self.constraints}
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
            bigram_lists: [None, bl(), bl(), bl(), bl()],
            trigram_lists: [None, tl(), tl(), tl(), tl()],
            finger_travel: [0.0; 8],
            effort: 0.0,
            travel: 0.0,
            imbalance: 0.0,
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
            (self.params.weights.fast_bigrams / strokes,
             scores.bigram_counts[BIGRAM_FAST] as f64),
            (self.params.weights.same_finger_bigrams / strokes,
             scores.bigram_counts[BIGRAM_SAME_FINGER] as f64),
            (self.params.weights.row_jumping_bigrams / strokes,
             scores.bigram_counts[BIGRAM_ROW_JUMPING] as f64),
            (self.params.weights.tiring_bigrams / strokes,
             scores.bigram_counts[BIGRAM_TIRING] as f64),
            (self.params.weights.fast_trigrams / strokes,
             scores.trigram_counts[TRIGRAM_FAST] as f64),
            (self.params.weights.same_finger_trigrams / strokes,
             scores.trigram_counts[TRIGRAM_SAME_FINGER] as f64),
            (self.params.weights.row_jumping_trigrams / strokes,
             scores.trigram_counts[TRIGRAM_ROW_JUMPING] as f64),
            (self.params.weights.reversing_trigrams / strokes,
             scores.trigram_counts[TRIGRAM_REVERSING] as f64)
        ].into_iter().map(|(score, weight)| score * weight).sum::<f64>();

        scores
    }
    fn key_cost_ranking(&'a self) -> &'a [usize; 30] {&self.key_cost_ranking}
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
        for (&count, props) in
                scores.heatmap.iter().zip(self.key_props.iter()) {
            scores.finger_travel[props.finger as usize] +=
                props.d_abs as f64 * count as f64;
        }
        let orig_finger_travel = scores.finger_travel;

        let percentile = (ts.total_bigrams() as f64 * precision) as u64;
        let mut total = 0;
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
            scores.bigram_lists[bigram_type]
                .as_mut().map(|v| v.push((bigram, count)));

            if bigram_type == BIGRAM_SAME_FINGER {
                // Correct travel estimate: going to k1 not from home
                // position but from k0 instead.
                let props = &self.key_props[k1];

                scores.finger_travel[props.finger as usize] +=
                    (props.d_rel[k0] - props.d_abs) as f64 * count as f64;
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
            scores.trigram_lists[trigram_type]
                .as_mut().map(|v| v.push((trigram, count)));

            if trigram_type == TRIGRAM_SAME_FINGER {
                // Correct travel estimate: going to k2 not from home
                // position but from k0 instead. But only if k1 uses a
                // different finger. Otherwise the same-finger bigrams
                // will account for the travel distance.
                let props = &self.key_props[k2];

                if props.finger != self.key_props[k1].finger {
                    scores.finger_travel[props.finger as usize] +=
                        (props.d_rel[k0] - props.d_abs) as f64 * count as f64;
                }
            }
        }
        for count in scores.trigram_counts.iter_mut() {
            *count = *count * ts.total_trigrams() / total;
        }
        for (travel, orig) in scores.finger_travel.iter_mut()
                                    .zip(orig_finger_travel) {
            *travel += (*travel - orig) * (1.0 - precision);
        }
    }

    fn score_travel(&self, scores: &mut KuehlmakScores) {
        // Weigh travel per finger by the average key cost of that finger.
        // This penalizes travel more heavily on keys that are expected
        // to be used less (due to higher average cost).
        //
        // Square the per-finger travel so the score is dominated by the
        // fingers that travel most. The square root of the sum brings
        // the value range back down and makes the score more sensitive.
        let mut finger_weight = [(0.0, 0); 8];
        for props in self.key_props.iter() {
            let f = props.finger as usize;
            finger_weight[f].0 += props.cost as f64;
            finger_weight[f].1 += 1;
        }
        scores.travel = scores.finger_travel.iter().zip(finger_weight)
                              .map(|(&travel, (weight, n))| {
                                  let t = travel * weight / (n as f64);
                                  t * t
                              }).sum::<f64>().sqrt() / scores.strokes as f64;
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
        let mut k = || Self::key_props({i += 1; i - 1}, params.board_type);
        let key_props = [
            k(), k(), k(), k(), k(), k(), k(), k(), k(), k(),
            k(), k(), k(), k(), k(), k(), k(), k(), k(), k(),
            k(), k(), k(), k(), k(), k(), k(), k(), k(), k(),
        ];

        // Fast bigrams going in one direction, also used to construct fast
        // trigrams in the same direction below. One hand only, the other
        // hand is derived algorithmically.
        let fast_bigrams_lr = vec![
            ( 1u8,  2u8), ( 1, 13),
            ( 2, 13),
            (10,  1), (10,  2), (10, 11), (10, 12), (10, 13), (10, 23),
            (11, 12), (11, 13), (11, 23),
            (12, 13), (12, 23),
            (20, 11), (20, 12), (20, 13), (20, 23)];
        let fast_bigrams_rl = vec![
            (13u8,  1u8), (13,  2), (13, 10), (13, 11), (13, 12), (13, 20),
            (23, 10), (23, 11), (23, 12), (23, 20),
            ( 2,  1), ( 2, 10),
            (12, 11), (12, 10), (12, 20),
            ( 1, 10),
            (11, 10), (11, 20)];
        let mut fast_bigrams = Vec::new();
        fast_bigrams.extend(&fast_bigrams_lr);
        fast_bigrams.extend(&fast_bigrams_rl);
        fast_bigrams.extend(fast_bigrams_lr.iter()
                           .map( |b| (mirror_key(b.0), mirror_key(b.1)) ));
        fast_bigrams.extend(fast_bigrams_rl.iter()
                           .map( |b| (mirror_key(b.0), mirror_key(b.1)) ));
        fast_bigrams.sort();

        // Bad row jumping:
        // - adjacent fingers when they're not both stretching in their
        //   preferred direction
        // - more distant fingers when neither are stretching in their
        //   preferred direction
        let row_jump_bigrams_down = vec![
            (0u8, 21u8), (0, 22), (1, 22), (2, 21), (3, 21), (3, 22), (4, 21), (4, 22),
        ];
        let mut row_jump_bigrams = Vec::new();
        row_jump_bigrams.extend(&row_jump_bigrams_down);
        row_jump_bigrams.extend(row_jump_bigrams_down.iter()
                                .map(|b| (b.1, b.0)));
        row_jump_bigrams.extend(row_jump_bigrams_down.iter()
                                .map(|b| (mirror_key(b.0), mirror_key(b.1))));
        row_jump_bigrams.extend(row_jump_bigrams_down.iter()
                                .map(|b| (mirror_key(b.1), mirror_key(b.0))));
        row_jump_bigrams.sort();

        // Fast trigrams: consecutive fast bigrams in same direction
        let mut fast_trigrams = Vec::new();
        for a in fast_bigrams_lr.iter() {
            fast_trigrams.extend(fast_bigrams_lr.iter().filter(|b| a.1 == b.0)
                                 .flat_map(|b| [(a.0, b.0, b.1),
                        (mirror_key(a.0), mirror_key(b.0), mirror_key(b.1))]));
        }
        for a in fast_bigrams_rl.iter() {
            fast_trigrams.extend(fast_bigrams_rl.iter().filter( |b| a.1 == b.0 )
                                 .flat_map(|b| [(a.0, b.0, b.1),
                        (mirror_key(a.0), mirror_key(b.0), mirror_key(b.1))]));
        }
        fast_trigrams.sort();

        let mut bigram_types = [[BIGRAM_NONE as u8; 30]; 30];
        for i in 0..30 {
            let h0 = key_props[i].hand;
            let f0 = key_props[i].finger;
            for j in 0..30 {
                if i == j {
                    continue;
                }

                let h1 = key_props[j].hand;
                let f1 = key_props[j].finger;

                let b = (i as u8, j as u8);

                if fast_bigrams.binary_search(&b).is_ok() {
                    bigram_types[i][j] = BIGRAM_FAST as u8;
                } else if row_jump_bigrams.binary_search(&b).is_ok() {
                    bigram_types[i][j] = BIGRAM_ROW_JUMPING as u8;
                } else if f0 == f1 {
                    bigram_types[i][j] = BIGRAM_SAME_FINGER as u8;
                } else if h0 == h1 {
                    bigram_types[i][j] = BIGRAM_TIRING as u8;
                }
            }
        }

        let mut trigram_types = [[[TRIGRAM_NONE as u8; 30]; 30]; 30];
        for i in 0..30 {
            let h0 = key_props[i].hand;
            let f0 = key_props[i].finger;

            for j in 0..30 {
                let h1 = key_props[j].hand;
                let f1 = key_props[j].finger;

                for k in 0..30 {
                    if i == k {
                        continue;
                    }

                    let h2 = key_props[k].hand;
                    let f2 = key_props[k].finger;
                    let t = (i as u8, j as u8, k as u8);
                    let b02 = (i as u8, k as u8);

                    if fast_trigrams.binary_search(&t).is_ok() {
                        trigram_types[i][j][k] = TRIGRAM_FAST as u8;
                    } else if f0 == f2 {
                        trigram_types[i][j][k] = TRIGRAM_SAME_FINGER as u8;
                    } else if h0 == h1 && h1 == h2 && // All in the same hand
                            f0 != f1 && f1 != f2 &&   // No finger repeat
                            (f2 > f1) ^ (f1 > f0) &&  // Reversing direction
                            f0 != 1 && f0 != 6 &&     // Ring finger not first
                            f1 != 1 && f1 != 6 {      // Ring finger not second
                        trigram_types[i][j][k] = TRIGRAM_REVERSING as u8;
                    } else if row_jump_bigrams.binary_search(&b02).is_ok() {
                        trigram_types[i][j][k] = TRIGRAM_ROW_JUMPING as u8;
                    }
                }
            }
        }

        let mut key_cost_ranking = [0; 30];
        for i in 0..30 {
            key_cost_ranking[i] = i;
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

    fn key_props(key: u8, keyboard_type: KeyboardType) -> KeyProps {
        let key = key as usize;
        let row = key / 10;
        let col = key % 10;
        assert!(row < 3);

        let (hand, finger, home_col) = match col {
            0     => (LEFT,  L_PINKY,  0.0),
            1     => (LEFT,  L_RING,   1.0),
            2     => (LEFT,  L_MIDDLE, 2.0),
            3..=4 => (LEFT,  L_INDEX,  3.0),
            5..=6 => (RIGHT, R_INDEX,  6.0),
            7     => (RIGHT, R_MIDDLE, 7.0),
            8     => (RIGHT, R_RING,   8.0),
            9     => (RIGHT, R_PINKY,  9.0),
            _     => panic!("col out of range"),
        };
        let (key_offsets, key_cost) = match keyboard_type {
            KeyboardType::Ortho => (&KEY_OFFSETS_ORTHO, &KEY_COST_ORTHO),
            KeyboardType::ANSI  => (&KEY_OFFSETS_ANSI, &KEY_COST_ANSI),
            KeyboardType::ISO   => (&KEY_OFFSETS_ISO, &KEY_COST_ISO),
        };

        // Weigh horizontal offset more severely (factor 1.5).
        let x = (col as f32 - home_col + key_offsets[row][hand]) * 1.5;
        let y = row as f32 - 1.0;
        let d_abs = (x*x + y*y).sqrt();

        // Calculate relative distance to other keys on the same finger.
        // Used for calculating finger travel distances.
        let mut d_rel = [-1.0; 30];
        d_rel[key] = 0.0;

        let mut calc_d_rel = |r: usize, c: usize| {
            let dx = (c as f32 - home_col + key_offsets[r][hand]) * 1.5 - x;
            let dy = r as f32 - 1.0 - y;
            d_rel[(r * 10 + c)] = (dx*dx + dy*dy).sqrt();
        };
        for r in 0..3 {
            if r != row {
                calc_d_rel(r, col);
            }
            if col == 3 || col == 5 {
                calc_d_rel(r, col + 1);
            } else if col == 4 || col == 6 {
                calc_d_rel(r, col - 1);
            }
        }

        KeyProps {
            hand: hand as u8,
            finger: finger as u8,
            d_abs, d_rel,
            cost: key_cost[key],
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

const BIGRAM_NONE:          usize = 0;
const BIGRAM_FAST:          usize = 1;
const BIGRAM_SAME_FINGER:   usize = 2;
const BIGRAM_ROW_JUMPING:   usize = 3;
const BIGRAM_TIRING:        usize = 4;
const BIGRAM_NUM_TYPES:     usize = 5;

const TRIGRAM_NONE:         usize = 0;
const TRIGRAM_FAST:         usize = 1;
const TRIGRAM_SAME_FINGER:  usize = 2;
const TRIGRAM_ROW_JUMPING:  usize = 3;
const TRIGRAM_REVERSING:    usize = 4;
const TRIGRAM_NUM_TYPES:    usize = 5;


type KeyOffsets = [[f32; 2]; 3];

const KEY_OFFSETS_ORTHO: KeyOffsets = [[ 0.0,   0.0 ], [0.0, 0.0], [ 0.0, 0.0]];
const KEY_OFFSETS_ANSI:  KeyOffsets = [[-0.25, -0.25], [0.0, 0.0], [ 0.5, 0.5]];
const KEY_OFFSETS_ISO:   KeyOffsets = [[-0.25, -0.25], [0.0, 0.0], [-0.5, 0.5]];
const KEY_COST_ORTHO: [u8; 30] = [
    16,  6,  2,  6, 12, 12,  6,  2,  6, 16,
     5,  2,  1,  1,  4,  4,  1,  1,  2,  5,
     8, 10,  6,  2,  8,  8,  2,  6, 10,  8,
];
const KEY_COST_ANSI: [u8; 30] = [
    16,  6,  2,  6, 10, 14,  6,  2,  6, 16,
      5,  2,  1,  1,  4,  4,  1,  1,  2,  5,
        8, 10,  6,  2, 12,  2,  3,  6, 10,  8,
];
const KEY_COST_ISO: [u8; 30] = [
     16,  6,  2,  6, 10, 14,  6,  2,  6, 16,
       5,  2,  1,  1,  4,  4,  1,  1,  2,  5,
     8, 10,  6,  4,  6,      6,  4,  6, 10,  8,
];
