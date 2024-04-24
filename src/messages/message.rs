#![allow(dead_code)]
use std::time::SystemTime;

#[cfg(feature = "raw-crypto")]
use crate::{
    crypto::{CryptoAlgorithm, Cypher, SignatureAlgorithm, Signer},
    helpers::{encrypt_cek, get_crypter_from_header, get_message_type, receive_jwe, receive_jws},
    Jwe, Mediated,
};
use crate::{Attachment, DidCommHeader, Error, JwmHeader, MessageType, PriorClaims, Recipient};
#[cfg(feature = "raw-crypto")]
use base64_url::decode;
#[cfg(all(feature = "resolve", feature = "raw-crypto"))]
use ddoresolver_rs::*;
#[cfg(feature = "raw-crypto")]
use rand::{RngCore, SeedableRng};
#[cfg(feature = "raw-crypto")]
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::Result;

/// DIDComm message structure.
///
/// `Message`s are used to construct new DIDComm messages.
///
/// A common flow is
/// - [creating a message][Message::new()]
/// - setting different properties with [chained setters](#impl-1)
/// - serializing the message to one of the following formats:
///   - a [plain][Message::as_raw_json()] DIDComm message
///   - a [signed][Message::sign()] JWS envelope
///   - an [encrypted][Message::seal()] JWE envelope
///   - a [sealed and encrypted][Message::seal_signed()] JWE envelope
///
/// For examples have a look [here][`crate`].
///
/// [Specification](https://identity.foundation/didcomm-messaging/spec/#message-structure)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// JOSE header, which is sent as public part with JWE.
    #[serde(flatten)]
    pub(crate) jwm_header: JwmHeader,

    /// DIDComm headers part, sent as part of encrypted message in JWE.
    #[serde(flatten)]
    pub(crate) didcomm_header: DidCommHeader,

    /// single recipient of JWE `recipients` collection as used in JWE
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) recipients: Option<Vec<Recipient>>,

    /// Message payload, which can be basically anything (JSON, text, file, etc.) represented
    ///     as base64url String of raw bytes of data.
    /// No direct access for encode/decode purposes! Use `get_body()` / `set_body()` methods instead.
    pub(crate) body: Value,

    /// Flag that toggles JWE serialization to flat JSON.
    /// Not part of the serialized JSON and ignored when deserializing.
    #[serde(skip)]
    pub(crate) serialize_flat_jwe: bool,

    /// Flag that toggles JWS serialization to flat JSON.
    /// Not part of the serialized JSON and ignored when deserializing.
    #[serde(skip)]
    pub(crate) serialize_flat_jws: bool,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) attachments: Vec<Attachment>,
}

impl Message {
    /// Generates EMPTY default message.
    /// Use extension messages to build final one before `send`ing.
    pub fn new() -> Self {
        match env_logger::try_init() {
            Ok(_) | Err(_) => (),
        }
        Message {
            jwm_header: JwmHeader::default(),
            didcomm_header: DidCommHeader::new(),
            recipients: None,
            body: json!({}),
            attachments: Vec::new(),
            serialize_flat_jwe: false,
            serialize_flat_jws: false,
        }
    }

    /// Adds (or updates) custom unique header key-value pair to the header.
    /// This portion of header is not sent as JOSE header.
    pub fn add_header_field(mut self, key: String, value: String) -> Self {
        if key.is_empty() {
            return self;
        }
        self.didcomm_header.other.insert(key, value);
        self
    }

    /// Sets message to be serialized as flat JWE JSON.
    /// If this message has multiple targets, `seal`ing it will result in an Error.
    #[cfg(feature = "raw-crypto")]
    pub fn as_flat_jwe(
        mut self,
        alg: &CryptoAlgorithm,
        recipient_public_key: Option<Vec<u8>>,
    ) -> Self {
        self.serialize_flat_jwe = true;
        self.as_jwe(alg, recipient_public_key)
    }

    /// Sets message to be serialized as flat JWS JSON and then calls `as_jws`.
    /// If this message has multiple targets, `seal`ing it will result in an Error.
    #[cfg(feature = "raw-crypto")]
    pub fn as_flat_jws(mut self, alg: &SignatureAlgorithm) -> Self {
        self.serialize_flat_jws = true;
        self.as_jws(alg)
    }

