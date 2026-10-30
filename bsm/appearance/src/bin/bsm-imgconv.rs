//! Sandboxed image converter run by bsm. See `bsm_appearance::imgconv`.

fn main() {
    std::process::exit(bsm_appearance::imgconv::main());
}
