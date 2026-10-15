//! `ferrited`: Ferrite's Server for a machine reached over ssh (ADR 0026).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    std::process::exit(ferrite_core::server::ferrited(&args));
}
