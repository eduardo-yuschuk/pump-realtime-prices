use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match standalone_indexer::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
