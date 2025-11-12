fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .compile_well_known_types(true)
        .out_dir("src/proto")
        .compile(
            &["../proto/engine.proto", "../proto/coordinator.proto"],
            &["../proto"], // .proto ファイルのインクルードパス
        )?;
    Ok(())
}
