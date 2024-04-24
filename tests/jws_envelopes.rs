#[cfg(feature = "raw-crypto")]
mod tests {

    extern crate chacha20poly1305;
    extern crate sodiumoxide;

    #[cfg(feature = "resolve")]
    pub use ddoresolver_rs::*;
    use didcomm_rs::crypto::{SignatureAlgorithm, Signer};
    use didcomm_rs::{Error, Message};

    use k256::ecdsa::signature::Keypair;
    use rand_core::OsRng;
    use serde_json::Value;

    #[test]
    fn can_create_flattened_jws_json() -> Result<(), Error> {
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let jws_string = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes()))
            .as_flat_jws(&SignatureAlgorithm::EdDsa)
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes())?;

        let jws_object: Value = serde_json::from_str(&jws_string)?;

        assert!(jws_object["signature"].as_str().is_some());
        assert!(jws_object["signatures"].as_array().is_none());

        Ok(())
    }

    #[test]
    fn can_create_general_jws_json() -> Result<(), Error> {
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let jws_string = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes()))
            .as_jws(&SignatureAlgorithm::EdDsa)
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes())?;

        let jws_object: Value = serde_json::from_str(&jws_string)?;

        assert!(jws_object["signature"].as_str().is_none());
        assert!(jws_object["signatures"].as_array().is_some());

        Ok(())
    }

    #[test]
    fn can_receive_flattened_jws_json() -> Result<(), Error> {
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let jws_string = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes()))
            .as_flat_jws(&SignatureAlgorithm::EdDsa)
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes())?;

        // 'verify' style receive
        let received = Message::verify(jws_string.as_bytes(), &sign_keypair.verifying_key().to_bytes());
        assert!(received.is_ok());

        // generic 'receive' style
        let received = Message::receive(
            &jws_string,
            Some(&[]),
            Some(sign_keypair.verifying_key().as_bytes().to_vec()),
            None,
        );
        assert!(received.is_ok());

        Ok(())
    }

    #[test]
    fn can_receive_general_jws_json() -> Result<(), Error> {
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let jws_string = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes()))
            .as_jws(&SignatureAlgorithm::EdDsa)
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes())?;

        // 'verify' style receive
        let received = Message::verify(jws_string.as_bytes(), &sign_keypair.verifying_key().to_bytes());
        assert!(received.is_ok());

        // generic 'receive' style
        let received = Message::receive(
            &jws_string,
            Some(&[]),
            Some(sign_keypair.verifying_key().to_bytes().to_vec()),
            None,
        );
        assert!(received.is_ok());

        Ok(())
    }
}
