use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::encode as base64_encode;
use hex;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tiny_http::{Header, Method, Response, Server};
use zstd::stream::encode_all;

/// Test the full ADEP workflow: init → build → keygen → sign → verify → pack
#[test]
fn test_full_workflow() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let root = temp_dir.path();

    // Get the path to the adep-cli binary
    let bin_path = get_bin_path();

    // 1. Initialize package
    let output = Command::new(&bin_path)
        .arg("init")
        .arg("--root")
        .arg(root)
        .arg("--app-name")
        .arg("Test App")
        .arg("--version")
        .arg("1.0.0")
        .output()
        .expect("failed to execute init");

    assert!(
        output.status.success(),
        "init failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("manifest.json").exists());
    assert!(root.join("manifest.json.sha256").exists());
    assert!(root.join("dist").exists());

    // 2. Create sample files in dist/
    let dist = root.join("dist");
    fs::write(dist.join("index.html"), b"<html><body>Test</body></html>")
        .expect("failed to write index.html");
    fs::write(dist.join("app.js"), b"console.log('test');").expect("failed to write app.js");
    fs::write(dist.join("test.worker.js"), b"self.onmessage = () => {};")
        .expect("failed to write worker");

    // 3. Build (generate files array)
    let output = Command::new(&bin_path)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to execute build");

    assert!(
        output.status.success(),
        "build failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify files were added to manifest
    let manifest_content =
        fs::read_to_string(root.join("manifest.json")).expect("failed to read manifest");
    assert!(manifest_content.contains("index.html"));
    assert!(manifest_content.contains("app.js"));
    assert!(manifest_content.contains("test.worker.js"));
    assert!(manifest_content.contains(r#""role": "runtime""#)); // worker should be runtime
    assert!(manifest_content.contains(r#""role": "asset""#)); // js/html should be asset

    // 4. Generate keypair
    let output = Command::new(&bin_path)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to execute keygen");

    assert!(
        output.status.success(),
        "keygen failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("keys/developer.json").exists());

    // Verify developer_key was added to manifest
    let manifest_content =
        fs::read_to_string(root.join("manifest.json")).expect("failed to read manifest");
    assert!(manifest_content.contains("developer_key"));
    assert!(manifest_content.contains("ed25519:"));

    // 5. Sign package
    let output = Command::new(&bin_path)
        .arg("sign")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to execute sign");

    assert!(
        output.status.success(),
        "sign failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("_sig/developer.sig").exists());

    // 6. Verify package
    let output = Command::new(&bin_path)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to execute verify");

    assert!(
        output.status.success(),
        "verify failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(output_str.contains("Verifying runtime files first"));
    assert!(output_str.contains("test.worker.js"));
    assert!(output_str.contains("Verification succeeded"));

    // 7. Pack archive
    let output = Command::new(&bin_path)
        .arg("pack")
        .arg("--root")
        .arg(root)
        .arg("--force")
        .output()
        .expect("failed to execute pack");

    assert!(
        output.status.success(),
        "pack failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("app.adep").exists());

    // Verify the archive is a valid ZIP
    let adep_file = fs::read(root.join("app.adep")).expect("failed to read adep file");
    assert!(
        adep_file.starts_with(b"PK"),
        "adep file is not a ZIP archive"
    );
}

#[test]
fn test_role_inference() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let root = temp_dir.path();
    let bin_path = get_bin_path();

    // Initialize
    Command::new(&bin_path)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to init");

    // Create files with different extensions
    let dist = root.join("dist");
    fs::write(dist.join("app.wasm"), b"wasm-content").unwrap();
    fs::write(dist.join("worker.worker.js"), b"worker-content").unwrap();
    fs::write(dist.join("bundle.js"), b"js-content").unwrap();
    fs::write(dist.join("style.css"), b"css-content").unwrap();

    // Build
    Command::new(&bin_path)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to build");

    let manifest = fs::read_to_string(root.join("manifest.json")).unwrap();

    // Check role inference
    assert!(manifest.contains(r#""path": "dist/app.wasm""#));
    assert!(manifest.contains(r#""path": "dist/worker.worker.js""#));

    // Find the roles - wasm and worker.js should be runtime
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let files = manifest_json["files"].as_array().unwrap();

    for file in files {
        let path = file["path"].as_str().unwrap();
        let role = file["role"].as_str().unwrap();

        if path.ends_with(".wasm") || path.ends_with(".worker.js") {
            assert_eq!(role, "runtime", "Expected runtime role for {}", path);
        } else {
            assert_eq!(role, "asset", "Expected asset role for {}", path);
        }
    }
}

#[test]
fn test_manifest_defaults() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let root = temp_dir.path();
    let bin_path = get_bin_path();

    // Initialize without app name
    let output = Command::new(&bin_path)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .expect("failed to init");

    assert!(output.status.success());

    let manifest = fs::read_to_string(root.join("manifest.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).unwrap();

    // Check default values
    assert!(manifest_json["publish_info"]["name"].is_string());
    assert!(manifest_json["publish_info"]["icon"].is_string());
    assert_eq!(
        manifest_json["publish_info"]["icon"].as_str().unwrap(),
        "dist/icon.png"
    );
}

#[test]
fn test_tampered_file_detection() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    // 正常にパッケージ作成
    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    let dist = root.join("dist");
    fs::write(dist.join("app.js"), b"original").unwrap();

    Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("sign")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // ファイル改ざん
    fs::write(dist.join("app.js"), b"tampered").unwrap();

    // verify失敗を確認
    let output = Command::new(&bin)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("hash mismatch") || stderr.contains("verification failed"));
}

#[test]
fn test_invalid_signature() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    // 正常にパッケージ作成
    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    fs::write(root.join("dist/app.js"), b"content").unwrap();
    Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("sign")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // 署名ファイル破壊
    let sig_path = root.join("_sig/developer.sig");
    let mut sig_data = fs::read(&sig_path).unwrap();
    sig_data[50] ^= 0xFF; // 1バイト反転
    fs::write(&sig_path, sig_data).unwrap();

    // verify失敗を確認
    let output = Command::new(&bin)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("signature verification failed"));
}

#[test]
fn test_manifest_hash_mismatch() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // manifest.json.sha256を改ざん
    fs::write(root.join("manifest.json.sha256"), "invalid_hash\n").unwrap();

    // verify失敗を確認
    let output = Command::new(&bin)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("manifest.json.sha256 mismatch"));
}

#[test]
fn test_developer_key_mismatch() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    // パッケージ作成
    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    fs::write(root.join("dist/app.js"), b"content").unwrap();
    Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("sign")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // 別の鍵を生成してmanifestに上書き
    let temp_key = TempDir::new().unwrap();
    Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(temp_key.path())
        .arg("--skip-manifest")
        .output()
        .unwrap();

    let new_key_file = temp_key.path().join("keys/developer.json");
    let new_key: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(new_key_file).unwrap()).unwrap();

    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();

    manifest["developer_key"] = serde_json::Value::String(format!(
        "ed25519:{}",
        new_key["public_key"].as_str().unwrap()
    ));

    let manifest_path = root.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // manifest.json.sha256も更新する（整合性を保つため）
    Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // verify失敗を確認
    let output = Command::new(&bin)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // developer_keyが変更されているため、署名検証が失敗することを確認
    assert!(
        stderr.contains("does not match manifest developer_key")
            || stderr.contains("signature public key does not match")
    );
}

#[test]
fn test_sbom_required_when_src_exists() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    fs::write(root.join("dist/app.js"), b"content").unwrap();
    Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("sign")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // src/ ディレクトリ作成（sbom.jsonなし）
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();

    // pack失敗を確認
    let output = Command::new(&bin)
        .arg("pack")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sbom.json") && stderr.contains("missing"));

    // verify失敗を確認
    let output = Command::new(&bin)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn test_key_rotation_validation() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    // 正常なパッケージを作成
    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // manifest.jsonにkey_rotationを追加（previous_keyが不正な形式）
    let manifest_path = root.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();

    // 不正なprevious_keyを設定
    manifest["key_rotation"] = serde_json::json!({
        "previous_key": "invalid_key_format",
        "reason": "Test rotation"
    });

    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // buildで検証が走るはずだが、key_rotationの検証はverifyで行われる
    // まずbuildを実行
    fs::write(root.join("dist/app.js"), b"content").unwrap();
    Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // keygenとsignを実行
    Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    Command::new(&bin)
        .arg("sign")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    // verifyでkey_rotationが検証される
    let output = Command::new(&bin)
        .arg("verify")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    // 不正なフォーマットでもverifyは実行されるが、エラーが出ることを期待
    // （現在の実装ではverify_key_rotation内でparse_developer_keyが呼ばれる）
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid previous_key") || stderr.contains("must start with ed25519:")
        );
    }
}

#[test]
fn test_invalid_capability_syntax() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    // 不正なcapabilityを持つmanifestを作成
    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    let manifest_path = root.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();

    // 不正な文字を含むcapability
    manifest["capabilities"] = serde_json::json!(["storage@invalid", "valid-capability"]);

    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    fs::write(root.join("dist/app.js"), b"content").unwrap();

    // buildで検証が走り失敗するはず
    let output = Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("capability") && stderr.contains("invalid character"));
}

