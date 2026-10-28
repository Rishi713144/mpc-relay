#![allow(deprecated)]
use std::fmt::{Display, Formatter};

use curv::elliptic::curves::{DeserializationError, Point, PointFromBytesError, Scalar};
use multi_party_eddsa::protocols::musig2::{PrivatePartialNonces, PublicPartialNonces};
use solana_pubkey::Pubkey;
use solana_signature::Signature;

#[derive(Debug)]
pub enum Error {
    BadBase64(String),
    FailedDeserialization(String),
    InputTooShort { expected: usize, found: usize },
    BadBase58(bs58::decode::Error),
    InvalidPoint(PointFromBytesError),
    InvalidScalar(DeserializationError),
    WrongTag { expected: Tag, found: Tag },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    AggMessage1 = 0,
    AggMessage2 = 1,
    PartialSignature = 2,
    SecretAggStepOne = 3,
    Unknown,
}

impl From<u8> for Tag {
    fn from(t: u8) -> Self {
        match t {
            _ if t == Tag::AggMessage1 as u8 => Tag::AggMessage1,
            _ if t == Tag::AggMessage2 as u8 => Tag::AggMessage2,
            _ if t == Tag::PartialSignature as u8 => Tag::PartialSignature,
            _ if t == Tag::SecretAggStepOne as u8 => Tag::SecretAggStepOne,
            _ => Tag::Unknown,
        }
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Tag::AggMessage1 => f.write_str("Aggregate Message1"),
            Tag::AggMessage2 => f.write_str("Aggregate Message2"),
            Tag::PartialSignature => f.write_str("Partial Signature"),
            Tag::SecretAggStepOne => f.write_str("Secret State Aggregate1"),
            Tag::Unknown => f.write_str("Unknown"),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedDeserialization(e) => write!(f, "Failed deserialization: {e}"),
            Self::BadBase64(e) => write!(f, "Invalid base64: {e}"),
            Self::InputTooShort { expected, found } => {
                write!(f, "Input too short, expected: {expected}, found: {found}")
            }
            Self::BadBase58(e) => write!(f, "Invalid base58: {e}"),
            Self::InvalidPoint(e) => write!(f, "Invalid Ed25519 Point: {e}"),
            Self::InvalidScalar(e) => write!(f, "Invalid Ed25519 Scalar: {e}"),
            Self::WrongTag { expected, found } => {
                write!(f, "Expected to find message: {expected}, instead found: {found}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<PointFromBytesError> for Error {
    fn from(e: PointFromBytesError) -> Self {
        Self::InvalidPoint(e)
    }
}

impl From<DeserializationError> for Error {
    fn from(e: DeserializationError) -> Self {
        Self::InvalidScalar(e)
    }
}

pub trait FieldError<T> {
    fn with_field(self, field_name: &'static str) -> Result<T, crate::error::Error>;
}

impl<T: SerializeCustom> FieldError<T> for Result<T, Error> {
    fn with_field(self, field_name: &'static str) -> Result<T, crate::error::Error> {
        self.map_err(|error| crate::error::Error::DeserializationFailed { error, field_name })
    }
}

pub trait SerializeCustom: Sized {
    fn serialize_bs58(&self) -> String {
        let mut vec = Vec::with_capacity(self.size_hint());
        self.serialize(&mut vec);
        bs58::encode(vec).into_string()
    }

    fn serialize(&self, append_to: &mut Vec<u8>);

    fn deserialize_bs58(s: impl AsRef<[u8]>) -> Result<Self, Error> {
        let out = bs58::decode(s).into_vec().map_err(Error::BadBase58)?;
        Self::deserialize(&out)
    }

    fn deserialize(b: &[u8]) -> Result<Self, Error>;

    fn size_hint(&self) -> usize;
}

#[derive(Debug, PartialEq)]
pub struct AggMessage1 {
    pub public_nonces: PublicPartialNonces,
    pub sender: Pubkey,
}

impl SerializeCustom for AggMessage1 {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.push(Tag::AggMessage1 as u8);
        append_to.extend(&*self.public_nonces.R[0].to_bytes(true));
        append_to.extend(&*self.public_nonces.R[1].to_bytes(true));
        append_to.extend(self.sender.to_bytes());
    }

    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 1 + 32 + 32 + 32 {
            return Err(Error::InputTooShort {
                expected: 1 + 32 + 32 + 32,
                found: b.len(),
            });
        }
        let tag = Tag::from(b[0]);
        if tag != Tag::AggMessage1 {
            return Err(Error::WrongTag {
                expected: Tag::AggMessage1,
                found: tag,
            });
        }
        let public_nonces = PublicPartialNonces {
            R: [
                Point::from_bytes(&b[1..32 + 1])?,
                Point::from_bytes(&b[1 + 32..64 + 1])?,
            ],
        };
        let sender = Pubkey::try_from(&b[64 + 1..64 + 32 + 1]).unwrap();
        Ok(Self {
            public_nonces,
            sender,
        })
    }

    fn size_hint(&self) -> usize {
        1 + 32 + 32 + 32
    }
}

#[derive(Debug, PartialEq)]
pub struct PartialSignature(pub Signature);

impl SerializeCustom for PartialSignature {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.push(Tag::PartialSignature as u8);
        append_to.extend(self.0.as_ref());
    }

    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 1 + 64 {
            return Err(Error::InputTooShort {
                expected: 1 + 64,
                found: b.len(),
            });
        }
        let tag = Tag::from(b[0]);
        if tag != Tag::PartialSignature {
            return Err(Error::WrongTag {
                expected: Tag::PartialSignature,
                found: tag,
            });
        }
        Ok(PartialSignature(Signature::try_from(&b[1..1 + 64]).unwrap()))
    }

