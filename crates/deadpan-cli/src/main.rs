fn main() -> std::process::ExitCode {
    deadpan_cli::entry(std::env::args().skip(1))
}