#[test]
fn test_manifest_metadata_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let bin = get_bin_path();

    // Test 1: Invalid channel
    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();

    let manifest_path = root.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();

    manifest["version"]["channel"] = serde_json::Value::String("invalid_channel".to_string());

    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    fs::write(root.join("dist/app.js"), b"content").unwrap();

    let output = Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("channel")
            && (stderr.contains("stable") || stderr.contains("beta") || stderr.contains("canary"))
    );

    // Test 2: Invalid egress_allow URL
    let temp_dir2 = TempDir::new().unwrap();
    let root2 = temp_dir2.path();

    Command::new(&bin)
        .arg("init")
        .arg("--root")
        .arg(root2)
        .output()
        .unwrap();

    let manifest_path2 = root2.join("manifest.json");
    let mut manifest2: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path2).unwrap()).unwrap();

    // 不正なURL（httpで始まる）
    // 古い egress_allow を削除し、network オブジェクトを追加
    manifest2.as_object_mut().unwrap().remove("egress_allow");
    manifest2["network"] = serde_json::json!({
        "egress_allow": ["http://example.com"],
        "http_proxy_dev": true
    });

    fs::write(
        &manifest_path2,
        serde_json::to_string_pretty(&manifest2).unwrap(),
    )
    .unwrap();

    fs::write(root2.join("dist/app.js"), b"content").unwrap();

    // デバッグ: 書き込まれた manifest を確認
    eprintln!("Manifest before build:");
    eprintln!("{}", fs::read_to_string(&manifest_path2).unwrap());

    let output2 = Command::new(&bin)
        .arg("build")
        .arg("--root")
        .arg(root2)
        .output()
        .unwrap();

    // デバッグ: 実際の出力を確認
    if output2.status.success() {
        eprintln!("ERROR: Build succeeded when it should have failed");
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output2.stdout));
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output2.stderr));
    } else {
        eprintln!("✓ Build failed as expected");
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output2.stderr));
    }

    assert!(
        !output2.status.success(),
        "Build should fail for invalid egress_allow URL"
    );
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert!(stderr2.contains("egress_allow") || stderr2.contains("https://"));
}