    /// Shortcut to `DidCommHeader::get_message_uri`
    ///
    pub fn get_message_uri(&self) -> String {
        self.didcomm_header.get_message_uri()
    }

    /// Sets `thid` and `pthid` same as those in `replying_to`
    /// Shortcut to `DidCommHeader::reply_to` method
    ///
    /// * `replying_to` - ref to message we're replying to
    pub fn reply_to(mut self, replying_to: &Self) -> Self {
        self.didcomm_header.reply_to(&replying_to.didcomm_header);
        self
    }

    /// Sets `pthid` to the `parent`'s `thid`.
    /// It defaults to `id` if `thid` is missing.
    ///
    /// # Parameters
    ///
    /// * `parent` - ref to a parent threaded `Message`
    ///
    pub fn with_parent(mut self, parent: &Self) -> Self {
        self.didcomm_header.pthid = Some(
            if let Some(thid_ref) = parent.didcomm_header.thid.as_ref() {
                thid_ref.clone()
            } else {
                parent.didcomm_header.id.clone()
            },
        );

        self
    }

    /// Setter of `from` header
    /// Helper method.
    ///
    /// For `resolve` feature will set `kid` header automatically
    ///     based on the did document resolved.
    #[cfg(feature = "raw-crypto")]
    pub fn as_jwe(mut self, alg: &CryptoAlgorithm, recipient_public_key: Option<Vec<u8>>) -> Self {
        self.jwm_header.as_encrypted(alg);
        if let Some(key) = recipient_public_key {
            self.jwm_header.kid = Some(base64_url::encode(&key));
        } else {
            #[cfg(feature = "resolve")]
            {
                if let Some(from) = &self.didcomm_header.from {
                    if let Some(document) = resolve_any(from) {
                        match alg {
                            CryptoAlgorithm::XC20P => {
                                self.jwm_header.kid =
                                    document.find_public_key_id_for_curve("X25519")
                            }
                            CryptoAlgorithm::A256GCM | CryptoAlgorithm::A256CBC => {
                                self.jwm_header.kid = document.find_public_key_id_for_curve("P-256")
                            }
                        }
                    }
                }
            }
        }
        self
    }

    /// Creates set of JWM related headers for the JWE
    /// Modifies JWM related header portion to match
    ///     encryption implementation and leaves other
    ///     parts unchanged.  TODO + FIXME: complete implementation
    #[cfg(feature = "raw-crypto")]
    pub fn as_jws(mut self, alg: &SignatureAlgorithm) -> Self {
        self.jwm_header.as_signed(alg);
        self
    }

    /// Setter of the `body`.
    /// Note, that given text has to be a valid JSON string to be a valid body value.
    pub fn body(mut self, body: &str) -> Result<Self> {
        self.body = serde_json::from_str(body)?;
        Ok(self)
    }

    /// Setter of `didcomm_header`.
    /// Replaces existing one with provided by consuming both values.
    /// Returns modified instance of `Self`.
    pub fn didcomm_header(mut self, h: DidCommHeader) -> Self {
        self.didcomm_header = h;
        self
    }

    /// Setter of `from` header.
    pub fn from(mut self, from: &str) -> Self {
        self.didcomm_header.from = Some(String::from(from));
        self
    }

