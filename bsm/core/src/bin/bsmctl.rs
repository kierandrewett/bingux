//! `bsmctl`: command line client of bsm. See `bsm::ctl`.

use bsm::ctl::{self, Parsed};

fn main() {
    match ctl::parse(std::env::args().skip(1)) {
        Ok(Parsed::Help) => print!("{}", ctl::HELP),
        Ok(Parsed::Version) => println!("bsmctl {} (protocol {})", env!("CARGO_PKG_VERSION"), bsm_proto::PROTOCOL_VERSION),
        Ok(Parsed::Run(a)) => std::process::exit(ctl::run(a)),
        Err(e) => {
            eprintln!("bsmctl: {e}");
            std::process::exit(2);
        }
    }
}
