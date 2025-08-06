use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum ProxyError {
    Io(io::Error),
    ServerInit(String),
    ConnectionFailed(String),
    DataTransfer(String),
}

impl fmt::Display for ProxyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyError::Io(e) => write!(f, "IO error: {}", e),
            ProxyError::ServerInit(msg) => write!(f, "Server initialization failed: {}", msg),
            ProxyError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            ProxyError::DataTransfer(msg) => write!(f, "Data transfer error: {}", msg),
        }
    }
}

impl StdError for ProxyError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            ProxyError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ProxyError {
    fn from(error: io::Error) -> Self {
        ProxyError::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, ProxyError>;