    /// Getter of the `body` as String.
    pub fn get_body(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.body)?)
    }

    /// `&DidCommHeader` getter.
    pub fn get_didcomm_header(&self) -> &DidCommHeader {
        &self.didcomm_header
    }

    /// `&JwmCommHeader` getter.
    pub fn get_jwm_header(&self) -> &JwmHeader {
        &self.jwm_header
    }

    /// If message `is_rotation()` true - returns from_prion claims.
    /// Errors otherwise with `Error::NoRotationData`
    pub fn get_prior(&self) -> Result<PriorClaims> {
        if self.is_rotation() {
            Ok(self
                .didcomm_header
                .from_prior()
                .ok_or(Error::NoRotationData)?
                .clone())
        } else {
            Err(Error::NoRotationData)
        }
    }

    /// Checks if message is rotation one.
    /// Exposed for explicit checks on calling code level.
    pub fn is_rotation(&self) -> bool {
        self.didcomm_header.from_prior().is_some()
    }

    /// Setter of `jwm_header`.
    /// Replaces existing one with provided by consuming both values.
    /// Returns modified instance of `Self`.
    pub fn jwm_header(mut self, h: JwmHeader) -> Self {
        self.jwm_header = h;
        self
    }

    /// Setter of `m_type` @type header
    pub fn m_type(mut self, m_type: &str) -> Self {
        self.didcomm_header.m_type = m_type.into();
        self
    }

    /// Setter of `typ` header property.
    ///
    /// # Parameters
    ///
    /// * `typ` - `MessageType` to be set for `typ` property
    pub fn typ(mut self, typ: MessageType) -> Self {
        self.jwm_header.typ = typ;
        self
    }

    // Setter of the `kid` header
    pub fn kid(mut self, kid: &str) -> Self {
        match &mut self.jwm_header.kid {
            Some(h) => *h = kid.into(),
            None => {
                self.jwm_header.kid = Some(kid.into());
            }
        }
        self
    }

    /// Sets times of creation as now and, optional, expires time.
    ///
    /// # Arguments
    ///
    /// * `expires` - time in seconds since Unix Epoch when message is
    ///               considered to be invalid.
    pub fn timed(mut self, expires: Option<u64>) -> Self {
        self.didcomm_header.expires_time = expires;
        self.didcomm_header.created_time =
            match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
                Ok(t) => Some(t.as_secs()),
                Err(_) => None,
            };
        self
    }

    /// Setter of `to` header
    pub fn to(mut self, to: &[&str]) -> Self {
        for s in to {
            self.didcomm_header.to.push(s.to_string());
        }
        while let Some(a) = self
            .didcomm_header
            .to
            .iter()
            .position(|e| e == &String::default())
        {
            self.didcomm_header.to.remove(a);
        }
        self
    }

    /// Setter of `didcomm_header`.
    /// Replaces existing one with provided by consuming both values.
    /// Returns modified instance of `Self`.
    pub fn set_didcomm_header(mut self, h: DidCommHeader) -> Self {
        self.didcomm_header = h;
        self
    }

    /// Gets `Iterator` over key-value pairs of application level headers
    pub fn get_application_params(&self) -> impl Iterator<Item = (&String, &String)> {
        self.didcomm_header.other.iter()
    }

    /// Setter of `thid` header
    pub fn thid(mut self, thid: &str) -> Self {
        self.didcomm_header.thid = Some(thid.to_string());
        self
    }

    /// Setter of `pthid` header
    pub fn pthid(mut self, pthid: &str) -> Self {
        self.didcomm_header.pthid = Some(pthid.to_string());
        self
    }
}

// Interactions with messages (sending, receiving, etc.)
#[cfg(feature = "raw-crypto")]
impl Message {
    /// Serializes current state of the message into json.
    /// Consumes original message - use as raw sealing of envelope.
    pub fn as_raw_json(self) -> Result<String> {
        Ok(serde_json::to_string(&self)?)
    }

    /// Presents IV and Payload to be externally encrypted and then sealed with `seal_pre_encrypted` method.
    ///
    /// # Returns
    /// Tuple of bytes where .0 is IV and .1 is payload for encryption
    ///
    pub fn export_for_encryption(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        Ok((
            decode(&Jwe::generate_iv())?,
            serde_json::to_string(&self)?.as_bytes().to_vec(),
        ))
    }

