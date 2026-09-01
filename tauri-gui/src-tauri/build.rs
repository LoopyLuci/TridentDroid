fn main() {
    let proto_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tridentd/src/proto/tridentd.proto");
    tonic_build::compile_protos(proto_path).expect("failed to compile tridentd.proto");

    // Embeds the Common-Controls-v6 manifest and app icon/version resources on Windows.
    // This was previously missing, which is why manifest embedding was worked around
    // via a manual mt.exe post-build step and a hardcoded-path linker flag.
    tauri_build::build();
}
