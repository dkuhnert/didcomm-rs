use std::convert::{TryFrom, TryInto};

use super::*;

/// Signature related batteries for DIDComm.
/// Implementation of all algorithms required by (spec)[https://identity.foundation/didcomm-messaging/spec/#algorithms]
#[derive(Debug, Clone)]
pub enum SignatureAlgorithm {
    /// `ed25519` signature
    EdDsa,
    /// `ECDSA/P-256` NIST signature
    Es256,
    /// `ECDSA/secp256k1` signature
    Es256k,
}

impl Signer for SignatureAlgorithm {
    /// Builds signer FnOnce, which performs signing.
    ///
    /// # Examples
    /// ```
    /// # fn main() {
    /// use didcomm_rs::crypto::{SignatureAlgorithm, Signer};
    /// let signer = SignatureAlgorithm::Es256k.signer();
    /// # }
    ///```
    fn signer(&self) -> SigningMethod {
        match self {
            // an &[u8] representing the scalar for the secret key, and a compressed Edwards-Y coordinate of a point on curve25519, both as bytes.
            SignatureAlgorithm::EdDsa => {
                Box::new(|key: &[u8], message: &[u8]| -> Result<Vec<u8>, Error> {
                    use ed25519_dalek::{Signer, SigningKey, SECRET_KEY_LENGTH};
                    let key = SigningKey::from_bytes(
                        key.try_into().map_err(|_| Error::InvalidKeySize(format!("ed25519 expects key size of {}", SECRET_KEY_LENGTH)))?
                    );
                    let s = key.sign(message);
                    Ok(s.to_bytes().to_vec())
                })
            }
            SignatureAlgorithm::Es256 => {
                Box::new(|key: &[u8], message: &[u8]| -> Result<Vec<u8>, Error> {
                    use p256::ecdsa::{signature::Signer, Signature, SigningKey};
                    let sk = SigningKey::from_bytes(
                        key.try_into().map_err(|_| Error::InvalidKeySize(format!("p256 invalid key size")))?
                    )?;
                    let signature: Signature = sk.sign(message);
                    Ok(signature.to_bytes().to_vec())
                })
            }
            SignatureAlgorithm::Es256k => {
                Box::new(|key: &[u8], message: &[u8]| -> Result<Vec<u8>, Error> {
                    use k256::ecdsa::{signature::Signer, Signature, SigningKey};
                    let sk = SigningKey::from_bytes(
                        key.try_into().map_err(|_| Error::InvalidKeySize(format!("k256 invalid key size")))?
                    ).map_err(|e| Error::Generic(e.to_string()))?;
                    let signature: Signature = sk.sign(message);
                    Ok(signature.to_bytes().to_vec())
                })
            }
        }
    }

    /// Builds validator FnOnce, which performs signature validation.
    ///
    /// # Examples
    /// ```
    /// # fn main() {
    /// use didcomm_rs::crypto::{Signer, SignatureAlgorithm};
    /// let validator = SignatureAlgorithm::Es256k.validator();
    /// # }
    /// ```
    fn validator(&self) -> ValidationMethod {
        match self {
            SignatureAlgorithm::EdDsa => Box::new(
                |key: &[u8], message: &[u8], signature: &[u8]| -> Result<bool, Error> {
                    use ed25519_dalek::{VerifyingKey, Signature, Verifier, SECRET_KEY_LENGTH};
                    let ed25519_key = key.try_into()
                        .map_err(|_| Error::InvalidKeySize(format!("ed25519 expects key size of {}", SECRET_KEY_LENGTH)))?;
                    let key = VerifyingKey::from_bytes(ed25519_key)?;
                    let s = Signature::try_from(signature)?;
                    Ok(key.verify(message, &s).is_ok())
                },
            ),
            SignatureAlgorithm::Es256 => Box::new(
                |key: &[u8], message: &[u8], signature: &[u8]| -> Result<bool, Error> {
                    use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
                    let key = VerifyingKey::from_sec1_bytes(key)?;
                    let s = Signature::try_from(signature)?;
                    Ok(key.verify(message, &s).is_ok())
                },
            ),
            SignatureAlgorithm::Es256k => Box::new(
                |key: &[u8], message: &[u8], signature: &[u8]| -> Result<bool, Error> {
                    use k256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
                    let vk = VerifyingKey::from_sec1_bytes(key)?;
                    let signature = Signature::try_from(signature)?;
                    Ok(vk.verify(message, &signature).is_ok())
                },
            ),
        }
    }
}

impl TryFrom<&String> for SignatureAlgorithm {
    type Error = Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        match &value[..] {
            "EdDSA" => Ok(Self::EdDsa),
            "ES256" => Ok(Self::Es256),
            "ES256K" => Ok(Self::Es256k),
            _ => Err(Error::JwsParseError),
        }
    }
}

#[test]
fn es256k_test() {
    use k256::{ecdsa::SigningKey, elliptic_curve::rand_core::OsRng};
    // Arrange
    let sk = SigningKey::random(&mut OsRng);
    let vk = &sk.verifying_key();
    let m = b"this is the message we're signing in this test...";
    // Act
    let signer = SignatureAlgorithm::Es256k.signer();
    let validator = SignatureAlgorithm::Es256k.validator();
    let sk: Vec<u8> = sk.to_bytes().to_vec();
    let vk = vk.to_sec1_bytes().to_vec();
    let signature = signer(&sk, m);
    let validation = validator(&vk, m, &signature.unwrap());
    // Assert
    assert!(&validation.is_ok());
    assert!(validation.unwrap());
}
