use anyhow::{Result}; // Removed anyhow function/macro import
use tracing_subscriber;
use tracing_appender;
use std::time::Duration;
use log::{info, error}; // Removed warn
use console::style;
use tracing_appender::non_blocking::WorkerGuard; // Import the guard type
use std::path::PathBuf; // Add PathBuf

/// Main entry point for the MCP host REPL
pub async fn main() -> Result<()> {
    // Setup logging and keep the guard alive
    let _logging_guard = setup_logging();

    // Print startup info - More structured
    println!("\n{}", style("--- MCP Host REPL ---").cyan().bold());
    // println!("Current directory: {:?}", std::env::current_dir().unwrap_or_default()); // Less verbose startup
    // println!("Command line args: {:?}", std::env::args().collect::<Vec<_>>()); // Less verbose startup

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut config_path_opt: Option<&str> = None;
    
    // Check for config file argument
    if args.len() > 2 && args[1] == "load_config" {
        config_path_opt = Some(&args[2]);
        info!("Config path specified: {}", args[2]);
    } else {
        // --- Suggestion: Add Default Path ---
        let default_path_buf = dirs::config_dir()
            .map(|p| p.join("mcp/mcp_host_config.json"));

        if let Some(ref path_buf) = default_path_buf {
             if path_buf.exists() {
                 println!("No config path specified, attempting to load default: {}", path_buf.display());
                 // Need to store the path string to pass its slice later
                 let path_str = path_buf.to_str().map(|s| s.to_string());
                 if let Some(_s) = path_str { // Prefix with underscore
                     // This is tricky because we need a 'static reference or owned string
                     // For simplicity, let's just load it here if it exists
                     // Or better, pass the PathBuf to the builder
                 } else {
                      println!("Could not convert default path to string.");
                 }
             } else {
                 println!("No config file specified and default not found ({}). Use 'load_config <path>' or create the default file.", path_buf.display());
             }
        } else {
             println!("No config file specified and could not determine default config path.");
        }
        // --- End Suggestion ---
    }

    // Initialize the MCPHost builder
    info!("Initializing MCPHost builder...");
    let mut host_builder = crate::host::MCPHost::builder()
        .request_timeout(Duration::from_secs(120)) // Example timeout, can be overridden by config
        .client_info("mcp-host-repl", "1.0.0");

    // Pass config path to builder if specified or default exists
    if let Some(path_str) = config_path_opt {
        host_builder = host_builder.config_path(PathBuf::from(path_str));
    } else if let Some(default_path_buf) = dirs::config_dir().map(|p| p.join("mcp/mcp_host_config.json")) {
         if default_path_buf.exists() {
             host_builder = host_builder.config_path(default_path_buf);
         }
    }

    // Build the host - this now loads the config internally
    let host = match host_builder.build().await {
        Ok(h) => {
            info!("MCPHost built successfully.");
            h
        },
        Err(e) => {
            error!("Failed to build MCPHost: {}", e);
            return Err(e.into()); // Propagate error
        }
    };

    // --- Print API Key Status ---
    println!("\n{}", style("AI Provider Key Status:").bold());
    let known_providers = [
        "openai", "anthropic", "deepseek", "gemini", "google",
        "ollama", "xai", "grok", "phind", "groq", "openrouter"
    ];
    let mut found_keys = Vec::new();
    let mut missing_keys = Vec::new();
    let mut not_needed = Vec::new();

    for provider in known_providers {
        if let Some(key_var) = crate::host::MCPHost::get_api_key_var(provider) {
            match crate::host::MCPHost::get_api_key_for_provider(provider) {
                Ok(_) => found_keys.push(provider.to_string()),
                Err(_) => missing_keys.push(format!("{} (Set {})", provider, key_var)),
            }
        } else if provider == "ollama" { // Explicitly handle providers not needing keys
             not_needed.push(provider.to_string());
        }
        // Ignore providers where get_api_key_var returns None and it's not Ollama (shouldn't happen with current list)
    }

    found_keys.sort();
    missing_keys.sort();
    not_needed.sort();

    for provider in found_keys {
        println!("  {} {}", style("✔").green(), provider);
    }
    for provider_info in missing_keys {
        println!("  {} {}", style("✖").red(), provider_info);
    }
     for provider in not_needed {
        println!("  {} {} (No API key needed)", style("ℹ").blue(), provider);
    }

    // OS-specific instructions
    let os = std::env::consts::OS;
    println!("\n{}", style("Tip:").bold());
    println!("  Set missing API keys as environment variables or in a `.env` file.");
    match os {
        "linux" | "macos" => {
            println!("  Example (Linux/macOS): {}", style("export PROVIDER_API_KEY=\"your_key\"").yellow());
            println!("  Or add to your shell profile (e.g., ~/.bashrc, ~/.zshrc).");
        }
        "windows" => {
            println!("  Example (Windows CMD): {}", style("set PROVIDER_API_KEY=your_key").yellow());
            println!("  Example (Windows PowerShell): {}", style("$env:PROVIDER_API_KEY=\"your_key\"").yellow());
            println!("  Or set permanently via System Properties -> Environment Variables.");
        }
        _ => {
            println!("  Consult your operating system's documentation for setting environment variables.");
        }
    }
    println!("  Using a `.env` file in the project root is recommended for managing keys.");
    // --- End API Key Status ---

    // Add a final startup line before applying config
    println!("\n{}", style("Initializing servers and REPL...").dim());


    // Apply the initial configuration loaded during build to start servers
    info!("Applying initial configuration to start servers...");
    let initial_config = { // Scope the lock guard
        let config_guard = host.config.lock().await;
        (*config_guard).clone() // Clone the Config inside the guard
    };
    if let Err(e) = host.apply_config(initial_config).await {
         error!("Failed to apply initial server configuration: {}", e);
         println!("{}", style(format!("Warning: Failed to start servers from initial config: {}", e)).yellow());
         // Decide how to handle this - maybe exit or continue without servers?
    } else {
         info!("Initial server configuration applied.");
    }
    info!("Returned from apply_config in main_repl."); // <-- Add log here

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
