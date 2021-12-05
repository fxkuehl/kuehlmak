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
    cur_scores: M::Scores,
    best_scores: M::Scores,
    steps: u64,
    best_steps: u64,
    steps_per_iter: u64,
    rng: SmallRng,
}

impl<'a, M> Anneal<'a, M>
where M: EvalModel<'a>
{
    pub fn new(model: &'a M, text: &'a TextStats,
               layout: &Layout, shuffle: bool, steps_per_iter: u64) -> Self {
        let mut rng = SmallRng::from_entropy();

        let scores = match shuffle {
            true  => {
                let mut shuffled = *layout;

                shuffled.shuffle(&mut rng);
                model.eval_layout(&shuffled, text)
            }
            false => model.eval_layout(layout, text)
        };

        Anneal {
            model, text,
            noise: 0.1,
            noise_step: 0.001,
            noise_floor: 0.001,
            cur_scores: scores.clone(),
            best_scores: scores,
            steps: 0,
            best_steps: 0,
            steps_per_iter,
            rng,
        }
    }

    pub fn write_stats<W>(&self, w: &mut W) -> io::Result<()>
    where W: io::Write {
        writeln!(w, "{:.4} {:.10}", self.noise, self.noise_step)
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
                self.noise *= 1.0 - self.noise_step;

                if self.noise_step < 0.1 {
                    self.noise_step *= 2.0f64.sqrt();
                }

                return Some(self.best_scores.clone());
            }
            self.steps += 1;

            let layout = self.mutate();
            let scores = self.model.eval_layout(&layout, self.text);
            let noisy_score = scores.total() - self.noise;

            if noisy_score >= self.best_scores.total() {
                if scores.total() - 5.0*self.noise > self.best_scores.total() {
                    // We're stuck in a local optimum with little hope of
                    // getting back out. Reset to last know global optimum
                    self.cur_scores = self.best_scores.clone();
                }
                continue;
            }

            self.cur_scores = scores.clone();
            if scores.total() < self.best_scores.total() {
                // Improving the score is like going to a lower energy state,
                // which is exothermic. This allows finding more paths from
                // the new best solution.
                self.noise += self.best_scores.total() - scores.total();

                self.best_scores = scores.clone();
                self.best_steps = self.steps;

                if self.noise_step > 0.000001 {
                    self.noise_step *= 0.25;
                }
                return Some(scores);
            }
        }
        None
    }
}

impl<'a, M> Anneal<'a, M>
where M: EvalModel<'a>
{
    fn sample2(&mut self, r: Range<usize>)
    -> (usize, usize) {
        let b: usize = self.rng.gen_range(r.start..(r.end - 1));
        let a: usize = self.rng.gen_range(r);
        (a, if b >= a {b + 1} else {b})
    }
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
        let mut layout = self.cur_scores.layout();
        let tmp = layout[a];
        layout[a] = layout[b];
        layout[b] = tmp;
        layout
    }
    fn swap_keys(&mut self) -> Layout {
        let (a, b) = self.sample2(0..30);
        let mut layout = self.cur_scores.layout();
        let tmp = layout[a];
        layout[a] = layout[b];
        layout[b] = tmp;
        layout
    }
    fn mutate(&mut self) -> Layout {
        match self.rng.gen_range(0..2) {
            0 => self.swap_keys(),
            1 => self.swap_finger_keys(),
            _ => panic!()
        }
    }
}
