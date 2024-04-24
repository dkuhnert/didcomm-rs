/// Integration tests of full cycles of message lifetime.
mod common;

#[cfg(feature = "raw-crypto")]
mod tests {
    use super::common::sample_dids;
    #[cfg(not(feature = "resolve"))]
    use didcomm_rs::crypto::{SignatureAlgorithm, Signer};
    use didcomm_rs::{crypto::CryptoAlgorithm, Jwe, Mediated, Message};
    #[cfg(not(feature = "resolve"))]
    use rand_core::OsRng;
    use serde_json::Value;
    use utilities::{get_keypair_set, KeyPairSet};

    #[test]
    #[cfg(not(feature = "resolve"))]
    fn send_receive_raw() {
        // Arrange
        let m = Message::new()
            .from("did:xyz:ulapcuhsatnpuhza930hpu34n_")
            .to(&[
                "did::xyz:34r3cu403hnth03r49g03",
                "did:xyz:30489jnutnjqhiu0uh540u8hunoe",
            ])
            .body(sample_dids::TEST_DID_ENCRYPT_1)
            .expect("failed to set body");

        // Act
        let ready_to_send = m.clone().as_raw_json().unwrap();

        // checking if encryption fails on it
        let packed = m.clone().seal(b"anuhcphus", None);
        assert!(packed.is_err());

        // receiving raw message
        let received = Message::receive(&ready_to_send, None, None, None);

        // Assert
        assert_eq!(m, received.unwrap());
    }

