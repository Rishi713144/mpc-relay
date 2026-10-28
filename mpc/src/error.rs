use std::fmt::{Display, Formatter};
use std::io::Error as IoError;

use actix_web::{
    HttpResponse, ResponseError,
    http::{StatusCode, header::ContentType},
};
use bs58::decode::Error as Bs58Error;

use crate::serialization::Error as DeserializationError;

#[derive(Debug)]
pub enum Error {
    SerializationFailed(String),
    SigningFailed(String),
    WrongNetwork(String),
    BadBase58(Bs58Error),
    InternalServerError(String),
    WrongKeyPair(ed25519_dalek::SignatureError),
    DeserializationFailed {
        error: DeserializationError,
        field_name: &'static str,
    },
    MismatchMessages,
    InvalidSignature,
    KeyPairIsNotInKeys,
    HttpCreation(IoError),
    BadRequest(String),
    Unauthorized,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InternalServerError(msg) => write!(f, "Internal server error: {msg}"),
            Self::SerializationFailed(msg) => write!(f, "Failed serializing data: {msg}"),
            Self::SigningFailed(msg) => write!(f, "Failed signing message: {msg}"),
            Self::WrongNetwork(net) => write!(f, "Unrecognized network: {net}"),
            Self::BadBase58(e) => write!(f, "Base58 error: {e}"),
            Self::WrongKeyPair(e) => write!(f, "Failed deserializing keypair: {e}"),
            Self::DeserializationFailed { error, field_name } => {
                write!(f, "Failed deserializing {field_name}: {error}")
            }
            Self::MismatchMessages => {
                write!(f, "There is a mismatch between partial signature nonces")
            }
            Self::InvalidSignature => write!(f, "The resulting signature is invalid"),
            Self::KeyPairIsNotInKeys => {
                write!(f, "The provided keypair is not in the list of pubkeys")
            }
            Self::HttpCreation(e) => write!(f, "Failed creating the HTTP server: {e}"),
            Self::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            Self::Unauthorized => write!(f, "Unauthorized"),
        }
    }
}

impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::InternalServerError(_)
            | Self::SerializationFailed(_)
            | Self::HttpCreation(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .content_type(ContentType::plaintext())
            .body(self.to_string())
    }
}

impl From<Bs58Error> for Error {
    fn from(e: Bs58Error) -> Self {
        Self::BadBase58(e)
    }
}

impl From<ed25519_dalek::SignatureError> for Error {
    fn from(e: ed25519_dalek::SignatureError) -> Self {
        Self::WrongKeyPair(e)
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Self::InternalServerError(format!("database error: {e}"))
    }
}

impl std::error::Error for Error {}
