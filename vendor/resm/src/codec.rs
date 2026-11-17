use serde::Serialize;

/// Pretty-printed JSON, the encoding the stores write for structured values.
#[derive(Debug, Clone)]
pub struct JsonCodec(());

impl JsonCodec {
    pub fn pretty() -> Self {
        Self(())
    }

    pub(crate) fn encode<T: Serialize>(&self, value: &T) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec_pretty(value)
    }
}
