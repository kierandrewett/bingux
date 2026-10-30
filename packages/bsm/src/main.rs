//! `bsm`: the Bingux Session Manager. One persistent process, named `bsm`.

use bsm::cli::{self, Parsed};
use bsm::run;

fn main() {
    let cli = match cli::parse(std::env::args().skip(1)) {
        Ok(Parsed::Help) => {
            print!("{}", cli::HELP);
            return;
        }
        Ok(Parsed::Version) => {
            println!("{}", bsm::version_line());
            return;
        }
        Ok(Parsed::Run(c)) => c,
        Err(e) => {
            eprintln!("bsm: {e}");
            std::process::exit(2);
        }
    };
    let print_config = cli.print_config;
    let settings = match run::resolve(cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("bsm: {e}");
            std::process::exit(2);
        }
    };
    if print_config {
        print!("{}", settings.describe());
        return;
    }
    run::init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), backend = ?settings.backend, "starting");
    if let Err(e) = run::run(settings) {
        tracing::error!("{e}");
        eprintln!("bsm: {e}");
        std::process::exit(1);
    }
}
