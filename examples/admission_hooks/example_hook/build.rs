fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(false)
        .compile_protos(
            &["proto/grpc_api.proto"],
            &["proto", "../../../ankaios_api/proto"],
        )?;
    Ok(())
}
