use super::{EvalModel, EvalScores, Layout, TextStats};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use std::ops::Range;
use std::io;

pub struct Anneal<'a, M>
where M: EvalModel<'a>
{
    model: &'a M,
    text: &'a TextStats,
    noise: f64,
    noise_step: f64,
    noise_floor: f64,
    precision: f64,
    cur_layout: Layout,
    best_scores: M::Scores,
    real_scores: M::Scores,
    steps: u64,
    steps_per_iter: u64,
    rng: SmallRng,
}

impl<'a, M> Anneal<'a, M>
where M: EvalModel<'a>
{
    pub fn new(model: &'a M, text: &'a TextStats,
               layout: Layout, shuffle: bool, steps_per_iter: u64) -> Self {
        let mut rng = SmallRng::from_entropy();
        let mut layout = layout;

        if shuffle {
            layout.shuffle(&mut rng);
        }

        Anneal {
            model, text,
            noise: 0.2,
            noise_step: 0.001,
            noise_floor: 0.001,
            precision: 0.0,
            cur_layout: layout,
            best_scores: model.eval_layout(&layout, text, 0.0),
            real_scores: model.eval_layout(&layout, text, 1.0),
            steps: 0,
            steps_per_iter,
            rng,
        }
    }

    pub fn write_stats<W>(&self, w: &mut W) -> io::Result<()>
    where W: io::Write {
        writeln!(w, "{:.4} {:.10} {:.3} {:6.4}",
                 self.noise, self.noise_step, self.precision,
                 self.best_scores.total())
    }
}

// Iterator interface for simulated annealing. Each call to next will return
// a new set of scores for the best layout found so far. Every time a new
// best layout is found, it is returned by the next method. If no better
// layout is found for a certain number of steps, the next method returns
// with the same scores. However, the internal state may be updated and
// further calls to next may yet find better layouts.
//
// next will return None once the annealing run is complete, with the noise
// so low that no more progress can be made.
impl<'a, M> Iterator for Anneal<'a, M>
where M: EvalModel<'a>
{
    type Item = M::Scores;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.steps;

        while self.noise > self.noise_floor {
            if self.steps - start >= self.steps_per_iter {
                // We haven't found a better solution in steps_per_iter
                // steps. Reduce noise and increase noise step to speed
                // up progress or termination
                self.noise *= 1.0 - self.noise_step;

                if self.noise_step < 0.1 {
                    self.noise_step *= 2.0f64.sqrt();
                }

                self.update_precision(self.noise_step*0.1);

                return Some(self.real_scores.clone());
            }
            self.steps += 1;

            let layout = self.mutate();
            let scores = self.model.eval_layout(&layout, self.text,
                                                self.precision);

            if scores.total() > self.best_scores.total() + 5.0*self.noise {
                // We're stuck in a local optimum with little hope of
                // getting back out. Reset to last know global optimum
                self.cur_layout = self.best_scores.layout();
                continue;
            }
            if scores.total() >= self.best_scores.total() + self.noise {
                continue;
            }

            self.cur_layout = layout;

            if scores.total() >= self.best_scores.total() {
                continue;
            }

            let real_scores = self.model.eval_layout(&layout, self.text, 1.0);
            if real_scores.total() > self.real_scores.total() {
                // The new layout is not actually an improvement. Increase
                // precision. The adjustment is proportional to the
                // error of the imprecise score and inversely proportional
                // to the noise
                let d = (real_scores.total() - scores.total()).abs()
                      / self.noise;

                self.update_precision(d.min(0.1));
            } else {
                // Improving the score is like going to a lower energy state,
                // which is exothermic. This allows finding more paths from
                // the new best solution.
                self.noise += self.real_scores.total() - real_scores.total();
                // Decrease noise step, allowing even more incremental
                // incremental improvements at this noise level
                if self.noise_step > 0.000001 {
                    self.noise_step *= 0.25;
                }

                self.best_scores = scores;
                self.real_scores = real_scores.clone();

                return Some(real_scores);
            }
        }
        None
    }
}

