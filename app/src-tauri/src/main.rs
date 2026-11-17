#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if quatsch_mit_sosse_app::run().is_err() {
        eprintln!("{}", quatsch_mit_sosse_app::startup_error());
        std::process::exit(1);
    }
}
