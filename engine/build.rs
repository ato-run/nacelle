fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .compile_well_known_types(false)
        .out_dir("src/proto")
        .compile(
            &[
                "../proto/common.proto",
                "../proto/engine.proto",
                "../proto/coordinator/v1/coordinator.proto",
            ],
            &["../proto"], // .proto ファイルのインクルードパス
        )?;
    Ok(())
}