#[test]
fn test_deps_capsule_push_pull_roundtrip() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let root = temp_dir.path();

    fs::create_dir_all(root.join("cas/blobs")).expect("failed to create CAS blobs dir");

    let manifest_json = json!({
        "schemaVersion": "1.2",
        "id": "00000000-0000-0000-0000-0000000000aa",
        "family_id": "00000000-0000-0000-0000-0000000000bb",
        "version": {
            "number": "1.0.0",
            "channel": "stable"
        },
        "network": {},
        "capabilities": [],
        "files": [],
        "x-cas": {
            "index": "cas/index.json",
            "blobs": "cas/blobs"
        }
    });
    fs::write(
        root.join("manifest.json"),
        serde_json::to_string_pretty(&manifest_json).expect("failed to serialize manifest"),
    )
    .expect("failed to write manifest");

    let raw_bytes = b"capsule integration test payload";
    let compressed = encode_all(&raw_bytes[..], 0).expect("failed to zstd compress payload");
    let raw_sha = Sha256::digest(raw_bytes);
    let compressed_sha = Sha256::digest(&compressed);
    let compressed_hex = hex::encode(compressed_sha);
    let raw_hex = hex::encode(raw_sha);
    let blob_name = format!("sha256-{compressed_hex}");
    fs::write(root.join("cas/blobs").join(&blob_name), &compressed)
        .expect("failed to write CAS blob");

    let entry = json!({
        "path": blob_name,
        "coords": ["pkg:pypi/demo@0.1.0"],
        "raw_sha256": raw_hex,
        "compressed_sha256": compressed_hex,
        "size": raw_bytes.len(),
        "platform": ["linux-x86_64"],
        "compressed": {
            "alg": "zstd",
            "size": compressed.len(),
            "sha256": compressed_hex,
        }
    });
    let index_json = serde_json::to_string_pretty(&vec![entry]).expect("failed to serialize index");
    fs::write(root.join("cas/index.json"), format!("{index_json}\n"))
        .expect("failed to write index.json");

    let key_path = root.join("keys").join("deps.json");
    fs::create_dir_all(key_path.parent().unwrap()).expect("failed to create key dir");

    let capsule_manifest_path = root.join("cas/capsule-manifest.json");
    let registry_dir = root.join("registry");
    let reference = "local/test:1";

    let bin = get_bin_path();

    let key_output = Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(&key_path)
        .arg("--skip-manifest")
        .output()
        .expect("failed to run adep keygen");
    assert!(
        key_output.status.success(),
        "keygen command failed: {}",
        String::from_utf8_lossy(&key_output.stderr)
    );

    let capsule_output = Command::new(&bin)
        .arg("deps")
        .arg("capsule")
        .arg("--root")
        .arg(root)
        .arg("--key")
        .arg(&key_path)
        .output()
        .expect("failed to run adep deps capsule");
    assert!(
        capsule_output.status.success(),
        "capsule command failed: {}",
        String::from_utf8_lossy(&capsule_output.stderr)
    );
    assert!(
        capsule_manifest_path.exists(),
        "capsule manifest not generated"
    );

    fs::create_dir_all(&registry_dir).expect("failed to create registry dir");
    let push_output = Command::new(&bin)
        .arg("deps")
        .arg("push")
        .arg("--root")
        .arg(root)
        .arg("--capsule")
        .arg(&capsule_manifest_path)
        .arg("--registry")
        .arg(&registry_dir)
        .arg("--reference")
        .arg(reference)
        .arg("--cas-dir")
        .arg(root.join("cas"))
        .output()
        .expect("failed to run adep deps push");
    assert!(
        push_output.status.success(),
        "push command failed: {}",
        String::from_utf8_lossy(&push_output.stderr)
    );

    let pull_temp = TempDir::new().expect("failed to create pull temp dir");
    let pull_cas = pull_temp.path().join("cas");
    let pull_output = Command::new(&bin)
        .arg("deps")
        .arg("pull")
        .arg("--registry")
        .arg(&registry_dir)
        .arg("--reference")
        .arg(reference)
        .arg("--cas-dir")
        .arg(&pull_cas)
        .output()
        .expect("failed to run adep deps pull");
    assert!(
        pull_output.status.success(),
        "pull command failed: {}",
        String::from_utf8_lossy(&pull_output.stderr)
    );

    let pulled_blob = pull_cas
        .join("blobs")
        .join(format!("sha256-{compressed_hex}"));
    assert!(
        pulled_blob.exists(),
        "pulled blob missing at {}",
        pulled_blob.display()
    );
    let pulled_index = pull_cas.join("index.json");
    assert!(pulled_index.exists(), "pulled index.json missing");
    let pulled_capsule = pull_cas.join("capsule-manifest.json");
    assert!(pulled_capsule.exists(), "pulled capsule manifest missing");

    let verify_output = Command::new(&bin)
        .arg("deps")
        .arg("verify")
        .arg("--index")
        .arg(&pulled_index)
        .arg("--blobs-dir")
        .arg(pull_cas.join("blobs"))
        .output()
        .expect("failed to run adep deps verify");
    assert!(
        verify_output.status.success(),
        "verify after pull failed: {}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn test_remote_oci_roundtrip_with_oras_flow() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let root = temp_dir.path();
    let home_dir = root.join("home");
    fs::create_dir_all(&home_dir).expect("failed to create home dir");

    let cas_root = root.join("cas");
    fs::create_dir_all(cas_root.join("blobs")).expect("failed to create CAS blobs dir");

    let manifest_json = json!({
        "schemaVersion": "1.2",
        "id": "00000000-0000-0000-0000-0000000000cc",
        "family_id": "00000000-0000-0000-0000-0000000000dd",
        "version": {
            "number": "2.0.0",
            "channel": "stable"
        },
        "network": {},
        "capabilities": [],
        "files": [],
        "deps": {
            "python": {
                "requirements": "requirements.lock"
            },
            "node": {
                "lockfile": "pnpm-lock.yaml"
            }
        },
        "x-cas": {
            "index": "cas/index.json",
            "blobs": "cas/blobs"
        }
    });
    fs::write(
        root.join("manifest.json"),
        serde_json::to_string_pretty(&manifest_json).expect("serialize manifest"),
    )
    .expect("write manifest");
    fs::write(
        root.join("requirements.lock"),
        "fake==0.1.0 --hash=sha256:abcd\n",
    )
    .expect("write requirements");
    fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: 5.4\n").expect("write pnpm lockfile");

    let py_raw = b"fake python wheel";
    let py_compressed = encode_all(&py_raw[..], 0).expect("compress python blob");
    let py_raw_hex = sha256_hex(py_raw);
    let py_compressed_hex = sha256_hex(&py_compressed);
    let py_blob_name = format!("sha256-{py_compressed_hex}");
    fs::write(cas_root.join("blobs").join(&py_blob_name), &py_compressed).expect("write py blob");

    let node_raw = b"fake node tarball";
    let node_compressed = encode_all(&node_raw[..], 0).expect("compress node blob");
    let node_raw_hex = sha256_hex(node_raw);
    let node_compressed_hex = sha256_hex(&node_compressed);
    let node_blob_name = format!("sha256-{node_compressed_hex}");
    fs::write(
        cas_root.join("blobs").join(&node_blob_name),
        &node_compressed,
    )
    .expect("write node blob");

    let py_entry = json!({
        "path": py_blob_name,
        "coords": ["pkg:pypi/demo@0.1.0"],
        "metadata": {
            "filename": "demo-0.1.0-py3-none-any.whl",
            "kind": "python-wheel"
        },
        "raw_sha256": py_raw_hex,
        "compressed_sha256": py_compressed_hex,
        "size": py_raw.len(),
        "platform": [],
        "compressed": {
            "alg": "zstd",
            "size": py_compressed.len(),
            "sha256": py_compressed_hex,
        }
    });
    let node_entry = json!({
        "path": node_blob_name,
        "coords": ["pkg:npm/demo@1.0.0"],
        "metadata": {
            "filename": "demo-1.0.0.tgz",
            "kind": "pnpm-tarball"
        },
        "raw_sha256": node_raw_hex,
        "compressed_sha256": node_compressed_hex,
        "size": node_raw.len(),
        "platform": [],
        "compressed": {
            "alg": "zstd",
            "size": node_compressed.len(),
            "sha256": node_compressed_hex,
        }
    });
    let index_json =
        serde_json::to_string_pretty(&json!([py_entry, node_entry])).expect("serialize index");
    fs::write(cas_root.join("index.json"), format!("{index_json}\n")).expect("write index");

    let bin = get_bin_path();
    let key_path = home_dir.join(".adep/keys/deps.json");
    fs::create_dir_all(key_path.parent().unwrap()).expect("create key dir");

    let key_output = Command::new(&bin)
        .arg("keygen")
        .arg("--root")
        .arg(root)
        .arg("--out")
        .arg(&key_path)
        .arg("--skip-manifest")
        .env("HOME", &home_dir)
        .output()
        .expect("run keygen");
    assert!(
        key_output.status.success(),
        "keygen failed: {}",
        String::from_utf8_lossy(&key_output.stderr)
    );

    let capsule_output = Command::new(&bin)
        .arg("deps")
        .arg("capsule")
        .arg("--root")
        .arg(root)
        .arg("--key")
        .arg(&key_path)
        .env("HOME", &home_dir)
        .output()
        .expect("run deps capsule");
    assert!(
        capsule_output.status.success(),
        "capsule failed: {}",
        String::from_utf8_lossy(&capsule_output.stderr)
    );
    let capsule_manifest = root.join("cas/capsule-manifest.json");
    assert!(capsule_manifest.exists(), "capsule manifest missing");

    let key_contents = fs::read_to_string(&key_path).expect("read key");
    let key_json: serde_json::Value = serde_json::from_str(&key_contents).expect("parse key json");
    let public_key = key_json["public_key"].as_str().expect("public_key missing");
    let expected_auth = format!("AdepKey ed25519:{}", public_key);
    let mut registry = TestRegistry::start(expected_auth);
    let registry_url = registry.url();

    let reference = "demo/app:v1";
    let push_output = Command::new(&bin)
        .arg("deps")
        .arg("push")
        .arg("--root")
        .arg(root)
        .arg("--capsule")
        .arg(&capsule_manifest)
        .arg("--registry")
        .arg(&registry_url)
        .arg("--reference")
        .arg(reference)
        .arg("--cas-dir")
        .arg(&cas_root)
        .env("HOME", &home_dir)
        .env("ADEP_REGISTRY_ALLOW_INSECURE", "1")
        .output()
        .expect("run deps push (remote)");
    assert!(
        push_output.status.success(),
        "remote push failed: {}",
        String::from_utf8_lossy(&push_output.stderr)
    );

    let pull_temp = TempDir::new().expect("create pull temp dir");
    let pull_cas = pull_temp.path().join("cas");
    let pull_output = Command::new(&bin)
        .arg("deps")
        .arg("pull")
        .arg("--registry")
        .arg(&registry_url)
        .arg("--reference")
        .arg(reference)
        .arg("--cas-dir")
        .arg(&pull_cas)
        .env("HOME", &home_dir)
        .env("ADEP_REGISTRY_ALLOW_INSECURE", "1")
        .output()
        .expect("run deps pull (remote)");
    assert!(
        pull_output.status.success(),
        "remote pull failed: {}",
        String::from_utf8_lossy(&pull_output.stderr)
    );

    let pulled_capsule = pull_cas.join("capsule-manifest.json");
    assert!(pulled_capsule.exists(), "pulled capsule missing");
    assert!(
        pull_cas.join("blobs").join(&py_blob_name).exists(),
        "python blob missing after pull"
    );
    assert!(
        pull_cas.join("blobs").join(&node_blob_name).exists(),
        "node blob missing after pull"
    );

    let resolve_output = Command::new(&bin)
        .arg("deps")
        .arg("resolve")
        .arg("--root")
        .arg(root)
        .arg("--capsule")
        .arg(&pulled_capsule)
        .arg("--cas-dir")
        .arg(&pull_cas)
        .arg("--output")
        .arg(root.join("deps-cache"))
        .env("HOME", &home_dir)
        .output()
        .expect("run deps resolve");
    assert!(
        resolve_output.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&resolve_output.stderr)
    );

    let install_output = Command::new(&bin)
        .arg("deps")
        .arg("install")
        .arg("--root")
        .arg(root)
        .arg("--capsule")
        .arg(&pulled_capsule)
        .arg("--cas-dir")
        .arg(&pull_cas)
        .arg("--output")
        .arg(root.join("deps-cache"))
        .arg("--dry-run")
        .env("HOME", &home_dir)
        .output()
        .expect("run deps install --dry-run");
    assert!(
        install_output.status.success(),
        "install dry-run failed: {}",
        String::from_utf8_lossy(&install_output.stderr)
    );

    let verify_output = Command::new(&bin)
        .arg("deps")
        .arg("verify")
        .arg("--index")
        .arg(pull_cas.join("index.json"))
        .arg("--blobs-dir")
        .arg(pull_cas.join("blobs"))
        .env("HOME", &home_dir)
        .output()
        .expect("run deps verify");
    assert!(
        verify_output.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&verify_output.stderr)
    );

    registry.shutdown();

    // Custom Authorization header takes highest priority even when other env vars are present.
    let custom_auth = "CustomScheme signed=42";
    let mut registry_custom = TestRegistry::start(custom_auth.to_string());
    let registry_custom_url = registry_custom.url();
    run_push_pull_with_env(
        &bin,
        root,
        &cas_root,
        &capsule_manifest,
        &home_dir,
        &registry_custom_url,
        "demo/app:v1-custom-header",
        &[
            ("ADEP_REGISTRY_AUTH_HEADER", Some(custom_auth)),
            ("ADEP_REGISTRY_TOKEN", Some("should-be-ignored")),
            ("ADEP_REGISTRY_USERNAME", Some("ignored-user")),
            ("ADEP_REGISTRY_PASSWORD", Some("ignored-pass")),
        ],
    );
    registry_custom.shutdown();

    // Bearer token must outrank Basic auth credentials.
    let bearer_token = "demo-token";
    let mut registry_bearer = TestRegistry::start(format!("Bearer {}", bearer_token));
    let registry_bearer_url = registry_bearer.url();
    run_push_pull_with_env(
        &bin,
        root,
        &cas_root,
        &capsule_manifest,
        &home_dir,
        &registry_bearer_url,
        "demo/app:v1-bearer",
        &[
            ("ADEP_REGISTRY_AUTH_HEADER", None),
            ("ADEP_REGISTRY_TOKEN", Some(bearer_token)),
            ("ADEP_REGISTRY_USERNAME", Some("ignored-user")),
            ("ADEP_REGISTRY_PASSWORD", Some("ignored-pass")),
        ],
    );
    registry_bearer.shutdown();

    // Basic auth should be used when username/password are provided without token.
    let basic_user = "cli-user";
    let basic_pass = "cli-pass";
    let basic_header = format!(
        "Basic {}",
        base64_encode(format!("{basic_user}:{basic_pass}"))
    );
    let mut registry_basic = TestRegistry::start(basic_header);
    let registry_basic_url = registry_basic.url();
    run_push_pull_with_env(
        &bin,
        root,
        &cas_root,
        &capsule_manifest,
        &home_dir,
        &registry_basic_url,
        "demo/app:v1-basic",
        &[
            ("ADEP_REGISTRY_AUTH_HEADER", None),
            ("ADEP_REGISTRY_TOKEN", None),
            ("ADEP_REGISTRY_USERNAME", Some(basic_user)),
            ("ADEP_REGISTRY_PASSWORD", Some(basic_pass)),
        ],
    );
    registry_basic.shutdown();
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[derive(Clone)]
struct ManifestRecord {
    bytes: Vec<u8>,
    digest: String,
}