    /// Builds JWE from current message and it's pre-encrypted payload:
    ///  `expert_for_encryption` should be used prior to this call and it's output
    ///  provided as payload.
    ///
    /// # Parameters
    /// `ciphertext` - encrypted output of `export_for_encryption` as JWE payload
    ///
    /// Returns serialized JSON JWE message, which is ready to be sent to receipent
    ///
    pub fn seal_pre_encrypted(self, cyphertext: impl AsRef<[u8]>) -> Result<String> {
        let d_header = self.get_didcomm_header();

        let mut unprotected = JwmHeader {
            skid: d_header.from.clone(),
            ..Default::default()
        };

        if self.recipients.is_none() {
            unprotected.kid = Some(d_header.to[0].clone());
        }

        let jwe = Jwe::new(
            Some(unprotected),
            self.recipients.clone(),
            cyphertext,
            Some(self.jwm_header.clone()),
            None::<&[u8]>,
            None,
        );

        Ok(serde_json::to_string(&jwe)?)
    }

    /// Construct a message from received data.
    /// Raw, JWS or JWE payload is accepted.
    ///
    /// # Arguments
    ///
    /// * `incoming` - serialized message as `Message`/`Jws`/`Jws`
    ///
    /// * `encryption_recipient_private_key` - recipients private key, used to decrypt `kek` in JWE
    ///
    /// * `encryption_sender_public_key` - senders public key, used to decrypt `kek` in JWE
    ///
    /// * `signing_sender_public_key` - senders public key, the JWS envelope was signed with
    pub fn receive(
        incoming: &str,
        encryption_recipient_private_key: Option<&[u8]>,
        encryption_sender_public_key: Option<Vec<u8>>,
        signing_sender_public_key: Option<&[u8]>,
    ) -> Result<Self> {
        let mut current_message: String = incoming.to_string();

        if get_message_type(&current_message)? == MessageType::DidCommJwe {
            let recipient_private_key = encryption_recipient_private_key.ok_or_else(|| {
                Error::Generic("missing encryption recipient private key".to_string())
            })?;
            current_message = receive_jwe(
                &current_message,
                recipient_private_key,
                encryption_sender_public_key,
            )?;
        }

        if get_message_type(&current_message)? == MessageType::DidCommJws {
            current_message = receive_jws(&current_message, signing_sender_public_key)?;
        }

        Ok(serde_json::from_str(&current_message)?)
    }

    /// Wrap self to be mediated by some mediator.
    /// Warning: Should be called on a `Message` instance which is ready to be sent!
    /// If message is not properly set up for crypto - this method will propagate error from
    ///     called `.seal()` method.
    /// Takes one mediator at a time to make sure that mediated chain preserves unchanged.
    /// This method can be chained any number of times to match all the mediators in the chain.
    ///
    /// # Arguments
    ///
    /// * `sender_private_key` - encryption key for inner message payload JWE encryption
    ///
    /// * `recipient_public_keys` - keys used to encrypt content encryption key for recipient;
    ///                             can be provided if key should not be resolved via recipients DID
    ///
    /// * `mediator_did` - DID of message mediator, will be `to` of mediated envelope
    ///
    /// * `mediator_public_key` - key used to encrypt content encryption key for mediator;
    ///                           can be provided if key should not be resolved via mediators DID
    pub fn routed_by(
        self,
        sender_private_key: &[u8],
        recipient_public_keys: Option<Vec<Option<Vec<u8>>>>,
        mediator_did: &str,
        mediator_public_key: Option<Vec<u8>>,
    ) -> Result<String> {
        let from = &self.didcomm_header.from.clone().unwrap_or_default();
        let alg = get_crypter_from_header(&self.jwm_header)?;
        let body = Mediated::new(self.didcomm_header.to[0].clone()).with_payload(
            self.seal(sender_private_key, recipient_public_keys)?
                .as_bytes()
                .to_vec(),
        );
        Message::new()
            .to(&[mediator_did])
            .from(from)
            .as_jwe(&alg, mediator_public_key.clone())
            .typ(MessageType::DidCommForward)
            .body(&serde_json::to_string(&body)?)?
            .seal(sender_private_key, Some(vec![mediator_public_key]))
    }

