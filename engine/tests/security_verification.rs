#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer, VerifyingKey};
    use rand::rngs::OsRng;
    use capsuled_engine::security::verifier::ManifestVerifier;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    #[test]
    fn test_verifier_enforcement_logic() {
        // 1. Generate a "Trusted" keypair
        let mut csprng = OsRng;
        let trusted_signing_key = SigningKey::generate(&mut csprng);
        let trusted_verifying_key = trusted_signing_key.verifying_key();
        let trusted_pubkey_fingerprint = format!("ed25519:{}", BASE64.encode(trusted_verifying_key.as_bytes()));

        // 2. Generate a "Malicious" keypair
        let malicious_signing_key = SigningKey::generate(&mut csprng);
        let malicious_verifying_key = malicious_signing_key.verifying_key();
        
        // 3. Create content
        let content = b"{\"name\": \"safe-capsule\"}";
        
        // 4. Sign content with TRUSTED key
        let signature_trusted = trusted_signing_key.sign(content);
        
        // 5. Sign content with MALICIOUS key
        let signature_malicious = malicious_signing_key.sign(content);

        // 6. Create signature file structure (mocking libadep format)
        let create_sig_bytes = |pk: &VerifyingKey, sig: &ed25519_dalek::Signature| -> Vec<u8> {
             let mut buffer = Vec::new();
             buffer.push(0x01); // Version
             buffer.push(0x01); // KeyType
             buffer.extend_from_slice(pk.as_bytes());
             buffer.extend_from_slice(&sig.to_bytes());
             let metadata = b"{}";
             buffer.extend_from_slice(&(metadata.len() as u16).to_be_bytes());
             buffer.extend_from_slice(metadata);
             buffer
        };

        let sig_trusted_bytes = create_sig_bytes(&trusted_verifying_key, &signature_trusted);
        let sig_malicious_bytes = create_sig_bytes(&malicious_verifying_key, &signature_malicious);
        
        let verifier = ManifestVerifier::new(Some(trusted_pubkey_fingerprint.clone()), true);

        // Case A: Verify Trusted Signature -> OK
        assert!(verifier.verify(content, &sig_trusted_bytes, "").is_ok());

        // Case B: Verify Malicious Signature -> FAIL (Key Mismatch)
        let res = verifier.verify(content, &sig_malicious_bytes, "");
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("is not the trusted signer"));

        // Case C: Tampered Content -> FAIL (Crypto Fail)
        let tampered_content = b"{\"name\": \"hacked\"}";
        let res = verifier.verify(tampered_content, &sig_trusted_bytes, "");
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Cryptographic verification failed"));
    }
}
