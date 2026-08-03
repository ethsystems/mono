use ark_ec::{
    AffineRepr,
    CurveGroup,
};
use ark_ff::PrimeField;
use ark_grumpkin::{
    Affine,
    Fr,
};
use ark_serialize::{
    CanonicalDeserialize,
    CanonicalSerialize,
};
use rand_core::CryptoRng;
use zeroize::Zeroize;

use crate::kem::Kem;

/// Byte length of a compressed Grumpkin affine point. Its base field is 254
/// bits wide and serializes to 32 bytes with 2 spare bits, exactly enough to
/// carry the sign and infinity flags without an extra byte.
const GRUMPKIN_EPK_LEN: usize = 32;

/// Grumpkin ECDH KEM. Grumpkin's cofactor is 1, so a valid on-curve decode
/// needs no subgroup check; only the identity point is rejected.
pub struct Grumpkin;

/// Grumpkin shared-point x-coordinate, reduced to a fixed-size byte string.
pub struct GrumpkinSharedSecret([u8; 32]);

impl AsRef<[u8]> for GrumpkinSharedSecret {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Zeroize for GrumpkinSharedSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for GrumpkinSharedSecret {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Draws a scalar from 64 bytes of caller-supplied randomness, reduced
/// modulo the Grumpkin scalar field order. The rng never enters arkworks:
/// only the reduced scalar does.
fn random_scalar(rng: &mut impl CryptoRng) -> Fr {
    let mut bytes = [0u8; 64];
    rng.fill_bytes(&mut bytes);
    let scalar = Fr::from_le_bytes_mod_order(&bytes);
    bytes.zeroize();
    scalar
}

/// Compressed encoding of an affine point.
fn compress(point: &Affine) -> [u8; GRUMPKIN_EPK_LEN] {
    let mut out = [0u8; GRUMPKIN_EPK_LEN];
    point
        .serialize_compressed(&mut out[..])
        .expect("grumpkin affine point compresses to GRUMPKIN_EPK_LEN bytes");
    out
}

/// x-coordinate of `point` as a fixed-size byte string, `None` for the
/// identity.
fn x_coordinate(point: &Affine) -> Option<[u8; 32]> {
    let (x, _) = point.xy()?;
    let mut out = [0u8; 32];
    x.serialize_compressed(&mut out[..])
        .expect("grumpkin base field element serializes to 32 bytes");
    Some(out)
}

impl Kem for Grumpkin {
    type PublicKey = Affine;
    type SecretKey = Fr;
    type Epk = [u8; GRUMPKIN_EPK_LEN];
    type SharedSecret = GrumpkinSharedSecret;

    const KEM_ID: u8 = 0x03;
    const EPK_LEN: usize = GRUMPKIN_EPK_LEN;

    /// An identity recipient key yields the all-zero shared secret, which
    /// [`seal`](crate::seal) rejects.
    fn encap(
        rng: &mut impl CryptoRng,
        to: &Self::PublicKey,
    ) -> (Self::Epk, Self::SharedSecret) {
        let esk = random_scalar(rng);
        let epk = (Affine::generator() * esk).into_affine();
        let shared = (*to * esk).into_affine();
        let secret = x_coordinate(&shared).unwrap_or([0u8; 32]);
        (compress(&epk), GrumpkinSharedSecret(secret))
    }

    fn decap(sk: &Self::SecretKey, epk: &[u8]) -> Option<Self::SharedSecret> {
        if epk.len() != GRUMPKIN_EPK_LEN {
            return None;
        }
        let point = Affine::deserialize_compressed(epk).ok()?;
        if point.is_zero() {
            return None;
        }
        let shared = (point * *sk).into_affine();
        Some(GrumpkinSharedSecret(x_coordinate(&shared)?))
    }

    fn encode_pk(pk: &Self::PublicKey) -> Self::Epk {
        compress(pk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epk_len_matches_compressed_size() {
        assert_eq!(GRUMPKIN_EPK_LEN, Affine::generator().compressed_size());
    }
}
