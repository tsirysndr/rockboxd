//! Print the parsed metadata of each file given on the command line:
//!
//! ```sh
//! cargo run --example tags -- ~/Music/**/*.flac
//! ```

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: tags <audio-file> [more files…]");
        std::process::exit(2);
    }

    for path in &args {
        match rockbox_metadata::read(path) {
            Ok(meta) => println!("{path}\n{meta:#?}\n"),
            Err(err) => eprintln!("{path}: {err}"),
        }
    }
}
