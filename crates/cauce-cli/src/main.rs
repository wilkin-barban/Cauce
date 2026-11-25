//! Entry point for `cauce-cli`.
//!
//! Exit codes:
//! - `0`: Success.
//! - `1`: Runtime failure (missing socket, daemon unavailable, communication error).
//! - `2`: Usage error (invalid arguments, unknown preset, unknown language, out-of-range CRF).

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let code = cauce_cli::run(args, &mut stdout, &mut stderr);
    std::process::exit(code);
}
