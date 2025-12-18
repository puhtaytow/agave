//! Put Alpenglow consensus messages here so all clients can agree on the format.
use {
    crate::vote::Vote,
    serde::{Deserialize, Serialize},
    solana_bls_signatures::Signature as BLSSignature,
    solana_clock::Slot,
    solana_hash::Hash,
};

/// The seed used to derive the BLS keypair
pub const BLS_KEYPAIR_DERIVE_SEED: &[u8; 9] = b"alpenglow";

/// Block, a (slot, hash) tuple
pub type Block = (Slot, Hash);

/// A consensus vote.
#[cfg_attr(
    feature = "frozen-abi",
    derive(AbiExample, StableAbi),
    frozen_abi(
        api_digest = "A9wHKYuPgAR7cxidTT51ACVv5WNqHkfj2jVqJLGBC5bv",
        abi_digest = "HBKh1X4nMVewJDFL5GA7zE5MRdm9GKa7FDvJZdHjC3bi"
    )
)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoteMessage {
    /// The type of the vote.
    pub vote: Vote,
    /// The signature.
    pub signature: BLSSignature,
    /// The rank of the validator.
    pub rank: u16,
}

#[cfg(feature = "frozen-abi")]
impl<'a> arbitrary::Arbitrary<'a> for VoteMessage {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            vote: u.arbitrary()?,
            signature: solana_bls_signatures::signature::Signature(u.arbitrary()?),
            rank: u.arbitrary()?,
        })
    }
}

/// The different types of certificates and their relevant state.
#[cfg_attr(
    feature = "frozen-abi",
    derive(AbiExample, AbiEnumVisitor, StableAbi),
    frozen_abi(
        api_digest = "CazjewshYYizgQuCgBBRv6gzasJpUvFVKoSeEirWRKgA",
        abi_digest = "3FsH8efdYXRgfbrskhQEWW2iKtxK83inGLCsE792w6Xp"
    )
)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum CertificateType {
    /// Finalize certificate
    Finalize(Slot),
    /// Fast finalize certificate
    FinalizeFast(Slot, Hash),
    /// Notarize certificate
    Notarize(Slot, Hash),
    /// Notarize fallback certificate
    NotarizeFallback(Slot, Hash),
    /// Skip certificate
    Skip(Slot),
    /// Genesis certificate
    Genesis(Slot, Hash),
}

#[cfg(feature = "frozen-abi")]
impl<'a> arbitrary::Arbitrary<'a> for CertificateType {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let variant: u8 = u.int_in_range(0..=5)?;
        match variant {
            0 => Ok(Self::Finalize(u.arbitrary()?)),
            1 => Ok(Self::FinalizeFast(
                u.arbitrary()?,
                Hash::new_from_array(u.arbitrary()?),
            )),
            2 => Ok(Self::Notarize(
                u.arbitrary()?,
                Hash::new_from_array(u.arbitrary()?),
            )),
            3 => Ok(Self::NotarizeFallback(
                u.arbitrary()?,
                Hash::new_from_array(u.arbitrary()?),
            )),
            4 => Ok(Self::Skip(u.arbitrary()?)),
            _ => Ok(Self::Genesis(
                u.arbitrary()?,
                Hash::new_from_array(u.arbitrary()?),
            )),
        }
    }
}

impl CertificateType {
    /// Get the slot of the certificate
    pub fn slot(&self) -> Slot {
        match self {
            Self::Finalize(slot)
            | Self::FinalizeFast(slot, _)
            | Self::Notarize(slot, _)
            | Self::NotarizeFallback(slot, _)
            | Self::Skip(slot)
            | Self::Genesis(slot, _) => *slot,
        }
    }

    /// Gets the block associated with this certificate, if present
    pub fn to_block(self) -> Option<Block> {
        match self {
            Self::Finalize(_) | Self::Skip(_) => None,
            Self::Notarize(slot, block_id)
            | Self::NotarizeFallback(slot, block_id)
            | Self::FinalizeFast(slot, block_id)
            | Self::Genesis(slot, block_id) => Some((slot, block_id)),
        }
    }
}

/// The actual certificate with the aggregate signature and bitmap for which validators are included in the aggregate.
/// BLS vote message, we need rank to look up pubkey
#[cfg_attr(
    feature = "frozen-abi",
    derive(AbiExample, StableAbi),
    frozen_abi(
        api_digest = "CLJbmbTECu2MeBmqWNDsfTgkAC2yudxHsmNU9saww8L",
        abi_digest = "8kkzEBFhWYa2Cz7NQLFMKqBsAJPq481ZTn1PuzZXgv7B"
    )
)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Certificate {
    /// The certificate type.
    pub cert_type: CertificateType,
    /// The aggregate signature.
    pub signature: BLSSignature,
    /// A rank bitmap for validators' signatures included in the aggregate.
    /// See solana-signer-store for encoding format.
    pub bitmap: Vec<u8>,
}

#[cfg(feature = "frozen-abi")]
impl<'a> arbitrary::Arbitrary<'a> for Certificate {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            cert_type: u.arbitrary()?,
            signature: solana_bls_signatures::signature::Signature(u.arbitrary()?),
            bitmap: u.arbitrary()?,
        })
    }
}

/// A consensus message sent between validators.
#[cfg_attr(
    feature = "frozen-abi",
    derive(AbiExample, AbiEnumVisitor, StableAbi, arbitrary::Arbitrary),
    frozen_abi(
        api_digest = "4YvBgNbve59tf9i4DSraiSZ3eoMF4Y1V5mDdUCoFv8S2",
        abi_digest = "Fdbxxd4CQ8hBJdXBTitD21Vud8DnxvuBeUiEffFvWRbs"
    )
)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum ConsensusMessage {
    /// A vote from a single party.
    Vote(VoteMessage),
    /// A certificate aggregating votes from multiple parties.
    Certificate(Certificate),
}

impl ConsensusMessage {
    /// Create a new vote message
    pub fn new_vote(vote: Vote, signature: BLSSignature, rank: u16) -> Self {
        Self::Vote(VoteMessage {
            vote,
            signature,
            rank,
        })
    }
}
