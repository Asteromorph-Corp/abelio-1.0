use talin_build::build_artifacts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_artifacts()?;

    // protox compiles the proto in-process, so the build needs no system `protoc`.
    let fds = protox::compile(["tei/v1/tei.proto"], ["protos"])?;
    tonic_prost_build::configure().compile_fds(fds)?;
    println!("cargo::rerun-if-changed=protos");
    Ok(())
}