    /// Seals (encrypts) self and returns ready to send JWE
    ///
    /// # Arguments
    ///
    /// * `sender_private_key` - encryption key for inner message payload JWE encryption
    ///
    /// * `recipient_public_keys` - keys used to encrypt content encryption key for recipient;
    ///                             can be provided if key should not be resolved via recipients DID
    pub fn seal(
        mut self,
        sender_private_key: impl AsRef<[u8]>,
        recipient_public_keys: Option<Vec<Option<Vec<u8>>>>,
    ) -> Result<String> {
        if sender_private_key.as_ref().len() != 32 {
            return Err(Error::InvalidKeySize("!32".into()));
        }
        let to_len = self.didcomm_header.to.len();
        let public_keys = if let Some(recipient_public_keys_value) = recipient_public_keys {
            if recipient_public_keys_value.len() != to_len {
                return Err(Error::Generic(
                    "`to` and `recipient_public_keys` must have same length".to_string(),
                ));
            }
            recipient_public_keys_value
        } else {
            vec![None; to_len]
        };

        // generate content encryption key
        let mut cek = [0u8; 32];
        let mut rng = ChaCha20Rng::from_seed(Default::default());
        rng.fill_bytes(&mut cek);
        trace!("sealing message with shared_key: {:?}", &cek.as_ref());

        if to_len == 0_usize {
            return Err(Error::NoJweRecipient);
        } else if self.serialize_flat_jwe && self.didcomm_header.to.len() > 1 {
            return Err(Error::Generic(
                "flat JWE serialization only supports a single `to`".to_string(),
            ));
        }

        let mut recipients: Vec<Recipient> = vec![];
        // create jwk from static secret per recipient
        for (i, public_key) in public_keys.iter().enumerate().take(to_len) {
            let rv = encrypt_cek(
                &self,
                sender_private_key.as_ref(),
                &self.didcomm_header.to[i],
                &cek,
                public_key.to_owned(),
            )?;
            recipients.push(Recipient::new(rv.header, rv.encrypted_key));
        }
        self.recipients = Some(recipients);
        // encrypt original message with static secret
        let alg = get_crypter_from_header(&self.jwm_header)?;
        self.encrypt(alg.encryptor(), cek.as_ref())
    }
}

/// Associated functions implementations.
/// Possibly not required as Jwe serialization covers this.
impl Message {
    /// Parses `iv` value as `Vec<u8>` from public header.
    /// Both regular JSON and Compact representations are accepted.
    /// Returns `Error` on failure.
    /// TODO: Add examples
    pub fn get_iv(received: &[u8]) -> Result<Vec<u8>> {
        // parse from compact
        let as_str = String::from_utf8(received.to_vec())?;
        let json: serde_json::Value = if let Some(header_end) = as_str.find('.') {
            serde_json::from_str(&String::from_utf8(base64_url::decode(
                &as_str[..header_end],
            )?)?)?
        } else {
            serde_json::from_str(&as_str)?
        };
        if let Some(iv) = json.get("iv") {
            if let Some(t) = iv.as_str() {
                if t.len() != 24 {
                    Err(Error::Generic(format!(
                        "IV [nonce] size is incorrect: {}",
                        t.len()
                    )))
                } else {
                    Ok(t.as_bytes().to_vec())
                }
            } else {
                Err(Error::Generic("wrong nonce format".into()))
            }
        } else {
            Err(Error::Generic("iv is not found in JOSE header".into()))
        }
    }

    /// Transforms incomming into `Jwe` if it is one
    /// Also checks if `skid` field is present or returns `None` othervise
    /// Key resolution and validation fall onto caller of this method
    ///
    /// # Parameters
    ///
    /// * `incomming` - incomming message
    ///
    /// Returns `Option<Jwe>` where `.header.skid` is skid and `.payload()` is cyphertext
    ///
    #[cfg(feature = "raw-crypto")]
    pub fn received_as_jwe(incomming: impl AsRef<[u8]>) -> Option<Jwe> {
        if let Ok(jwe) = serde_json::from_slice::<Jwe>(incomming.as_ref()) {
            if jwe.get_skid().is_some() {
                Some(jwe)
            } else {
                None
            }
        } else {
            None
        }
    }
    /// Transforms decrypted `Jwe` into `Message`
    ///
    /// # Parameters
    ///
    /// * `decrypted` - result of decrypting of Jwe payload retreived after
    ///  decrypting content of `as_jwe` function call output.
    ///
    pub fn receive_external_crypto(decrypted: impl AsRef<[u8]>) -> Result<Self> {
        Ok(serde_json::from_slice(decrypted.as_ref())?)
    }

