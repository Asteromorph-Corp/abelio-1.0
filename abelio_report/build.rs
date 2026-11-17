fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    talin_build::build_artifacts()?;
    Ok(())
}
