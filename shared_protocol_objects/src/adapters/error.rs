use crate::{
    JsonRpcError, INTERNAL_ERROR, INVALID_REQUEST, METHOD_NOT_FOUND, INVALID_PARAMS,
    PARSE_ERROR, REQUEST_CANCELLED, CONTENT_MODIFIED, SERVER_NOT_INITIALIZED,
    PROTOCOL_VERSION_MISMATCH, // Ensure this is defined in crate::lib or crate::rpc::error
};
use thiserror::Error;


#[derive(Error, Debug)]
pub enum AdapterError {
    #[error("RMCP SDK error: {0}")]
    RmcpError(#[from] rmcp::Error), // Directly wrap the SDK's error type

    #[error("Protocol conversion error: {0}")]
    ConversionError(String), // For errors during data structure mapping

    #[error("Transport error: {0}")]
    TransportError(String), // For adapter-level transport issues (e.g., setup)

    #[error("Service error: {0}")]
    ServiceError(String), // For adapter-level service issues

    #[error("Invalid state: {0}")]
    InvalidState(String), // For logic errors within the adapter

    #[error("Underlying transport failure: {0}")]
    IoError(#[from] std::io::Error), // Propagate IO errors if necessary

    #[error("Timeout waiting for response")]
    Timeout,

    #[error("Feature disabled: RMCP integration is not compiled")]
    FeatureDisabled,

    #[error(transparent)]
    Other(#[from] anyhow::Error), // Catch-all for other errors
}

// Optional: Implement conversion from AdapterError to anyhow::Error if needed elsewhere,
// though using anyhow::Error directly or AdapterError::Other might suffice.
// impl From<AdapterError> for anyhow::Error {
//     fn from(err: AdapterError) -> Self {
//         anyhow::anyhow!(err.to_string())
//     }
// }

// Conversion from RMCP SDK error types to our JSON-RPC error representation
// This is useful if we need to surface SDK errors as JSON-RPC errors to the caller.
impl From<rmcp::Error> for JsonRpcError {
    fn from(err: rmcp::Error) -> Self {
        let (code, message) = match &err {
            // Map SDK errors to our standard JSON-RPC codes
            rmcp::Error::ParseError(msg) => (PARSE_ERROR, msg.clone()),
            rmcp::Error::InvalidRequest(msg) => (INVALID_REQUEST, msg.clone()),
            rmcp::Error::MethodNotFound(msg) => (METHOD_NOT_FOUND, msg.clone()),
            rmcp::Error::InvalidParams(msg) => (INVALID_PARAMS, msg.clone()),
            rmcp::Error::InternalError(msg) => (INTERNAL_ERROR, msg.clone()),

            // Map LSP/RMCP specific codes if they exist in our definitions
            rmcp::Error::RequestCancelled(msg) => (REQUEST_CANCELLED, msg.clone()),
            rmcp::Error::ContentModified(msg) => (CONTENT_MODIFIED, msg.clone()),
            // rmcp::Error::ServerNotInitialized(msg) => (SERVER_NOT_INITIALIZED, msg.clone()), // Uncomment if SDK has this variant

            // Map other SDK error types
            rmcp::Error::Transport(details) => (INTERNAL_ERROR, format!("Transport error: {}", details)),
            rmcp::Error::Encode(details) => (INTERNAL_ERROR, format!("Encoding error: {}", details)),
            rmcp::Error::Decode(details) => (PARSE_ERROR, format!("Decoding error: {}", details)), // Treat decode errors as Parse errors
            rmcp::Error::DuplicateId(id) => (INTERNAL_ERROR, format!("Duplicate request ID: {:?}", id)),
            rmcp::Error::ResponseMismatch(id) => (INTERNAL_ERROR, format!("Response ID mismatch for request ID: {:?}", id)),
            // Map other specific rmcp::Error variants as needed...
            _ => (INTERNAL_ERROR, err.to_string()), // Default mapping
        };

        JsonRpcError {
            code,
            message,
            data: None, // Optionally extract data from rmcp::Error if available
        }
    }
}

// Conversion from AdapterError to JsonRpcError (useful for top-level error handling)
impl From<AdapterError> for JsonRpcError {
    fn from(err: AdapterError) -> Self {
        match err {
            AdapterError::RmcpError(sdk_err) => sdk_err.into(), // Reuse the conversion above
            AdapterError::ConversionError(msg) => JsonRpcError::new(INVALID_PARAMS, msg, None),
            AdapterError::TransportError(msg) | AdapterError::ServiceError(msg) | AdapterError::InvalidState(msg) => {
                JsonRpcError::new(INTERNAL_ERROR, msg, None)
            }
            AdapterError::IoError(io_err) => JsonRpcError::new(INTERNAL_ERROR, io_err.to_string(), None),
            AdapterError::Timeout => JsonRpcError::new(INTERNAL_ERROR, "Request timed out".to_string(), None), // Or a specific timeout code
            AdapterError::FeatureDisabled => JsonRpcError::new(INTERNAL_ERROR, "Feature disabled".to_string(), None),
            AdapterError::Other(e) => JsonRpcError::new(INTERNAL_ERROR, e.to_string(), None),
        }
    }
}
