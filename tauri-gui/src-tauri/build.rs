fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tridentd/src/proto/tridentd.proto");
    tonic_build::compile_protos(proto_path)?;
    Ok(())
}
