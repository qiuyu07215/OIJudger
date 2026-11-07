use clap::Parser;

fn main() {
    let cli = oij::cli::Cli::parse();
    match oij::cli::run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(2);
        }
    }
}
