fn main() {
    // Get the vendored protoc path
    let protoc_path = protoc_bin_vendored::protoc_bin_path()
        .expect("Failed to locate vendored protoc");

    // Set the environment variable so prost/tonic uses this binary
    std::env::set_var("PROTOC", protoc_path);

    // Optional: use a prost config if you want customization
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(&["proto/capture.proto"], &["proto"])
        .expect("Failed to compile .proto files");

    println!("cargo:rerun-if-changed=proto/capture.proto");
    println!("cargo:rerun-if-changed=proto");
}
