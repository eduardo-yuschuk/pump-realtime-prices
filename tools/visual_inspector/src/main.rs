use std::{env, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    match visual_inspector::run(&args, workspace_root) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            if error.shows_usage() {
                eprintln!("{}", visual_inspector::USAGE);
            }
            ExitCode::FAILURE
        }
    }
}
