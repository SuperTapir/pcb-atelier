fn main() {
    match atelier_cli::execute(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
