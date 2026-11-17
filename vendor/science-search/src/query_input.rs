use schemars::JsonSchema;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};

/// The input for the search tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryInput {
    /// The query to search for. Escape special characters as needed.
    pub query: String,
}

/// Accepts either a plain string `"search query"` or an object `{"query": "search query"}`.
impl<'de> Deserialize<'de> for QueryInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(QueryInputVisitor)
    }
}

struct QueryInputVisitor;

impl<'de> Visitor<'de> for QueryInputVisitor {
    type Value = QueryInput;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string or { query: string }")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(QueryInput {
            query: v.to_owned(),
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut query: Option<String> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "query" => {
                    if query.is_some() {
                        return Err(serde::de::Error::duplicate_field("query"));
                    }
                    query = Some(map.next_value()?);
                }
                other => return Err(serde::de::Error::unknown_field(other, &["query"])),
            }
        }
        let query = query.ok_or_else(|| serde::de::Error::missing_field("query"))?;
        Ok(QueryInput { query })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn accepts_plain_string_and_object() {
        let from_str: QueryInput = serde_json::from_value(json!("crispr")).unwrap();
        let from_obj: QueryInput = serde_json::from_value(json!({ "query": "crispr" })).unwrap();
        assert_eq!(from_str.query, "crispr");
        assert_eq!(from_obj.query, "crispr");
    }

    #[test]
    fn rejects_unknown_fields() {
        let res = serde_json::from_value::<QueryInput>(json!({ "query": "x", "limit": 3 }));
        assert!(res.is_err());
    }
}
