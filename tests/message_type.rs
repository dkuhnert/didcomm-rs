#[cfg(feature = "raw-crypto")]
extern crate chacha20poly1305;
#[cfg(feature = "raw-crypto")]
extern crate didcomm_rs;

use didcomm_rs::{Error, Message};
use serde_json::Value;

#[test]
fn sets_message_type_correctly_for_plain_messages() -> Result<(), Error> {
    let message = Message::new()
        .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
        .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"]);

    let jwm_string: String = serde_json::to_string(&message)?;
    let jwm_object: Value = serde_json::from_str(&jwm_string)?;

    assert!(jwm_object["typ"].as_str().is_some());
    assert_eq!(
        jwm_object["typ"]
            .as_str()
            .ok_or(Error::JwmHeaderParseError)?,
        "application/didcomm-plain+json",
    );

    Ok(())
}

#[cfg(feature = "raw-crypto")]
mod tests {
    use std::str::from_utf8;

    use didcomm_rs::{
        crypto::{CryptoAlgorithm, SignatureAlgorithm, Signer},
        Error, JwmHeader, Message, MessageType,
    };
    use rand_core::OsRng;
    use serde_json::Value;
    use utilities::{get_keypair_set, KeyPairSet};

    #[test]
    #[cfg(feature = "raw-crypto")]
    fn sets_message_type_correctly_for_signed_messages() -> Result<(), Error> {
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let jws_string = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_flat_jws(&SignatureAlgorithm::EdDsa)
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes())?;

        let jws_object: Value = serde_json::from_str(&jws_string)?;
        assert!(jws_object["protected"].as_str().is_some());
        let protected_encoded = jws_object["protected"]
            .as_str()
            .ok_or(Error::JwmHeaderParseError)?;
        let protected_decoded_buffer = base64_url::decode(&protected_encoded.as_bytes())?;
        let protected_decoded_string =
            from_utf8(&protected_decoded_buffer).map_err(|_| Error::JwsParseError)?;
        let protected_object: Value = serde_json::from_str(protected_decoded_string)?;

        assert_eq!(
            protected_object["typ"]
                .as_str()
                .ok_or(Error::JwmHeaderParseError)?,
            "application/didcomm-signed+json",
        );

        Ok(())
    }

    #[test]
    #[cfg(feature = "raw-crypto")]
    fn sets_message_type_correctly_for_signed_and_encrypted_messages() -> Result<(), Error> {
        let KeyPairSet {
            alice_private,
            bobs_public,
            ..
        } = get_keypair_set();
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let message = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()))
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes()));

        let jwe_string = message.seal_signed(
            &alice_private,
            Some(vec![Some(bobs_public.to_vec())]),
            SignatureAlgorithm::EdDsa,
            &sign_keypair.to_bytes(),
        )?;

        let jwe_object: Value = serde_json::from_str(&jwe_string)?;

        assert!(jwe_object["protected"].as_str().is_some());
        let protected_encoded = jwe_object["protected"]
            .as_str()
            .ok_or(Error::JwmHeaderParseError)?;
        let protected_decoded_buffer = base64_url::decode(&protected_encoded.as_bytes())?;
        let protected_decoded_string =
            from_utf8(&protected_decoded_buffer).map_err(|_| Error::JwsParseError)?;
        let protected_object: Value = serde_json::from_str(protected_decoded_string)?;

        assert_eq!(
            protected_object["typ"]
                .as_str()
                .ok_or(Error::JwmHeaderParseError)?,
            "application/didcomm-encrypted+json",
        );

        Ok(())
    }

    #[test]
    #[cfg(feature = "raw-crypto")]
    fn sets_message_type_correctly_for_forwarded_messages() -> Result<(), Error> {
        let KeyPairSet {
            alice_private,
            bobs_public,
            mediators_public,
            ..
        } = get_keypair_set();
        let message = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()));

        let jwe_string = message.routed_by(
            &alice_private,
            Some(vec![Some(bobs_public.to_vec())]),
            "did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf",
            Some(mediators_public.to_vec()),
        )?;

        let jwe_object: Value = serde_json::from_str(&jwe_string)?;

        assert!(jwe_object["protected"].as_str().is_some());
        let protected_encoded = jwe_object["protected"]
            .as_str()
            .ok_or(Error::JwmHeaderParseError)?;
        let protected_decoded_buffer = base64_url::decode(&protected_encoded.as_bytes())?;
        let protected_decoded_string =
            from_utf8(&protected_decoded_buffer).map_err(|_| Error::JwsParseError)?;
        let protected_object: Value = serde_json::from_str(protected_decoded_string)?;

        assert_eq!(
            protected_object["typ"]
                .as_str()
                .ok_or(Error::JwmHeaderParseError)?,
            "https://didcomm.org/routing/2.0/forward",
        );

        Ok(())
    }

    #[test]
    #[cfg(feature = "raw-crypto")]
    fn keeps_inner_message_type_as_plain_for_signed_messages() -> Result<(), Error> {
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let jws_string = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jws(&SignatureAlgorithm::EdDsa)
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes()))
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes())?;

        let jws_object: Value = serde_json::from_str(&jws_string)?;
        let jws_protected_encoded = jws_object
            .get("signatures")
            .ok_or(Error::JwsParseError)?
            .as_array()
            .ok_or(Error::JwsParseError)?[0]
            .as_object()
            .ok_or(Error::JwsParseError)?
            .get("protected")
            .ok_or(Error::JwsParseError)?
            .as_str()
            .ok_or(Error::JwsParseError)?;
        let jws_protected_string_decoded = base64_url::decode(&jws_protected_encoded)?;
        let jws_jwm_header: JwmHeader = serde_json::from_slice(&jws_protected_string_decoded)?;

        let payload_string_encoded = jws_object
            .get("payload")
            .ok_or(Error::JwsParseError)?
            .as_str()
            .ok_or(Error::JwsParseError)?;
        let payload_string_decoded = base64_url::decode(&payload_string_encoded)?;
        let payload_jwm_header: JwmHeader = serde_json::from_slice(&payload_string_decoded)?;
        let received_message = Message::receive(&jws_string, None, None, None)?;

        assert_eq!(jws_jwm_header.typ, MessageType::DidCommJws);
        assert_eq!(payload_jwm_header.typ, MessageType::DidCommRaw);
        assert_eq!(
            received_message.get_jwm_header().typ,
            MessageType::DidCommRaw
        );

        Ok(())
    }
}

#[test]
fn serializes_missing_body_as_empty_object() -> Result<(), Error> {
    let message = Message::new()
        .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
        .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"]);

    let jwm_string: String = serde_json::to_string(&message)?;
    let jwm_object: Value = serde_json::from_str(&jwm_string)?;

    assert!(jwm_object["body"].as_object().is_some());
    assert_eq!(serde_json::to_string(&jwm_object["body"])?, "{}",);

    Ok(())
}

#[test]
fn serializes_existing_body_as_object() -> Result<(), Error> {
    let message = Message::new()
        .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
        .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
        .body(r#"{"foo":"bar"}"#)?;

    let jwm_string: String = serde_json::to_string(&message)?;
    let jwm_object: Value = serde_json::from_str(&jwm_string)?;

    assert!(jwm_object["body"].as_object().is_some());
    assert_eq!(
        serde_json::to_string(&jwm_object["body"])?,
        r#"{"foo":"bar"}"#,
    );

    Ok(())
}
