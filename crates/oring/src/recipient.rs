use crate::kem::Kem;

/// A recipient secret key together with the public key it derives.
///
/// Suite v1 mixes the recipient public key into the KDF info, so the receive
/// path needs both halves of the keypair.
///
/// Build one per key and keep it: construction pays one public-key derivation
/// and caches the encoded form the KDF consumes, which [`open`](crate::open)
/// would otherwise recompute per envelope.
pub struct Recipient<K: Kem> {
    sk: K::SecretKey,
    pk: K::PublicKey,
    pk_encoded: K::Epk,
}

impl<K: Kem> Recipient<K> {
    /// Builds a recipient from `sk`, deriving its public key.
    pub fn new(sk: K::SecretKey) -> Self {
        let pk = K::derive_pk(&sk);
        let pk_encoded = K::encode_pk(&pk);
        Self { sk, pk, pk_encoded }
    }

    /// The public key senders seal to.
    pub fn public_key(&self) -> &K::PublicKey {
        &self.pk
    }

    /// The secret key this recipient was built from.
    pub fn secret_key(&self) -> &K::SecretKey {
        &self.sk
    }

    /// The public key in `Epk` encoding, the form suite v1 binds into the
    /// KDF info.
    pub(crate) fn pk_encoded(&self) -> &[u8] {
        self.pk_encoded.as_ref()
    }
}
