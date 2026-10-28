#![allow(deprecated)]
#![allow(non_snake_case)]

use curv::elliptic::curves::{Ed25519, Point, Scalar};
use multi_party_eddsa::protocols::ExpandedKeyPair;
use multi_party_eddsa::protocols::musig2::{self, PrivatePartialNonces, PublicPartialNonces};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_native_token as native_token;
use solana_pubkey::Pubkey;
use solana_signature::Signature;
use solana_signer::{Signer, SignerError};
use solana_system_interface::instruction as system_instruction;
use solana_transaction::versioned::VersionedTransaction;
use solana_transaction::Transaction;

use crate::error::Error;
use crate::serialization::{
    AggMessage1, Error as DeserializationError, PartialSignature, SecretAggStepOne,
};

/// Build the SOL transfer transaction that all MPC nodes partial-sign.
pub fn create_unsigned_transaction(
    amount_lamports: u64,
    to: &Pubkey,
    payer: &Pubkey,
) -> Transaction {
    let transfer_ix = system_instruction::transfer(payer, to, amount_lamports);
    let message = Message::new(&[transfer_ix], Some(payer));
    Transaction::new_unsigned(message)
}

/// Build the aggregate public key. Pass `key=None` to get the plain aggregate.
pub fn key_agg(keys: Vec<Pubkey>, key: Option<Pubkey>) -> Result<musig2::PublicKeyAgg, Error> {
    let convert_keys = |k: Pubkey| {
        Point::from_bytes(&k.to_bytes()).map_err(|e| Error::DeserializationFailed {
            error: DeserializationError::InvalidPoint(e),
            field_name: "keys",
        })
    };
    let keys: Vec<_> = keys.into_iter().map(convert_keys).collect::<Result<_, _>>()?;
    let key = key.map(convert_keys).unwrap_or_else(|| Ok(keys[0].clone()))?;
    musig2::PublicKeyAgg::key_aggregation_n(keys, &key).ok_or(Error::KeyPairIsNotInKeys)
}

pub fn aggregate_public_key(keys: Vec<Pubkey>) -> Result<Pubkey, Error> {
    let agg = key_agg(keys, None)?;
    Pubkey::try_from(&*agg.agg_public_key.to_bytes(true))
        .map_err(|_| Error::InvalidSignature)
}

/// Round 1: commit to nonces.
pub fn step_one(keypair: &Keypair) -> (AggMessage1, SecretAggStepOne) {
    let extended_keypair = ExpandedKeyPair::create_from_private_key(keypair.secret().to_bytes());
    let (private_nonces, public_nonces) = musig2::generate_partial_nonces(&extended_keypair, None);

    (
        AggMessage1 {
            sender: keypair.pubkey(),
            public_nonces: public_nonces.clone(),
        },
        SecretAggStepOne {
            private_nonces,
            public_nonces,
        },
    )
}

/// Round 2: partial-sign arbitrary message bytes (works for SOL, SPL and Jupiter
/// transactions because the backend passes the serialized message it built).
pub fn step_two_raw(
    keypair: &Keypair,
    message: &[u8],
    keys: Vec<Pubkey>,
    first_messages: Vec<AggMessage1>,
    secret_state: SecretAggStepOne,
) -> Result<PartialSignature, Error> {
    let other_nonces: Vec<_> = first_messages
        .into_iter()
        .map(|msg1| msg1.public_nonces.R)
        .collect();

    let aggkey = key_agg(keys, Some(keypair.pubkey()))?;
    let extended_keypair = ExpandedKeyPair::create_from_private_key(keypair.secret().to_bytes());

    let signer = PartialSigner {
        signer_private_nonce: secret_state.private_nonces,
        signer_public_nonce: secret_state.public_nonces,
        other_nonces,
        extended_kepair: extended_keypair,
        aggregated_pubkey: aggkey,
    };

    let signature = signer
        .try_sign_message(message)
        .map_err(|error| Error::SigningFailed(error.to_string()))?;
    Ok(PartialSignature(signature))
}

/// Round 2 for a legacy SOL transfer.
#[allow(clippy::too_many_arguments)]
pub fn step_two(
    keypair: &Keypair,
    amount_lamports: u64,
    to: Pubkey,
    recent_block_hash: Hash,
    keys: Vec<Pubkey>,
    first_messages: Vec<AggMessage1>,
    secret_state: SecretAggStepOne,
) -> Result<PartialSignature, Error> {
    let aggkey = key_agg(keys.clone(), Some(keypair.pubkey()))?;
    let aggpubkey = Pubkey::try_from(&*aggkey.agg_public_key.to_bytes(true))
        .map_err(|_| Error::InvalidSignature)?;

    let mut tx = create_unsigned_transaction(amount_lamports, &to, &aggpubkey);
    tx.message.recent_blockhash = recent_block_hash;

    step_two_raw(
        keypair,
        &tx.message.serialize(),
        keys,
        first_messages,
        secret_state,
    )
}

/// Round 2 for a Jupiter `VersionedTransaction` (base64 from the swap API).
pub fn step_two_swap(
    keypair: &Keypair,
    jupiter_swap_tx_base64: &str,
    keys: Vec<Pubkey>,
    first_messages: Vec<AggMessage1>,
    secret_state: SecretAggStepOne,
) -> Result<PartialSignature, Error> {
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        jupiter_swap_tx_base64,
    )
    .map_err(|_| Error::DeserializationFailed {
        error: DeserializationError::BadBase64("invalid base64 transaction".to_string()),
        field_name: "swap_transaction",
    })?;

    let tx: VersionedTransaction =
        bincode::deserialize(&bytes).map_err(|_| Error::DeserializationFailed {
            error: DeserializationError::FailedDeserialization(
                "invalid versioned transaction".to_string(),
            ),
            field_name: "swap_transaction",
        })?;

    step_two_raw(
        keypair,
        &tx.message.serialize(),
        keys,
        first_messages,
        secret_state,
    )
}

