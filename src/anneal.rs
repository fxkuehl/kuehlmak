use super::{EvalModel, EvalScores, Layout, TextStats};
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
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
            best_scores: model.eval_layout(&layout, text, 0.0, false),
            real_scores: model.eval_layout(&layout, text, 1.0, false),
            steps: 0,
            steps_per_iter,
            rng,
        }
    }

    pub fn write_stats<W>(&self, w: &mut W) -> io::Result<()>
    where W: io::Write {
        writeln!(w, "step:{} nois:{:.4} dNoi:{:.10} prec:{:.3} best:{:6.4}",
                 self.steps, self.noise, self.noise_step, self.precision,
                 self.best_scores.total())
    }

    fn update_precision(&mut self, d: f64) {
        self.precision += (1.0 - self.precision) * d;

        // Reevaluate the best known layout with updated precision
        self.best_scores = self.model.eval_layout(&self.best_scores.layout(),
                                                  self.text, self.precision,
                                                  false);
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

            let layout = self.model.neighbor(&mut self.rng, &self.cur_layout);
            let scores = self.model.eval_layout(&layout, self.text,
                                                self.precision, false);

            if scores.total() > self.best_scores.total() + 100.0*self.noise {
                // We're stuck in a local optimum with little hope of
                // getting back out. Reset to last know global optimum
                self.cur_layout = self.best_scores.layout();
                continue;
            }
            if scores.total() >= self.best_scores.total() + self.noise {
                // Reject score because it's above the noise level
                continue;
            }

            self.cur_layout = layout;

            if scores.total() >= self.best_scores.total() {
                // The layout was accepted but it's not a global improvement.
                continue;
            }

            let real_scores = self.model.eval_layout(&layout, self.text, 1.0, false);
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
