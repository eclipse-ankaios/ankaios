use std::fmt;

use tokio::task::JoinError;

pub struct CommunicationMiddlewareError(pub String);

impl From<JoinError> for CommunicationMiddlewareError {
    fn from(error: JoinError) -> Self {
        CommunicationMiddlewareError(error.to_string())
    }
}

impl fmt::Display for CommunicationMiddlewareError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            CommunicationMiddlewareError(message) => {
                write!(f, "{}", message)
            }
        }
    }
}
