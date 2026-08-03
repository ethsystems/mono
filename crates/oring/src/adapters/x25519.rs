use rand_core::CryptoRng;
use subtle::ConstantTimeEq;
use x25519_dalek::{
    EphemeralSecret,
    PublicKey,
    StaticSecret,
};
use zeroize::Zeroize;

use crate::kem::Kem;

/// Byte length of a Montgomery-form X25519 point.
const X25519_EPK_LEN: usize = 32;

/// X25519 KEM. Every 32-byte string is a valid input; low-order points
/// produce an all-zero Diffie-Hellman output, rejected in constant time.
pub struct X25519;

/// X25519 Diffie-Hellman output.
pub struct X25519SharedSecret([u8; 32]);

impl AsRef<[u8]> for X25519SharedSecret {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Zeroize for X25519SharedSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for X25519SharedSecret {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl Kem for X25519 {
    type PublicKey = PublicKey;
    type SecretKey = StaticSecret;
    type Epk = [u8; X25519_EPK_LEN];
    type SharedSecret = X25519SharedSecret;

    const KEM_ID: u8 = 0x02;
    const EPK_LEN: usize = X25519_EPK_LEN;

    fn encap(
        rng: &mut impl CryptoRng,
        to: &Self::PublicKey,
    ) -> (Self::Epk, Self::SharedSecret) {
        let ephemeral = EphemeralSecret::random_from_rng(rng);
        let epk = PublicKey::from(&ephemeral);
        let shared = ephemeral.diffie_hellman(to);
        (epk.to_bytes(), X25519SharedSecret(shared.to_bytes()))
    }

    fn decap(sk: &Self::SecretKey, epk: &[u8]) -> Option<Self::SharedSecret> {
        let epk_bytes: [u8; X25519_EPK_LEN] = epk.try_into().ok()?;
        let shared = sk.diffie_hellman(&PublicKey::from(epk_bytes));
        let shared_bytes = shared.to_bytes();
        let is_zero: bool = shared_bytes
            .as_slice()
            .ct_eq([0u8; X25519_EPK_LEN].as_slice())
            .into();
        if is_zero {
            return None;
        }
        Some(X25519SharedSecret(shared_bytes))
    }

    fn encode_pk(pk: &Self::PublicKey) -> Self::Epk {
        pk.to_bytes()
    }
}
