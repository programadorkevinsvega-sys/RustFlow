fn main() {
    let code = match rustflow_cli::run(std::env::args_os().skip(1)) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("Run `rustflow --help` to see available commands.");
            2
        }
    };

    if code != 0 {
        std::process::exit(code);
    }
}
