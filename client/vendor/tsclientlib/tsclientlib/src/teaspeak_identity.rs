//! TeaSpeak TEAMSPEAK identity handshake helpers (`authentication_method=1`).
//!
//! TeamSpeak/TeaSpeak identities are P-256 keys in libtomcrypt DER form.
//! `tsproto_types::crypto` already speaks that format; this module wraps the
//! pieces needed for `handshakebegin` / `handshakeindentityproof`.

use base64::prelude::*;
use tsproto::Identity;
use tsproto_types::crypto::{EccKeyPrivP256, Error as CryptoError};

type Result<T> = std::result::Result<T, CryptoError>;

/// Public key as libtomcrypt `ecc_export(PK_PUBLIC)`, base64 — for `handshakebegin`.
pub fn public_key_tomcrypt_b64(identity: &Identity) -> String {
	identity.key().to_pub().to_ts()
}

/// ECDSA-SHA256 DER signature of `message`, base64 — for `handshakeindentityproof`.
pub fn sign_challenge(identity: &Identity, message: &str) -> String {
	let sig = identity.key().clone().sign(message.as_bytes());
	BASE64_STANDARD.encode(sig)
}

/// Parse a TomCrypt private key (base64 DER or nested base64) into an [`Identity`].
///
/// Useful when importing raw TeaSpeak/oldclient key material without a hash-cash
/// counter prefix. The counter defaults to 0; call [`Identity::upgrade_level`]
/// afterwards if needed.
pub fn identity_from_tomcrypt_b64(input: &str) -> Result<Identity> {
	let der = decode_identity_der(input)?;
	let key = EccKeyPrivP256::from_tomcrypt(&der)?;
	Ok(Identity::new(key, 0))
}

fn decode_identity_der(input: &str) -> Result<Vec<u8>> {
	let normalized: String = input.chars().filter(|c| !c.is_whitespace()).collect();
	let direct = BASE64_STANDARD.decode(&normalized)?;
	if !direct.is_empty() && direct[0] == 0x30 {
		return Ok(direct);
	}
	// Some exports wrap the DER base64 in another base64 layer.
	if let Ok(nested) = std::str::from_utf8(&direct) {
		let nested: String = nested.chars().filter(|c| !c.is_whitespace()).collect();
		if !nested.is_empty() {
			let nested_der = BASE64_STANDARD.decode(&nested)?;
			if !nested_der.is_empty() && nested_der[0] == 0x30 {
				return Ok(nested_der);
			}
		}
	}
	Err(CryptoError::KeyDecodeError)
}

#[cfg(test)]
mod tests {
	use super::*;

	// Same test key as tsproto Identity tests (TomCrypt private DER, base64).
	const TEST_PRIV_KEY: &str =
		"MG8DAgeAAgEgAiEA6rtKxDn/o/Bo50rNtAE5Ph3h2RKLHQ0gbFkvm2yA79kCIQCrfzAZts/\
		 vHP+3MOetKLjNnpZXt4c6U3UB4gWLKR4H9AIgYTyJofmztcTBjq3KZcDdxu+G4RPVwE5vg8VaN2jbQao=";

	#[test]
	fn public_key_and_sign_roundtrip() {
		let identity = Identity::new_from_str(TEST_PRIV_KEY).unwrap();
		let pub_b64 = public_key_tomcrypt_b64(&identity);
		assert!(!pub_b64.is_empty());
		let der = BASE64_STANDARD.decode(&pub_b64).unwrap();
		assert_eq!(der[0], 0x30);

		let proof = sign_challenge(&identity, "challenge-bytes");
		let sig = BASE64_STANDARD.decode(&proof).unwrap();
		identity.key().to_pub().verify(b"challenge-bytes", &sig).unwrap();
	}

	#[test]
	fn parse_tomcrypt_identity() {
		let identity = identity_from_tomcrypt_b64(TEST_PRIV_KEY).unwrap();
		assert_eq!(identity.key().to_pub().get_uid(), "test/9PZ9vww/Bpf5vJxtJhpz80=");
	}
}
