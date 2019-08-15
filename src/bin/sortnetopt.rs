#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use structopt::StructOpt;

use sortnetopt::{
    logging,
    output_set::OutputSet,
    subsume_index::{incremental_minimal_elements, AbstractedPair},
};

#[derive(Debug, StructOpt)]
struct Opt {
    /// Width (number of channels) of the sorting network
    width: usize,
}

fn main() {
    logging::setup();

    let opt = Opt::from_args();

    let output_set = OutputSet::all_values(opt.width);

    let mut layer = vec![AbstractedPair::new(output_set, ())];

    let mut layer_count = 0;

    while !layer.is_empty() {
        log::info!("layer {} size: {}", layer_count, layer.len());
        layer_count += 1;

        let progress = ProgressBar::new(layer.len() as u64);

        let template = "{elapsed_precise} [{wide_bar:.green/blue}] {percent}% {pos}/{len} {eta}";

        progress.set_style(
            ProgressStyle::default_bar()
                .template(template)
                .progress_chars("#>-"),
        );

        progress.enable_steady_tick(100);

        layer = incremental_minimal_elements(layer, |AbstractedPair { output_set, .. }| {
            progress.inc(1);

            let mut next_output_sets = vec![];
            let implications = output_set.implications();
            for j in 0..opt.width {
                for i in 0..j {
                    if implications.is_associated(i, j) {
                        continue;
                    }
                    let mut next_output_set = output_set.apply_comparator(i, j);
                    next_output_set.order_channels_by_weight();
                    next_output_sets.push(next_output_set);
                }
            }
            next_output_sets.sort_unstable();
            next_output_sets.dedup();
            next_output_sets
                .into_iter()
                .map(|output_set| AbstractedPair::new(output_set, ()))
                .par_bridge()
                .collect::<Vec<_>>()
        });

        progress.finish();
    }
}
