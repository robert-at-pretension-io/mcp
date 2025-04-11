// Main module exports for RPC client
mod error;
mod id_generator;
mod progress;
mod client;
mod transport;
mod sse_client_transport; // Add SSE client module
#[cfg(feature = "sse_server")] // Conditionally compile server module
mod sse_server_transport; // Add SSE server module

pub use self::client::{McpClient, McpClientBuilder};
pub use self::transport::{Transport, ProcessTransport, NotificationHandler};
pub use self::error::McpError;
pub use self::id_generator::IdGenerator;
pub use self::progress::ProgressTracker;
pub use self::sse_client_transport::SSEClientTransport; // Export SSE client transport
#[cfg(feature = "sse_server")] // Conditionally export server transport
pub use self::sse_server_transport::SSEServerTransport; // Export SSE server transport
