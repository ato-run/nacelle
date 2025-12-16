fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let proto_root = std::path::Path::new(&manifest_dir).join("..").join("proto");
    let proto_file = proto_root.join("depsd/v1/depsd.proto");
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .out_dir(std::env::var("OUT_DIR")?)
        .compile(&[proto_file], &[proto_root])?;
    println!("cargo:rerun-if-changed=../proto/depsd/v1/depsd.proto");
    println!("cargo:rerun-if-changed=../proto");
    Ok(())
}
