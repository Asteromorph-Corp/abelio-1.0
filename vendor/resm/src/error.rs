use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Codec error: {0}")]
    Codec(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// An error raised by the computation inside a `cached_*!` block.
    #[error("User error: {0}")]
    User(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// One failure delivered to every task coalesced on the same key.
    #[error("Shared error: {0}")]
    Shared(Arc<Error>),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn codec<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Codec(Box::new(err))
    }

    pub fn backend<E>(err: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self::Backend(err.into())
    }

    /// Wraps `err` as `User`, unless it already is a resm `Error`, which is passed through
    /// unchanged.
    pub fn user<E>(err: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let boxed: Box<dyn std::error::Error + Send + Sync> = err.into();

        match boxed.downcast::<Error>() {
            Ok(resm_err) => *resm_err,
            Err(err) => Self::User(err),
        }
    }
}
