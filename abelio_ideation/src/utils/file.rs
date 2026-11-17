use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

pub struct Importer {
    pub path: PathBuf,
}

impl Importer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let path = root.as_ref().to_path_buf();

        if !path.exists() {
            panic!("Input directory does not exist: {}", path.display());
        }

        Self { path }
    }

    pub fn iter_dir(
        &self,
        ext: Option<&str>,
    ) -> Result<impl Iterator<Item = PathBuf>, std::io::Error> {
        let entries = std::fs::read_dir(&self.path)?
            .filter_map(Result::ok)
            .map(|entry| entry.path());

        let filtered: Box<dyn Iterator<Item = PathBuf>> =
            match ext {
                Some(extension) => Box::new(entries.filter(move |path| {
                    path.extension().and_then(|e| e.to_str()) == Some(extension)
                })),
                None => Box::new(entries),
            };

        Ok(filtered)
    }

    pub fn import_json_immed<T: DeserializeOwned>(
        path: impl AsRef<Path>,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = path.as_ref();
        if !file_path.exists() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {}", file_path.display()),
            )));
        }
        let file_content = std::fs::read_to_string(file_path)?;
        let value = serde_json::from_str(&file_content)?;

        Ok(value)
    }
}
