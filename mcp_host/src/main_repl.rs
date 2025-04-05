use anyhow::Result;
use tracing_subscriber;
use tracing_appender;
use std::time::Duration;
use log::info;
use console::style;
use tracing_appender::non_blocking::WorkerGuard; // Import the guard type

/// Main entry point for the MCP host REPL
pub async fn main() -> Result<()> {
    // Setup logging and keep the guard alive
    let _logging_guard = setup_logging();
    
    // Print startup info
    println!("MCP REPL starting...");
    println!("Current directory: {:?}", std::env::current_dir().unwrap_or_default());
    println!("Command line args: {:?}", std::env::args().collect::<Vec<_>>());
    
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut config_path_opt: Option<&str> = None;
    
    // Check for config file argument
    if args.len() > 2 && args[1] == "load_config" {
        config_path_opt = Some(&args[2]);
        info!("Config path specified: {}", args[2]);
    } else {
        println!("No config file specified. Use 'load_config <config_path>' to load a configuration.");
        // Optionally, you could try loading a default path here or exit
    }

    // Load configuration if path is provided
    let config = if let Some(config_path) = config_path_opt {
        println!("{}", style(format!("Loading configuration from: {}", config_path)).yellow());
        match crate::host::config::Config::load(config_path).await {
            Ok(cfg) => {
                println!("{}", style("Successfully loaded config!").green());
                Some(cfg)
            },
            Err(e) => {
                println!("{}", style(format!("Error loading config: {}", e)).red());
                println!("Attempting to continue with default settings...");
                None
            }
        }
    } else {
        None
    };

    // Initialize the MCPHost, passing AI provider config if available
    info!("Initializing MCPHost...");
    let mut host_builder = crate::host::MCPHost::builder()
        .request_timeout(Duration::from_secs(120)) // Example timeout
        .client_info("mcp-host-repl", "1.0.0");

    if let Some(ref cfg) = config {
        host_builder = host_builder.ai_provider_config(cfg.ai_provider.clone());
        // Apply timeouts from config if needed
        host_builder = host_builder.request_timeout(Duration::from_secs(cfg.timeouts.request));
    }

    let host = host_builder.build().await?;
    info!("MCPHost initialized.");

    // Configure the host with the loaded server configurations (if config was loaded)
    if let Some(cfg) = config {
         // We only need the servers part now
        let server_config = crate::host::config::Config {
             servers: cfg.servers,
             ai_provider: Default::default(), // Not needed here
             timeouts: Default::default(), // Not needed here
        };
        if let Err(e) = host.configure(server_config).await {
             println!("{}", style(format!("Error applying server configurations: {}", e)).red());
        }
    }

    // Run the REPL interface
    host.run_repl().await
}

// Return the WorkerGuard to keep it alive
pub fn setup_logging() -> Option<WorkerGuard> { 
    // Check if tracing should be disabled
    if std::env::var("DISABLE_TRACING").is_ok() {
        // Just use basic env_logger
        match env_logger::try_init() {
            Ok(_) => info!("Basic logging initialized"),
            Err(_) => eprintln!("Warning: Failed to initialize logger, another logger may be active")
        }
        return None; // No guard for env_logger
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
        
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender); // Rename _guard to guard
        
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG) // Change level to DEBUG
            .with_writer(non_blocking)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_target(true);
            
        // Try to initialize, but don't panic if it fails
        match subscriber.try_init() {
            Ok(_) => {
                info!("Tracing initialized successfully");
                Some(guard) // Return the guard
            },
            Err(e) => {
                eprintln!("Warning: Could not initialize tracing: {:?}", e);
                None // No guard if init fails
            }
        }
    } else {
        // Fallback to basic stderr logging if file appender fails
        eprintln!("Warning: Could not create file appender, falling back to stderr logging.");
        env_logger::builder().filter_level(log::LevelFilter::Info).init();
        None // No guard for fallback
    }
    
    // This log might happen before the guard takes effect, which is fine.
    // info!("MCP Host Enhanced REPL starting"); 
}
