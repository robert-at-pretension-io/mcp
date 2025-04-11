// Main module exports for RPC client
mod client;
mod transport;
mod client;
mod transport;
mod error;
mod id_generator;
mod progress;
mod sse_client_transport; // Add SSE client module
#[cfg(feature = "sse_server")] // Conditionally compile server module
mod sse_server_transport; // Add SSE server module

pub use client::{McpClient, McpClientBuilder};
pub use transport::{Transport, ProcessTransport, NotificationHandler};
pub use error::McpError;
pub use id_generator::IdGenerator;
pub use progress::ProgressTracker;
pub use sse_client_transport::SSEClientTransport; // Export SSE client transport
#[cfg(feature = "sse_server")] // Conditionally export server transport
pub use sse_server_transport::SSEServerTransport; // Export SSE server transport