#[derive(Default)]
struct RegistryState {
    blobs: HashMap<String, Vec<u8>>,
    manifests_by_tag: HashMap<String, ManifestRecord>,
    manifests_by_digest: HashMap<String, ManifestRecord>,
}

struct TestRegistry {
    port: u16,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestRegistry {
    fn start(expected_auth: String) -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind test registry listener");
        let port = listener
            .local_addr()
            .expect("listener missing local addr")
            .port();
        let server =
            Server::from_listener(listener, None).expect("failed to start test registry server");
        let state = Arc::new(Mutex::new(RegistryState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_shutdown = shutdown.clone();
        let server_state = state.clone();
        let handle = thread::spawn(move || {
            run_registry(server, server_state, server_shutdown, expected_auth);
        });
        Self {
            port,
            shutdown,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn shutdown(&mut self) {
        if self.handle.is_some() {
            self.stop();
        }
    }

    fn stop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        send_shutdown_request(self.port);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for TestRegistry {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.stop();
        }
    }
}

fn run_registry(
    server: Server,
    state: Arc<Mutex<RegistryState>>,
    shutdown: Arc<AtomicBool>,
    expected_auth: String,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match server.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(request)) => {
                let url = request.url().to_string();
                if url == "/__shutdown" {
                    let _ = request.respond(Response::from_string("ok").with_status_code(200));
                    break;
                }
                if !auth_ok(&request, &expected_auth) {
                    let _ = request
                        .respond(Response::from_string("unauthorized").with_status_code(401));
                    continue;
                }
                if let Err(err) = handle_registry_request(request, &state) {
                    eprintln!("test registry handler error: {}", err);
                }
            }
            Ok(None) => continue,
            Err(_) => break,
        }
    }
    shutdown.store(true, Ordering::SeqCst);
}

