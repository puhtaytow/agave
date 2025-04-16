use {
    serde_derive::{Deserialize, Serialize},
    solana_config_program_client::instructions_bincode::ConfigState,
    solana_hash::Hash,
    solana_keypair::signable::Signable,
    solana_pubkey::Pubkey,
    solana_signature::Signature,
    std::{borrow::Cow, error, fmt},
};

#[derive(Debug)]
pub enum UpdateManifestError {
    Deserialization(bincode::Error),
    VerificationFailed,
}

impl fmt::Display for UpdateManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateManifestError::Deserialization(e) => {
                write!(f, "Failed to deserialize manifest: {}", e)
            }
            UpdateManifestError::VerificationFailed => write!(f, "Manifest verification failed"),
        }
    }
}

impl error::Error for UpdateManifestError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            UpdateManifestError::Deserialization(e) => Some(e),
            UpdateManifestError::VerificationFailed => None,
        }
    }
}

impl From<bincode::Error> for UpdateManifestError {
    fn from(e: bincode::Error) -> Self {
        UpdateManifestError::Deserialization(e)
    }
}

/// Information required to download and apply a given update
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct UpdateManifest {
    pub timestamp_secs: u64, // When the release was deployed in seconds since UNIX EPOCH
    pub download_url: String, // Download URL to the release tar.bz2
    pub download_sha256: Hash, // SHA256 digest of the release tar.bz2 file
}

/// Data of an Update Manifest program Account.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct SignedUpdateManifest {
    pub manifest: UpdateManifest,
    pub manifest_signature: Signature,
    #[serde(skip)]
    pub account_pubkey: Pubkey,
}

impl Signable for SignedUpdateManifest {
    fn pubkey(&self) -> Pubkey {
        self.account_pubkey
    }

    fn signable_data(&self) -> Cow<[u8]> {
        Cow::Owned(bincode::serialize(&self.manifest).expect("serialize"))
    }
    fn get_signature(&self) -> Signature {
        self.manifest_signature
    }
    fn set_signature(&mut self, signature: Signature) {
        self.manifest_signature = signature
    }
}

impl SignedUpdateManifest {
    pub fn deserialize(account_pubkey: &Pubkey, input: &[u8]) -> Result<Self, UpdateManifestError> {
        let mut manifest: SignedUpdateManifest = bincode::deserialize(input)?;
        manifest.account_pubkey = *account_pubkey;
        if !manifest.verify() {
            Err(UpdateManifestError::VerificationFailed)
        } else {
            Ok(manifest)
        }
    }
}

impl ConfigState for SignedUpdateManifest {
    fn max_space() -> u64 {
        256 // Enough space for a fully populated SignedUpdateManifest
    }
}