    /// Signs raw message and then packs it to encrypted envelope
    /// [Spec](https://identity.foundation/didcomm-messaging/spec/#message-signing)
    ///
    /// # Arguments
    ///
    /// * `encryption_sender_private_key` - encryption key for inner message payload JWE encryption
    ///
    /// * `encryption_recipient_public_keys` - keys used to encrypt content encryption key for
    ///                                        recipient with; can be provided if key should not be
    ///                                        resolved via recipients DID
    ///
    /// * `signing_algorithm` - encryption algorithm used
    ///
    /// * `signing_sender_private_key` - signing key for enveloped message JWS encryption
    #[cfg(feature = "raw-crypto")]
    pub fn seal_signed(
        self,
        encryption_sender_private_key: &[u8],
        encryption_recipient_public_keys: Option<Vec<Option<Vec<u8>>>>,
        signing_algorithm: SignatureAlgorithm,
        signing_sender_private_key: &[u8],
    ) -> Result<String> {
        let mut to = self.clone();
        let signed = self
            .as_jws(&signing_algorithm)
            .sign(signing_algorithm.signer(), signing_sender_private_key)?;
        to.body = serde_json::from_str(&signed)?;
        to.typ(MessageType::DidCommJws).seal(
            encryption_sender_private_key,
            encryption_recipient_public_keys,
        )
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn iv_from_json_test() {
        // Arrange
        // Example JWM from RFC: https://tools.ietf.org/html/draft-looker-jwm-01#section-2.3
        // Extendet twice to be 192bit (24byte) nonce.
        let raw_json = r#" { "protected": "eyJ0eXAiOiJKV00iLCJlbmMiOiJBMjU2R0NNIiwia2lkIjoiUEdvWHpzME5XYVJfbWVLZ1RaTGJFdURvU1ZUYUZ1eXJiV0k3VjlkcGpDZyIsImFsZyI6IkVDREgtRVMrQTI1NktXIiwiZXBrIjp7Imt0eSI6IkVDIiwiY3J2IjoiUC0yNTYiLCJ4IjoiLU5oN1NoUkJfeGFDQlpSZElpVkN1bDNTb1IwWXc0VEdFUXFxR2lqMXZKcyIsInkiOiI5dEx4ODFQTWZRa3JPdzh5dUkyWXdJMG83TXROemFDR2ZDQmJaQlc1WXJNIn19",
                "recipients": [
                  {
                    "encrypted_key": "J1Fs9JaDjOT_5481ORQWfEZmHy7OjE3pTNKccnK7hlqjxbPalQWWLg"
                  }
                ],
                "iv": "u5kIzo0m_d2PjI4mu5kIzo0m",
                "ciphertext": "qGuFFoHy7HBmkf2BaY6eREwzEjn6O_FnRoXj2H-DAXo1PgQdfON-_1QbxtnT8e8z_M6Gown7s8fLtYNmIHAuixqFQnSA4fdMcMSi02z1MYEn2JC-1EkVbWr4TqQgFP1EyymB6XjCWDiwTYd2xpKoUshu8WW601HLSgFIRUG3-cK_ZSdFaoWosIgAH5EQ2ayJkRB_7dXuo9Bi1MK6TYGZKezc6rpCK_VRSnLXhFwa1C3T0QBes",
                "tag": "doeAoagwJe9BwKayfcduiw"
            }"#;
        // Act
        let iv = Message::get_iv(raw_json.as_bytes());
        // Assert
        assert!(iv.is_ok());
        assert_eq!(
            "u5kIzo0m_d2PjI4mu5kIzo0m",
            &String::from_utf8(iv.unwrap()).unwrap()
        );
    }

