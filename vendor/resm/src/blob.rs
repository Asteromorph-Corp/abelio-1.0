use std::borrow::Cow;

use bytes::Bytes;

/// File extension a store uses for a blob (`"md"`, `"json"`, ...), persisted by the file and redb
/// backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreHint(Cow<'static, str>);

impl StoreHint {
    pub const MD: Self = Self(Cow::Borrowed("md"));
    pub const JSON: Self = Self(Cow::Borrowed("json"));

    pub(crate) fn custom(ext: impl Into<Cow<'static, str>>) -> Self {
        Self(ext.into())
    }

    pub fn extension(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct EntityBlob {
    pub hint: StoreHint,
    pub bytes: Bytes,
}
