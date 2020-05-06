use bytes::BytesMut;
use std::{error, fmt, io};

#[derive(Debug, PartialEq)]
pub enum RequestCode {
    Ping,
    GetStats,
    ResetStats,
    Compress(BytesMut),
}

#[derive(Debug, PartialEq)]
pub enum StatusCode {
    Ok(BytesMut), // BytesMut may be empty
    #[allow(dead_code)]
    UnknownError, // UnknownError is never used
    MessageTooLarge,
    UnsupportedRequestType,
    EmptyBuffer, // these implementation specific status codes start at 33
    NonEmptyBuffer,
    NonAscii,
    NonAlphabetic,
    NonLowerCase,
    IoError(io::ErrorKind),
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StatusCode: {:?}", self)
    }
}

impl From<io::Error> for StatusCode {
    fn from(error: io::Error) -> Self {
        StatusCode::IoError(error.kind())
    }
}

impl error::Error for StatusCode {} // also use status codes for errors
