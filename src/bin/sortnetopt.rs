#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use structopt::StructOpt;

use sortnetopt::logging;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Width (number of channels) of the sorting network
    width: usize,
}

fn main() {
    logging::setup();

    let opt = Opt::from_args();

    log::info!("{:?}", opt);

    unimplemented!()
}
