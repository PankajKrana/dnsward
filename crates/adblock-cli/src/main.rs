fn main() -> std::process::ExitCode {
    adblock_cli::main_with_args(std::env::args_os().collect())
}
