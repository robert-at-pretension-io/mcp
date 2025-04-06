use anyhow::Result;
use mcp_host::main_repl; // Use the main_repl module from the library crate

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if it exists
    // This should be one of the first things the application does.
    match dotenvy::dotenv() {
        Ok(path) => println!("Loaded .env file from: {}", path.display()),
        Err(_) => println!("No .env file found or failed to load."), // It's okay if it doesn't exist
    }

    // Simply forward to the REPL implementation defined in the library
    main_repl::main().await
}