    fn size_hint(&self) -> usize {
        1 + 64
    }
}

#[derive(Debug, PartialEq)]
pub struct SecretAggStepOne {
    pub private_nonces: PrivatePartialNonces,
    pub public_nonces: PublicPartialNonces,
}

impl SerializeCustom for SecretAggStepOne {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.push(Tag::SecretAggStepOne as u8);

        append_to.extend(&*self.private_nonces.r[0].to_bytes());
        append_to.extend(&*self.private_nonces.r[1].to_bytes());
        append_to.extend(&*self.public_nonces.R[0].to_bytes(true));
        append_to.extend(&*self.public_nonces.R[1].to_bytes(true));
    }

    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 1 + 64 + 64 {
            return Err(Error::InputTooShort {
                expected: 1 + 64 + 64,
                found: b.len(),
            });
        }

        let tag = Tag::from(b[0]);
        if tag != Tag::SecretAggStepOne {
            return Err(Error::WrongTag {
                expected: Tag::SecretAggStepOne,
                found: tag,
            });
        }
        let private_nonces = PrivatePartialNonces {
            r: [
                Scalar::from_bytes(&b[1..1 + 32])?,
                Scalar::from_bytes(&b[1 + 32..1 + 64])?,
            ],
        };
        #[allow(non_snake_case)]
        let public_nonces = PublicPartialNonces {
            R: [
                Point::from_bytes(&b[1 + 64..1 + 64 + 32])?,
                Point::from_bytes(&b[1 + 96..1 + 96 + 32])?,
            ],
        };
        Ok(Self {
            private_nonces,
            public_nonces,
        })
    }

    fn size_hint(&self) -> usize {
        1 + 64 + 64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use curv::elliptic::curves::{Ed25519, Point};
    use solana_keypair::Keypair;
    use solana_signer::Signer;

    #[test]
    fn agg_message_roundtrip() {
        let keypair = Keypair::new();
        let point = Point::<Ed25519>::generator();
        let msg = AggMessage1 {
            public_nonces: PublicPartialNonces {
                R: [point.clone().into(), point.into()],
            },
            sender: keypair.pubkey(),
        };
        let encoded = msg.serialize_bs58();
        let decoded = AggMessage1::deserialize_bs58(encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn rejects_truncated_payload() {
        assert!(AggMessage1::deserialize(&[0u8; 4]).is_err());
    }
}
