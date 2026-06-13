//! ISO Compliance Module for NexusChain
//! 
//! Provides ISO 20022 and ISO 8583 message handling for financial services compliance.

pub mod iso20022;
pub mod iso8583;
pub mod validators;
pub mod currency;
pub mod message_router;

pub use iso20022::*;
pub use iso8583::*;
pub use validators::*;
pub use currency::*;
pub use message_router::*;

use thiserror::Error;

/// Result type for ISO operations
pub type IsoResult<T> = Result<T, IsoError>;

/// ISO compliance errors
#[derive(Error, Debug)]
pub enum IsoError {
    #[error("XML parsing error: {0}")]
    XmlParseError(String),
    
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Unknown message type: {0}")]
    UnknownMessageType(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
    
    #[error("Routing failed: {0}")]
    RoutingFailed(String),
    
    #[error("Compliance check failed: {0}")]
    ComplianceFailed(String),
}
