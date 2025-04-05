use anyhow::Result;
use mcp_host::main_repl; // Use the main_repl module from the library crate

#[tokio::main]
async fn main() -> Result<()> {
    // Simply forward to the REPL implementation defined in the library
    main_repl::main().await
}
