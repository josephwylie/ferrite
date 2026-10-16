//! The one program: `herdr-projects` and its subcommands. Argument routing only: everything lives in the library.
fn main() { std::process::exit(sw::cli::main(std::env::args().skip(1).collect())); }