    #[test]
    fn iv_from_compact_json_test() {
        // Arrange
        // Example JWM from RFC: https://tools.ietf.org/html/draft-looker-jwm-01#section-2.3
        let compact = r#"eyJ0eXAiOiJKV00iLCJlbmMiOiJBMjU2R0NNIiwia2lkIjoiUEdvWHpzME5XYVJfbWVLZ1RaTGJFdURvU1ZUYUZ1eXJiV0k3VjlkcGpDZyIsImFsZyI6IkVDREgtRVMrQTI1NktXIiwiaXYiOiAidTVrSXpvMG1fZDJQakk0bXU1a0l6bzBtIn0."#;
        // Act
        let iv = Message::get_iv(compact.as_bytes());
        // Assert
        assert!(iv.is_ok());
        assert_eq!(
            "u5kIzo0m_d2PjI4mu5kIzo0m",
            &String::from_utf8(iv.unwrap()).unwrap()
        );
    }
}

#[cfg(all(test, feature = "raw-crypto"))]
mod crypto_tests {
    extern crate chacha20poly1305;
    extern crate sodiumoxide;

    #[cfg(feature = "resolve")]
    use base58::FromBase58;
    use rand_core::OsRng;
    use utilities::{get_keypair_set, KeyPairSet};

    use super::*;
    #[cfg(feature = "resolve")]
    use crate::{Jwe, Mediated};

