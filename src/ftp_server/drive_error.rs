use thiserror::Error;

#[derive(Error, Debug)]
pub enum DriveError {
    #[error("")]
    Disconnect(),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] std::num::ParseIntError),

    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Missing parameter error: {0}")]
    MissingParameter(&'static str),

    #[error("Protocol violation error: {0}")]
    ProtocolViolation(&'static str),

    #[error("File system error: {0}")]
    FileSystem(String),

    #[error("General error: {0}")]
    Custom(String),
}
