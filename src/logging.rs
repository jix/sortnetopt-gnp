use std::io::Write;

pub fn setup() {
    better_panic::install();
    let startup = std::time::Instant::now();

    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(move |buf, record| {
            let elapsed = startup.elapsed().as_millis();
            let minutes = elapsed / 60000;
            let seconds = (elapsed % 60000) / 1000;
            let millis = elapsed % 1000;
            writeln!(
                buf,
                "{:3}:{:02}.{:03}: {}",
                minutes,
                seconds,
                millis,
                record.args()
            )
        })
        .is_test(cfg!(test))
        .try_init();
}
