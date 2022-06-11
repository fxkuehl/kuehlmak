# Kuehlmak

Kuehlmak is a keyboard layout generator written in Rust that uses simulated annealing to optimize keyboard layouts according to a multi-objective fitness functions applied to a corpus of text documents. It is still a work-in-progress. The end goal is to create a keyboard layout named "Kuehlmak" and prove that it is fast and comfortable to type on.

The algorithm was ported from an initial [prototype](https://github.com/fxkuehl/keyboard/blob/master/layout.md) written in Python. The port to Rust improved performance about tenfold compared to the Python version running on pypy3. This allows faster iteration and experimentation with the fitness function.
