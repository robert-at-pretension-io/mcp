use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    mcp_host::main_repl::main().await?;
    Ok(())
}