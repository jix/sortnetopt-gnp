# Lower Size Bounds for Sorting Networks using Generate and Prune

This is an implementation of the Generate and Prune approach described in
["Sorting nine inputs requires twenty-five comparisons" by Codish, Cruz-Filipe,
Frank and Schneider-Kamp][0].

It improves upon the algorithm described in that paper by using a much faster
subsumption check. The improvements are based on two ideas:

1. Reducing the number of permutations to consider using matchings in a
   bipartite graph of compatible channels. Similar to the approach as described
   in ["An Improved Subsumption Testing Algorithm for the Optimal-Size Sorting
   Network Problem" by Frăsinaru and Răschip][1].

2. Using a k-d-Tree of output sets to check an output set against multiple
   output sets at the same time. While descending from the root of the tree to
   the leaves, more edges in the biparatite graph of compatible channels are
   removed. This makes it possible to detect that no compatible matching exists
   for all output sets in a subtree.

If someone is interested I might find the time to describe this in more detail.

## Usage

After [installing Rust][2] this can be compiled and run using:

`cargo run --release <CHANNEL_COUNT>`

It will display the size of layer (R<sub>k</sub>) and a progress bar for the
currently computed layer.

## Performance

On a scaleway GP1-L instance (32 threads) running this for 9 channels took less
than 2 hours. The runtime and required memory grow quite fast with the number
of channels.

On an AMD Ryzen 9 3950X 16-Core Processor (32 threads) at 4.1GHz running this
for 9 channels takes 44 minutes.

See [jix/sortnetopt][3] for a newer approach that is much faster and unlike
this also works for 11 channels.


[0]: https://doi.org/10.1016/j.jcss.2015.11.014
[1]: https://doi.org/10.1007/978-3-030-19212-9_19
[2]: https://www.rust-lang.org/tools/install
[3]: https://github.com/jix/sortnetopt