    #[test]
    #[cfg(not(feature = "resolve"))]
    fn create_and_send() {
        let KeyPairSet {
            alice_private,
            bobs_public,
            ..
        } = get_keypair_set();
        let m = Message::new().as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()));
        let p = m.seal(&alice_private, Some(vec![Some(bobs_public.to_vec())]));
        assert!(p.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn create_and_send() {
        let KeyPairSet { alice_private, .. } = get_keypair_set();
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, None);
        let p = m.seal(&alice_private, None);
        assert!(p.is_ok());
    }

    #[test]
    fn create_and_send_without_resolving_dids() {
        let KeyPairSet {
            alice_private,
            bobs_public,
            ..
        } = get_keypair_set();
        let m = Message::new().as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()));
        let p = m.seal(&alice_private, Some(vec![Some(bobs_public.to_vec())]));
        assert!(p.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn receive_test() {
        // Arrange
        let KeyPairSet {
            alice_public,
            alice_private,
            bobs_private,
            ..
        } = get_keypair_set();
        // alice seals JWE
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, None);
        let jwe = m.seal(&alice_private, None).unwrap();

        // Act
        // bob receives JWE
        let received =
            Message::receive(&jwe, Some(&bobs_private), Some(alice_public.to_vec()), None);

        // Assert
        assert!(received.is_ok());
    }

    #[test]
    fn receive_test_without_resolving_dids() {
        // Arrange
        let KeyPairSet {
            alice_public,
            alice_private,
            bobs_private,
            bobs_public,
            ..
        } = get_keypair_set();
        // alice seals JWE
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()));
        let jwe = m
            .seal(&alice_private, Some(vec![Some(bobs_public.to_vec())]))
            .unwrap();

        // Act
        // bob receives JWE
        let received =
            Message::receive(&jwe, Some(&bobs_private), Some(alice_public.to_vec()), None);

        // Assert
        assert!(received.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn send_receive_didkey_test() {
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, None);
        // TODO: validate derived pub from priv key <<<
        let KeyPairSet {
            alice_private,
            bobs_private,
            ..
        } = get_keypair_set();
        let jwe = m.seal(&alice_private, None);
        assert!(jwe.is_ok());

        let received = Message::receive(&jwe.unwrap(), Some(&bobs_private), None, None);
        assert!(received.is_ok());
    }

    #[test]
    fn send_receive_didkey_explicit_pubkey_test() {
        let KeyPairSet {
            alice_public,
            alice_private,
            bobs_private,
            bobs_public,
            ..
        } = get_keypair_set();
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()));

        let jwe = m.seal(&alice_private, Some(vec![Some(bobs_public.to_vec())]));
        assert!(jwe.is_ok());

        let received = Message::receive(
            &jwe.unwrap(),
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            None,
        );
        assert!(received.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn send_receive_didkey_test_1pu_aes256() {
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::A256GCM, None);
        // TODO: validate derived pub from priv key <<<
        let KeyPairSet {
            alice_private,
            bobs_private,
            ..
        } = get_keypair_set();
        let jwe = m.seal(&alice_private, None);
        assert!(jwe.is_ok());

        let received = Message::receive(&jwe.unwrap(), Some(&bobs_private), None, None);
        assert!(received.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn send_receive_didkey_test_1pu_aes256_explicit_pubkey() {
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::A256GCM, None);
        // TODO: validate derived pub from priv key <<<
        let KeyPairSet {
            alice_private,
            bobs_private,
            ..
        } = get_keypair_set();
        let jwe = m.seal(&alice_private, None);
        assert!(jwe.is_ok());

        let KeyPairSet { alice_public, .. } = get_keypair_set();

        let received = Message::receive(
            &jwe.unwrap(),
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            None,
        );
        assert!(received.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn send_receive_didkey_multiple_recipients_test() {
        let m = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&[
                "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
                "did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf",
            ])
            .as_jwe(&CryptoAlgorithm::XC20P, None);
        let KeyPairSet {
            alice_private,
            bobs_private,
            ..
        } = get_keypair_set();
        let third_private = "ACa4PPJ1LnPNq1iwS33V3Akh7WtnC71WkKFZ9ccM6sX2"
            .from_base58()
            .unwrap();
        let jwe = m.seal(&alice_private, None);
        assert!(jwe.is_ok());

        let jwe = jwe.unwrap();
        let received_bob = Message::receive(&jwe, Some(&bobs_private), None, None);
        let received_third = Message::receive(&jwe, Some(&third_private), None, None);
        assert!(received_bob.is_ok());
        assert!(received_third.is_ok());
    }

    #[test]
    #[cfg(feature = "resolve")]
    fn mediated_didkey_test() {
        let mediator_private = "ACa4PPJ1LnPNq1iwS33V3Akh7WtnC71WkKFZ9ccM6sX2"
            .from_base58()
            .unwrap();
        let KeyPairSet {
            alice_private,
            bobs_private,
            ..
        } = get_keypair_set();
        let sealed = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .as_jwe(&CryptoAlgorithm::XC20P, None)
            .routed_by(
                &alice_private,
                None,
                "did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf",
                None,
            );
        assert!(sealed.is_ok());

        let mediator_received =
            Message::receive(&sealed.unwrap(), Some(&mediator_private), None, None);
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
            None,
            None,
        );
        assert!(bob_received.is_ok());
    }

    #[test]
    fn can_pass_explicit_signing_verification_keys() -> Result<()> {
        let KeyPairSet {
            alice_private,
            alice_public,
            bobs_private,
            bobs_public,
            ..
        } = get_keypair_set();
        let sign_keypair = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let body = r#"{"foo":"bar"}"#;
        let message = Message::new()
            .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
            .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
            .body(body)? // packing in some payload
            .as_flat_jwe(&CryptoAlgorithm::XC20P, Some(bobs_public.to_vec()))
            .kid(&hex::encode(vec![1; 32])); // invalid key, passing no key will not succeed

        let jwe_string = message.seal_signed(
            &alice_private,
            Some(vec![Some(bobs_public.to_vec())]),
            SignatureAlgorithm::EdDsa,
            &sign_keypair.to_bytes(),
        )?;

        let received_failure_no_key = Message::receive(
            &jwe_string,
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            None,
        );
        let received_failure_wrong_key = Message::receive(
            &jwe_string,
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            Some(&[0; 32]),
        );
        let received_success = Message::receive(
            &jwe_string,
            Some(&bobs_private),
            Some(alice_public.to_vec()),
            Some(&sign_keypair.verifying_key().to_bytes()),
        );

        // Assert
        assert!(&received_failure_no_key.is_err());
        assert!(&received_failure_wrong_key.is_err());
        assert!(&received_success.is_ok());
        let received = received_success.unwrap();
        let sample_body: Value = serde_json::from_str(body).unwrap();
        let received_body: Value = serde_json::from_str(&received.get_body().unwrap()).unwrap();
        assert_eq!(sample_body.to_string(), received_body.to_string(),);

        Ok(())
    }
}