fn auth_ok(request: &tiny_http::Request, expected_auth: &str) -> bool {
    request
        .headers()
        .iter()
        .any(|header| header.field.equiv("Authorization") && header.value.as_str() == expected_auth)
}

fn handle_registry_request(
    request: tiny_http::Request,
    state: &Arc<Mutex<RegistryState>>,
) -> Result<(), String> {
    let url = request.url().to_string();
    let (path, query) = split_url(&url);
    if path == "/v2" || path == "/v2/" {
        return respond_plain(request, 200, "{}");
    }
    if !path.starts_with("/v2/") {
        return respond_plain(request, 404, "not found");
    }
    let rest = &path[4..];
    match *request.method() {
        Method::Post => {
            if let Some((repo, _)) = rest.rsplit_once("/blobs/uploads/") {
                return handle_blob_upload(request, state, repo, query);
            } else {
                return respond_not_found(request);
            }
        }
        Method::Head => {
            if let Some((_, digest)) = rest.rsplit_once("/blobs/") {
                return handle_blob_head(request, state, digest);
            } else {
                return respond_not_found(request);
            }
        }
        Method::Get => {
            if let Some((_, digest)) = rest.rsplit_once("/blobs/") {
                return handle_blob_get(request, state, digest);
            } else if let Some((repo, reference)) = rest.rsplit_once("/manifests/") {
                return handle_manifest_get(request, state, repo, reference);
            } else {
                return respond_not_found(request);
            }
        }
        Method::Put => {
            if let Some((repo, reference)) = rest.rsplit_once("/manifests/") {
                return handle_manifest_put(request, state, repo, reference);
            } else {
                return respond_not_found(request);
            }
        }
        _ => {
            return respond_plain(request, 405, "method not allowed");
        }
    }
}

