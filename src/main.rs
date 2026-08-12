use std::process;

fn main() {
    if let Err(err) = c2ap::run() {
        eprintln!("error: {err}");
        process::exit(1);
    }
}