impl<'a, M> Anneal<'a, M>
where M: EvalModel<'a>
{
    fn update_precision(&mut self, d: f64) {
        self.precision += (1.0 - self.precision) * d;

        // Reevaluate the best known layout with updated precision
        self.best_scores = self.model.eval_layout(&self.best_scores.layout(),
                                                  self.text,
                                                  self.precision);
    }

    fn mutate(&mut self) -> Layout {
        // Use large mutations (that change more than two keys) only when
        // precision is still low.
        let available_ops = if self.precision < 0.5 {5} else {3};
        match self.rng.gen_range(0..available_ops) {
            0 => self.swap_keys(),
            1 => self.swap_finger_keys(),
            2 => self.swap_ranked_keys(),
            3 => self.swap_rows(),
            4 => self.swap_fingers(),
            _ => panic!()
        }
    }
    // Just swap two random keys
    fn swap_keys(&mut self) -> Layout {
        let (a, b) = self.sample2(0..30);
        let mut layout = self.cur_layout;
        layout[a] = layout[b];
        layout[b] = self.cur_layout[a];
        layout
    }
    // Swap two keys in the same finger. This doesn't change same-finger
    // scores. On symmetrical models it doesn't change effort scores.
    fn swap_finger_keys(&mut self) -> Layout {
        let f = self.rng.gen_range(0..8);
        let (a, b) = match f {
            0..=2 => {
                let (a, b) = self.sample2(0..3);
                (a * 10 + f, b * 10 + f)
            },
            3..=4 => {
                let (a, b) = self.sample2(0..6);
                ((a >> 1) * 10 + (a & 1) + f + (!f & 1),
                 (b >> 1) * 10 + (b & 1) + f + (!f & 1))
            },
            5..=7 => {
                let (a, b) = self.sample2(0..3);
                (a * 10 + f + 2, b * 10 + f + 2)
            },
            _ => panic!(),
        };
        let mut layout = self.cur_layout;
        layout[a] = layout[b];
        layout[b] = self.cur_layout[a];
        layout
    }
    // Swap similarly ranked keys. This reduces the change in the effort
    // scores compared to swapping completely random keys.
    fn swap_ranked_keys(&mut self) -> Layout {
        let window_size = 8;
        let window_start = self.rng.gen_range(0..(30 - window_size));
        let (a, b) = self.sample2(window_start..(window_start + window_size));
        let ranking = self.model.key_cost_ranking();
        let (a, b) = (ranking[a], ranking[b]);
        let mut layout = self.cur_layout;
        layout[a] = layout[b];
        layout[b] = self.cur_layout[a];
        layout
    }
    // Swap two rows in one hand only. This doesn't change same-finger
    // scores.
    fn swap_rows(&mut self) -> Layout {
        let h = self.rng.gen_range(0..2);
        let (a, b) = self.sample2(0..3);
        let mut layout = self.cur_layout;
        let a = h*5 + a*10;
        let b = h*5 + b*10;
        layout[a..a+5].copy_from_slice(&self.cur_layout[b..b+5]);
        layout[b..b+5].copy_from_slice(&self.cur_layout[a..a+5]);
        layout
    }
    // Swap two fingers. This doesn't change same-finger scores. Index
    // fingers are special because they have twice as many keys. They can
    // only be swapped with each other and get mirrored in the process.
    fn swap_fingers(&mut self) -> Layout {
        let (a, b) = self.sample2(0..7);
        let a = if a > 3 {a + 3} else {a};
        let b = if b > 3 {b + 3} else {b};
        let mut layout = self.cur_layout;
        if a == 3 || b == 3 {
            for r in 0..3 {
                layout[r*10 + 3] = layout[r*10 + 6];
                layout[r*10 + 4] = layout[r*10 + 5];
                layout[r*10 + 6] = self.cur_layout[r*10 + 3];
                layout[r*10 + 5] = self.cur_layout[r*10 + 4];
            }
        } else {
            for r in 0..3 {
                layout[r*10 + a] = layout[r*10 + b];
                layout[r*10 + b] = self.cur_layout[r*10 + a];
            }
        }
        layout
    }

    fn sample2(&mut self, r: Range<usize>)
    -> (usize, usize) {
        let b: usize = self.rng.gen_range(r.start..(r.end - 1));
        let a: usize = self.rng.gen_range(r);
        (a, if b >= a {b + 1} else {b})
    }
}