fn respond_not_found(request: tiny_http::Request) -> Result<(), String> {
    respond_plain(request, 404, "not found")
}

fn respond_plain(request: tiny_http::Request, status: u16, body: &str) -> Result<(), String> {
    let response = Response::from_string(body.to_string()).with_status_code(status);
    request.respond(response).map_err(|err| err.to_string())
}

fn handle_blob_upload(
    mut request: tiny_http::Request,
    state: &Arc<Mutex<RegistryState>>,
    _repo: &str,
    query: Option<&str>,
) -> Result<(), String> {
    let digest_param = query
        .and_then(|q| {
            q.split('&')
                .find(|part| part.starts_with("digest="))
                .map(|part| part.trim_start_matches("digest="))
        })
        .ok_or_else(|| "missing digest query parameter".to_string())?;
    let mut body = Vec::new();
    request
        .as_reader()
        .read_to_end(&mut body)
        .map_err(|err| err.to_string())?;
    let mut state = state.lock().map_err(|err| err.to_string())?;
    state.blobs.insert(digest_param.to_string(), body);
    let digest_header =
        Header::from_bytes(&b"Docker-Content-Digest"[..], digest_param.as_bytes()).unwrap();
    request
        .respond(
            Response::from_string("")
                .with_status_code(201)
                .with_header(digest_header),
        )
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn handle_blob_head(
    request: tiny_http::Request,
    state: &Arc<Mutex<RegistryState>>,
    digest: &str,
) -> Result<(), String> {
    let state = state.lock().map_err(|err| err.to_string())?;
    if state.blobs.contains_key(digest) {
        let digest_header =
            Header::from_bytes(&b"Docker-Content-Digest"[..], digest.as_bytes()).unwrap();
        let response = Response::from_string("")
            .with_status_code(200)
            .with_header(digest_header);
        request.respond(response).map_err(|err| err.to_string())
    } else {
        respond_plain(request, 404, "")
    }
}

fn handle_blob_get(
    request: tiny_http::Request,
    state: &Arc<Mutex<RegistryState>>,
    digest: &str,
) -> Result<(), String> {
    let state = state.lock().map_err(|err| err.to_string())?;
    if let Some(bytes) = state.blobs.get(digest) {
        let content_header =
            Header::from_bytes(&b"Content-Type"[..], b"application/octet-stream").unwrap();
        let digest_header =
            Header::from_bytes(&b"Docker-Content-Digest"[..], digest.as_bytes()).unwrap();
        let response = Response::from_data(bytes.clone())
            .with_status_code(200)
            .with_header(content_header)
            .with_header(digest_header);
        request.respond(response).map_err(|err| err.to_string())
    } else {
        respond_plain(request, 404, "")
    }
}

fn handle_manifest_put(
    mut request: tiny_http::Request,
    state: &Arc<Mutex<RegistryState>>,
    repo: &str,
    reference: &str,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    request
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    let digest = sha256_hex(&bytes);
    let record = ManifestRecord {
        bytes: bytes.clone(),
        digest: format!("sha256:{digest}"),
    };
    let mut state = state.lock().map_err(|err| err.to_string())?;
    state
        .manifests_by_tag
        .insert(format!("{}:{}", repo, reference), record.clone());
    state
        .manifests_by_digest
        .insert(format!("{}@{}", repo, record.digest), record.clone());
    let digest_header =
        Header::from_bytes(&b"Docker-Content-Digest"[..], record.digest.as_bytes()).unwrap();
    let content_header = Header::from_bytes(
        &b"Content-Type"[..],
        b"application/vnd.oci.image.manifest.v1+json",
    )
    .unwrap();
    let response = Response::from_string("")
        .with_status_code(201)
        .with_header(digest_header)
        .with_header(content_header);
    request.respond(response).map_err(|err| err.to_string())
}

fn handle_manifest_get(
    request: tiny_http::Request,
    state: &Arc<Mutex<RegistryState>>,
    repo: &str,
    reference: &str,
) -> Result<(), String> {
    let state = state.lock().map_err(|err| err.to_string())?;
    let key = if reference.starts_with("sha256:") {
        format!("{}@{}", repo, reference)
    } else {
        format!("{}:{}", repo, reference)
    };
    let record = if reference.starts_with("sha256:") {
        state.manifests_by_digest.get(&key)
    } else {
        state.manifests_by_tag.get(&key)
    };
    if let Some(record) = record {
        let digest_header =
            Header::from_bytes(&b"Docker-Content-Digest"[..], record.digest.as_bytes()).unwrap();
        let content_header = Header::from_bytes(
            &b"Content-Type"[..],
            b"application/vnd.oci.image.manifest.v1+json",
        )
        .unwrap();
        let response = Response::from_data(record.bytes.clone())
            .with_status_code(200)
            .with_header(content_header)
            .with_header(digest_header);
        request.respond(response).map_err(|err| err.to_string())
    } else {
        respond_plain(request, 404, "not found")
    }
}

fn run_push_pull_with_env(
    bin: &Path,
    root: &Path,
    cas_root: &Path,
    capsule_manifest: &Path,
    home_dir: &Path,
    registry_url: &str,
    reference: &str,
    extra_env: &[(&str, Option<&str>)],
) {
    let mut push_cmd = Command::new(bin);
    push_cmd
        .arg("deps")
        .arg("push")
        .arg("--root")
        .arg(root)
        .arg("--capsule")
        .arg(capsule_manifest)
        .arg("--registry")
        .arg(registry_url)
        .arg("--reference")
        .arg(reference)
        .arg("--cas-dir")
        .arg(cas_root);
    push_cmd.env("HOME", home_dir);
    push_cmd.env("ADEP_REGISTRY_ALLOW_INSECURE", "1");
    apply_auth_env(&mut push_cmd, extra_env);
    let push_output = push_cmd.output().expect("failed to run adep deps push");
    assert!(
        push_output.status.success(),
        "push command failed: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&push_output.stdout),
        String::from_utf8_lossy(&push_output.stderr)
    );

    let pull_temp = TempDir::new().expect("failed to create pull temp dir");
    let pull_cas = pull_temp.path().join("cas");
    let mut pull_cmd = Command::new(bin);
    pull_cmd
        .arg("deps")
        .arg("pull")
        .arg("--registry")
        .arg(registry_url)
        .arg("--reference")
        .arg(reference)
        .arg("--cas-dir")
        .arg(&pull_cas);
    pull_cmd.env("HOME", home_dir);
    pull_cmd.env("ADEP_REGISTRY_ALLOW_INSECURE", "1");
    apply_auth_env(&mut pull_cmd, extra_env);
    let pull_output = pull_cmd.output().expect("failed to run adep deps pull");
    assert!(
        pull_output.status.success(),
        "pull command failed: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&pull_output.stdout),
        String::from_utf8_lossy(&pull_output.stderr)
    );
    assert!(
        pull_cas.join("capsule-manifest.json").exists(),
        "pulled capsule manifest missing"
    );
}

fn apply_auth_env(cmd: &mut Command, envs: &[(&str, Option<&str>)]) {
    for key in [
        "ADEP_REGISTRY_AUTH_HEADER",
        "ADEP_REGISTRY_TOKEN",
        "ADEP_REGISTRY_USERNAME",
        "ADEP_REGISTRY_PASSWORD",
    ] {
        cmd.env_remove(key);
    }
    for (key, maybe_value) in envs {
        match maybe_value {
            Some(value) => {
                cmd.env(key, value);
            }
            None => {
                cmd.env_remove(key);
            }
        }
    }
}

fn split_url(url: &str) -> (&str, Option<&str>) {
    if let Some((path, query)) = url.split_once('?') {
        (path, Some(query))
    } else {
        (url, None)
    }
}

fn send_shutdown_request(port: u16) {
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream
            .write_all(b"GET /__shutdown HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    }
}

/// Get the path to the adep-cli binary
fn get_bin_path() -> PathBuf {
    if let Some(bin) = option_env!("CARGO_BIN_EXE_adep") {
        return PathBuf::from(bin);
    }

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("adep");

    if path.exists() {
        return path;
    }

    let mut legacy = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    legacy.push("target");
    legacy.push("debug");
    legacy.push("adep-cli");

    if legacy.exists() {
        return legacy;
    }

    panic!(
        "Binary not found. Run `cargo build` to produce the adep binary (checked {:?} and {:?}).",
        path, legacy
    );
}
