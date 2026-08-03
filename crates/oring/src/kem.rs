use rand_core::CryptoRng;
use zeroize::Zeroize;

/// Curve-generic key-encapsulation mechanism.
///
/// Static dispatch only: the generic rng parameter makes this trait not
/// dyn-compatible, by design.
pub trait Kem {
    /// Recipient long-term public key.
    type PublicKey;

    /// Recipient long-term secret key. The crate borrows it; storage
    /// hygiene is the consumer's responsibility.
    type SecretKey;

    /// Ephemeral public key carried in the envelope alongside the ciphertext.
    type Epk: AsRef<[u8]>;

    /// Per-envelope KEM output fed into HKDF. The crate calls
    /// [`Zeroize::zeroize`] on every value it owns once that value has been
    /// consumed, so an implementor is free to derive `Zeroize` alone.
    type SharedSecret: Zeroize;

    /// Wire identifier for this KEM, carried in the envelope header.
    const KEM_ID: u8;

    /// Byte length of `Epk`.
    const EPK_LEN: usize;

    /// Encapsulates a fresh shared secret to `to`, returning the ephemeral
    /// public key and the shared secret.
    fn encap(
        rng: &mut impl CryptoRng,
        to: &Self::PublicKey,
    ) -> (Self::Epk, Self::SharedSecret);

    /// Decapsulates `epk` under `sk`.
    ///
    /// Returns `None` for invalid points and for non-contributory results,
    /// for example the all-zero X25519 output produced by a low-order point.
    fn decap(sk: &Self::SecretKey, epk: &[u8]) -> Option<Self::SharedSecret>;

    /// Encodes `pk` in the same format as `Epk`.
    fn encode_pk(pk: &Self::PublicKey) -> Self::Epk;

    /// Decapsulates a batch of ephemeral keys, one output slot per input.
    ///
    /// The default is a scalar loop over [`decap`](Self::decap); adapters
    /// that can batch field inversions override this.
    fn decap_batch(
        sk: &Self::SecretKey,
        epks: &[&[u8]],
        out: &mut [Option<Self::SharedSecret>],
    ) {
        for (epk, slot) in epks.iter().zip(out) {
            *slot = Self::decap(sk, epk);
        }
    }
}