    #[test]
    #[cfg(not(feature = "resolve"))]
    fn send_receive_encrypted_xc20p_json_test() {
        // Arrange
        // keys
        let KeyPairSet {
            alice_private,
            alice_public,
            bobs_private,
            bobs_public,
            mediators_public: carol_public,
            ..
        } = get_keypair_set();

        // Message construction
        let message = Message::new() // creating message
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp") // setting from
            .to(&[
                "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp",
                "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
            ]) // setting to
            .body(sample_dids::TEST_DID_SIGN_1)
            .expect("failed to set body") // packing in some payload
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec())) // set JOSE header for XC20P algorithm
            .add_header_field("my_custom_key".into(), "my_custom_value".into()) // custom header
            .add_header_field("another_key".into(), "another_value".into()) // another coustom header
            .kid(r#"#z6LShs9GGnqk85isEBzzshkuVWrVKsRp24GnDuHk8QWkARMW"#); // set kid header

        // Act
        let ready_to_send = message
            .seal(
                &alice_private,
                Some(vec![
                    Some(bobs_public.to_vec()),
                    Some(carol_public.to_vec()),
                ]),
            )
            .unwrap();
        let received = Message::receive(
            &ready_to_send,
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            None,
        ); // and now we parse received

        // Assert
        assert!(&received.is_ok());
        let sample_body: Value = serde_json::from_str(sample_dids::TEST_DID_SIGN_1).unwrap();
        let received_body: Value =
            serde_json::from_str(&received.unwrap().get_body().unwrap()).unwrap();
        assert_eq!(sample_body.to_string(), received_body.to_string());
    }

    #[test]
    #[cfg(not(feature = "resolve"))]
    fn send_receive_signed_json_test() {
        // Arrange + Act
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        // Message construction an JWS wrapping
        let message = Message::new() // creating message
            .from("did:xyz:ulapcuhsatnpuhza930hpu34n_") // setting from
            .to(&[
                "did::xyz:34r3cu403hnth03r49g03",
                "did:xyz:30489jnutnjqhiu0uh540u8hunoe",
            ]) // setting to
            .body(sample_dids::TEST_DID_SIGN_1)
            .expect("failed to set body") // packing in some payload
            .as_jws(&SignatureAlgorithm::EdDsa)
            .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes());

        assert!(&message.is_ok());

        // Receiving JWS
        let received = Message::verify(
            &message.unwrap().as_bytes(),
            &sign_keypair.verifying_key().to_bytes(),
        );
        // Assert
        assert!(&received.is_ok());
        // convert to serde values to compare contents and not formatting
        let sample_body: Value = serde_json::from_str(sample_dids::TEST_DID_SIGN_1).unwrap();
        let received_body: Value =
            serde_json::from_str(&received.unwrap().get_body().unwrap()).unwrap();
        assert_eq!(sample_body.to_string(), received_body.to_string());
    }

    #[test]
    #[cfg(not(feature = "resolve"))]
    fn send_receive_direct_signed_and_encrypted_xc20p_test() {
        // Arrange
        // keys

        let KeyPairSet {
            alice_public: _,
            alice_private,
            bobs_private: _,
            bobs_public,
            mediators_public: carol_public,
            ..
        } = get_keypair_set();
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);

        // Message construction
        let message = Message::new() // creating message
            .from("did:xyz:ulapcuhsatnpuhza930hpu34n_") // setting from
            .to(&[
                "did::xyz:34r3cu403hnth03r49g03",
                "did:xyz:30489jnutnjqhiu0uh540u8hunoe",
            ]) // setting to
            .body(sample_dids::TEST_DID_SIGN_1)
            .expect("failed to set body") // packing in some payload
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec())) // set JOSE header for XC20P algorithm
            .add_header_field("my_custom_key".into(), "my_custom_value".into()) // custom header
            .add_header_field("another_key".into(), "another_value".into()) // another custom header
            .kid(&hex::encode(sign_keypair.verifying_key().to_bytes())); // set kid header

        // Act
        // Send
        message
            .seal_signed(
                &alice_private,
                Some(vec![
                    Some(bobs_public.to_vec()),
                    Some(carol_public.to_vec()),
                ]),
                SignatureAlgorithm::EdDsa,
                &sign_keypair.to_bytes(),
            )
            .unwrap();
    }

    #[test]
    fn send_receive_mediated_encrypted_xc20p_json_test() {
        let KeyPairSet {
            alice_private,
            alice_public,
            bobs_private,
            bobs_public,
            mediators_private,
            mediators_public,
        } = get_keypair_set();
        let sealed = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .body(sample_dids::TEST_DID_SIGN_1)
            .expect("failed to add body") // packing in some payload
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()))
            .add_header_field("my_custom_key".into(), "my_custom_value".into()) // custom header
            .add_header_field("another_key".into(), "another_value".into()) // another coustom header
            .routed_by(
                &alice_private,
                Some(vec![Some(bobs_public.to_vec())]),
                "did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf",
                Some(mediators_public.to_vec()),
            );
        assert!(sealed.is_ok());

        let mediator_received = Message::receive(
            &sealed.unwrap(),
            Some(&mediators_private),
            Some(alice_public.to_vec()),
            None,
        );
        assert!(mediator_received.is_ok());

        let mediator_received_unwrapped = mediator_received.unwrap().get_body().unwrap();
        let pl_string = String::from_utf8_lossy(mediator_received_unwrapped.as_ref());
        let message_to_forward: Mediated = serde_json::from_str(&pl_string).unwrap();
        let attached_jwe = serde_json::from_slice::<Jwe>(&message_to_forward.payload);
        assert!(attached_jwe.is_ok());
        let str_jwe = serde_json::to_string(&attached_jwe.unwrap());
        assert!(str_jwe.is_ok());

        let bob_received = Message::receive(
            &String::from_utf8_lossy(&message_to_forward.payload),
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            None,
        );
        assert!(bob_received.is_ok());
        // convert to serde values to compare contents and not formatting
        let sample_body: Value = serde_json::from_str(sample_dids::TEST_DID_SIGN_1).unwrap();
        let bob_received_body: Value =
            serde_json::from_str(&bob_received.unwrap().get_body().unwrap()).unwrap();
        assert_eq!(sample_body.to_string(), bob_received_body.to_string());
    }
}