/// Combine all partial signatures into one Ed25519 signature.
pub fn aggregate_signature(
    keys: Vec<Pubkey>,
    signatures: Vec<PartialSignature>,
) -> Result<Signature, Error> {
    if signatures.is_empty() {
        return Err(Error::BadRequest("no partial signatures provided".into()));
    }
    let _ = key_agg(keys, None)?;

    if !signatures[1..]
        .iter()
        .map(|s| &s.0.as_ref()[..32])
        .all(|s| s == &signatures[0].0.as_ref()[..32])
    {
        return Err(Error::MismatchMessages);
    }

    let deserialize_R = |s| {
        Point::from_bytes(s).map_err(|e| Error::DeserializationFailed {
            error: DeserializationError::InvalidPoint(e),
            field_name: "signatures",
        })
    };
    let deserialize_s = |s| {
        Scalar::from_bytes(s).map_err(|e| Error::DeserializationFailed {
            error: DeserializationError::InvalidScalar(e),
            field_name: "signatures",
        })
    };

    let first_sig = musig2::PartialSignature {
        R: deserialize_R(&signatures[0].0.as_ref()[..32])?,
        my_partial_s: deserialize_s(&signatures[0].0.as_ref()[32..])?,
    };

    let partial_sigs: Vec<_> = signatures[1..]
        .iter()
        .map(|s| deserialize_s(&s.0.as_ref()[32..]))
        .collect::<Result<_, _>>()?;

    let full_sig = musig2::aggregate_partial_signatures(&first_sig, &partial_sigs);

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&*full_sig.R.to_bytes(true));
    sig_bytes[32..].copy_from_slice(&full_sig.s.to_bytes());
    Signature::try_from(sig_bytes).map_err(|_| Error::InvalidSignature)
}

/// Verify and return the signed transaction (legacy SOL transfer).
pub fn sign_and_broadcast(
    amount_lamports: u64,
    to: Pubkey,
    recent_block_hash: Hash,
    keys: Vec<Pubkey>,
    signatures: Vec<PartialSignature>,
) -> Result<Transaction, Error> {
    let aggpubkey = aggregate_public_key(keys.clone())?;
    let sig = aggregate_signature(keys, signatures)?;

    let mut tx = create_unsigned_transaction(amount_lamports, &to, &aggpubkey);
    tx.message.recent_blockhash = recent_block_hash;
    tx.signatures[0] = sig;

    if tx.verify().is_err() {
        return Err(Error::InvalidSignature);
    }
    Ok(tx)
}

struct PartialSigner {
    signer_private_nonce: PrivatePartialNonces,
    signer_public_nonce: PublicPartialNonces,
    other_nonces: Vec<[Point<Ed25519>; 2]>,
    extended_kepair: ExpandedKeyPair,
    aggregated_pubkey: musig2::PublicKeyAgg,
}

impl Signer for PartialSigner {
    fn try_pubkey(&self) -> Result<Pubkey, SignerError> {
        Pubkey::try_from(&*self.aggregated_pubkey.agg_public_key.to_bytes(true))
            .map_err(|e| SignerError::Custom(format!("invalid aggregate pubkey: {e}")))
    }

    fn try_sign_message(&self, message: &[u8]) -> Result<Signature, SignerError> {
        let sig = musig2::partial_sign(
            &self.other_nonces,
            self.signer_private_nonce.clone(),
            self.signer_public_nonce.clone(),
            &self.aggregated_pubkey,
            &self.extended_kepair,
            message,
        );
        let mut sig_bytes = [0u8; 64];
        sig_bytes[..32].copy_from_slice(&*sig.R.to_bytes(true));
        sig_bytes[32..].copy_from_slice(&sig.my_partial_s.to_bytes());
        Signature::try_from(sig_bytes).map_err(|e| SignerError::Custom(e.to_string()))
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

/// Amount helper used by callers that think in SOL.
pub fn sol_to_lamports(amount: f64) -> u64 {
    native_token::sol_to_lamports(amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::SerializeCustom;

    #[test]
    fn three_party_musig_roundtrip() {
        let message = b"mpc-relay test message";

        let keys: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
        let pubkeys: Vec<Pubkey> = keys.iter().map(|k| k.pubkey()).collect();

        let (msgs, secrets): (Vec<_>, Vec<_>) = keys.iter().map(step_one).unzip();
        // Serialize/deserialize like the HTTP layer does, to exercise the wire format.
        let msgs: Vec<AggMessage1> = msgs
            .iter()
            .map(|m| AggMessage1::deserialize_bs58(m.serialize_bs58()).unwrap())
            .collect();

        let partials: Vec<PartialSignature> = keys
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let others: Vec<AggMessage1> = msgs
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, m)| AggMessage1::deserialize_bs58(m.serialize_bs58()).unwrap())
                    .collect();
                let secret = SecretAggStepOne::deserialize_bs58(secrets[i].serialize_bs58()).unwrap();
                step_two_raw(key, message, pubkeys.clone(), others, secret).unwrap()
            })
            .collect();

        let agg = aggregate_public_key(pubkeys.clone()).unwrap();
        let sig = aggregate_signature(pubkeys, partials).unwrap();

        assert!(sig.verify(agg.as_ref(), message));
    }
}
