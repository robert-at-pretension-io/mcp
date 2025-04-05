use anyhow::Result;
use tracing_subscriber;
use tracing_appender;
use std::time::Duration;
use log::info;
use console::style;

/// Main entry point for the MCP host REPL
pub async fn main() -> Result<()> {
    // Setup logging
    setup_logging();
    
    // Print startup info
    println!("MCP REPL starting...");
    println!("Current directory: {:?}", std::env::current_dir().unwrap_or_default());
    println!("Command line args: {:?}", std::env::args().collect::<Vec<_>>());
    
    // Initialize the MCPHost
    info!("Initializing MCPHost...");
    let host = crate::host::MCPHost::builder()
        .request_timeout(Duration::from_secs(120))
        .client_info("mcp-host-repl", "1.0.0")
        .build()
        .await?;
    
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    
    // Check for config file
    if args.len() > 2 && args[1] == "load_config" {
        let config_path = &args[2];
        info!("Loading config from command line: {}", config_path);
        println!("{}", style(format!("Loading configuration from: {}", config_path)).yellow());
        
        // Verify file exists
        let path = std::path::Path::new(config_path);
        if path.exists() {
            println!("Config file exists: {}", config_path);
            println!("Absolute path: {:?}", path.canonicalize().unwrap_or_default());
            println!("File size: {} bytes", std::fs::metadata(path).map(|m| m.len()).unwrap_or(0));
            
            // Try reading it directly first
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    println!("File content preview: {}", &content[..content.len().min(100)]);
                },
                Err(e) => {
                    println!("Error reading file directly: {}", e);
                }
            }
        } else {
            println!("{}", style(format!("Warning: Config file does not exist: {}", config_path)).red());
        }
        
        match host.load_config(config_path).await {
            Ok(_) => println!("{}", style("Successfully loaded config!").green()),
            Err(e) => {
                println!("{}", style(format!("Error loading config: {}", e)).red());
                // Try creating a minimal config directly
                println!("Attempting to continue with a minimal default config...");
                let mut config = crate::host::config::Config::default();
                config.servers.insert(
                    "default".to_string(), 
                    crate::host::config::ServerConfig {
                        command: "/home/elliot/Projects/mcp/target/debug/mcp_tools".to_string(),
                        env: std::collections::HashMap::new(),
                    }
                );
                match host.configure(config).await {
                    Ok(_) => println!("{}", style("Successfully configured with default!").green()),
                    Err(e) => println!("{}", style(format!("Error configuring with default: {}", e)).red()),
                }
            }
        }
    } else {
        println!("No config file specified. Use 'load_config <config_path>' to load a configuration.");
    }
    
    // Run the REPL interface
    host.run_repl().await
}

pub fn setup_logging() {
    // Check if tracing should be disabled
    if std::env::var("DISABLE_TRACING").is_ok() {
        // Just use basic env_logger
        match env_logger::try_init() {
            Ok(_) => info!("Basic logging initialized"),
            Err(_) => eprintln!("Warning: Failed to initialize logger, another logger may be active")
        }
        return;
    }
    
    // Try to set up file appender for persistent logs if env_logger didn't initialize
    let log_dir = std::env::var("LOG_DIR")
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| format!("{}/Developer/mcp/logs", h.display()))
                .unwrap_or_else(|| "logs".to_string())
        });
    // Ensure log directory exists
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Warning: Could not create log directory {}: {}", log_dir, e);
    }
    // Only try to initialize tracing if we're not disabling it
    if let Ok(file_appender) = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::NEVER)
        .filename_prefix("mcp-host-repl")
        .filename_suffix("log")
        .build(log_dir) {
        
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(non_blocking)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_target(true);
            
        // Try to initialize, but don't panic if it fails
        match subscriber.try_init() {
            Ok(_) => info!("Tracing initialized successfully"),
            Err(e) => eprintln!("Warning: Could not initialize tracing: {:?}", e)
        }
    } else {
        // Fallback to basic stderr logging if file appender fails
        eprintln!("Warning: Could not create file appender, falling back to stderr logging.");
        env_logger::builder().filter_level(log::LevelFilter::Info).init();
    }
    
    info!("MCP Host Enhanced REPL starting");
}
