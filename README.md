# Batched plotting for my criterion benchmarks

Most of my recent criterion microbenchmarks follow some implicit conventions:

- A benchmark group contains time measurements of an implementation of some
  computation, at various input sizes/iteration counts (often but no always
  following some power-of-two exponential laws, possibly with gaps to avoid
  spending time on "uninteresting" runs).
- The name of a benchmark group is a slash-separated sequence of benchmark
  properties, from most general (e.g. kind of computation being performed) to
  most specific (e.g. degree of extra instruction-level parallelism in the inner
  loop).
- The property of interest is the benchmark throughput, in elements/second
  (usually FLOP/s) or sometimes bytes/second.

The purpose of this quick and dirty program is to ease automatically drawing
stacked line charts from such data.
