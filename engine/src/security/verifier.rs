use anyhow::{anyhow, Result};
use std::sync::Arc;
use tracing::{info, warn};
use libadep_core::signing::{ensure_signature_matches_manifest, verify_signature_file, SignatureFile};
use std::path::PathBuf;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

/// Verifies capsule manifests against a trusted public key.
#[derive(Clone, Debug)]
pub struct ManifestVerifier {
    public_key_fingerprint: Option<String>,
    _enforce: bool,
}

impl ManifestVerifier {
    pub fn new(public_key_fingerprint: Option<String>, enforce: bool) -> Self {
        if public_key_fingerprint.is_none() {
            warn!("Security: No public key configured. Signature verification will be skipped.");
        }
        Self {
            public_key_fingerprint,
            _enforce: enforce,
        }
    }

    /// Verifies the content against the provided signature.
    /// 
    /// # Arguments
    /// * `content` - The raw bytes of the manifest (JSON/TOML)
    /// * `signature_bytes` - The raw bytes of the signature file (Ed25519)
    /// * `developer_key` - The developer key fingerprint from the manifest (e.g., "ed25519:...")
    pub fn verify(&self, content: &[u8], signature_bytes: &[u8], developer_key: &str) -> Result<()> {
        // 1. Check if we have a trusted root key configured
        let trusted_key = match &self.public_key_fingerprint {
            Some(k) => k,
            None => {
                // If no key is configured, we cannot verify. 
                return Ok(()); 
            }
        };

        // 2. Parse the signature file
        let sig_file = self.parse_signature_bytes(signature_bytes)
            .map_err(|e| anyhow!("Invalid signature format: {}", e))?;

        // 3. Ensure the signature's public key matches the trusted key
        // Note: libadep uses `ensure_signature_matches_manifest` to check if signature.public_key == manifest.developer_key.
        // But if manifest.developer_key is empty (V1), we can't use that check effectively for TRUST.
        // We fundamentally want to know: "Is this signature signed by our Trusted Key?"
        
        // We construct a temporary key fingerprint from the signature itself
        let sig_key_fingerprint = format!("ed25519:{}", BASE64.encode(sig_file.public_key));
        if sig_key_fingerprint != *trusted_key {
              return Err(anyhow!("Signature key {} is not the trusted signer {}", sig_key_fingerprint, trusted_key));
        }

        // 4. If developer_key is provided (from manifest), check it matches too (optional consistency check like libadep)
        if !developer_key.is_empty() {
             ensure_signature_matches_manifest(&sig_file, developer_key)
                .map_err(|e| anyhow!("Signature key mismatch with manifest developer_key: {}", e))?;
        }

        // 5. Verify the cryptographic signature
        verify_signature_file(&sig_file, content)
            .map_err(|e| anyhow!("Cryptographic verification failed: {}", e))?;

        info!("Security: Manifest verified successfully for trusted signer {}", trusted_key);
        Ok(())
    }

    fn parse_signature_bytes(&self, data: &[u8]) -> Result<SignatureFile> {
        // Logic adapted from libadep element `read_signature_file`
        if data.len() < 1 + 1 + 32 + 64 + 2 {
            return Err(anyhow!("signature file too short"));
        }
        let version = data[0];
        let key_type = data[1];
        let mut offset = 2;
        
        let mut public_key = [0u8; 32];
        if data.len() < offset + 32 { return Err(anyhow!("invalid length")); }
        public_key.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        let mut sig_bytes = [0u8; 64];
        if data.len() < offset + 64 { return Err(anyhow!("invalid length")); }
        sig_bytes.copy_from_slice(&data[offset..offset + 64]);
        offset += 64;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        let metadata_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if data.len() < offset + metadata_len {
            return Err(anyhow!("signature metadata length out of bounds"));
        }
        let metadata_bytes = &data[offset..offset + metadata_len];
        let metadata: serde_json::Value = serde_json::from_slice(metadata_bytes)
            .map_err(|e| anyhow!("failed to parse signature metadata JSON: {}", e))?;

        Ok(SignatureFile {
            version,
            key_type,
            public_key,
            signature,
            metadata,
        })
    }
}
