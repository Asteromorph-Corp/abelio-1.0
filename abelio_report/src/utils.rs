use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

static RE_BRACKET_LOWER: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"\[([a-z]+)\]"));

pub fn extract_lowercase_in_brackets(s: &str) -> eyre::Result<Vec<String>> {
    let re = RE_BRACKET_LOWER.as_ref().map_err(|e| eyre::eyre!("{e}"))?;

    let mut set = HashSet::new();

    for caps in re.captures_iter(s) {
        set.insert(caps[1].to_string());
    }

    let result: Vec<String> = set.into_iter().collect();
    Ok(result)
}

pub fn substitute_lowercase_in_brackets(s: &str) -> eyre::Result<String> {
    let re = RE_BRACKET_LOWER.as_ref().map_err(|e| eyre::eyre!("{e}"))?;

    let result = re.replace_all(s, |caps: &regex::Captures| format!("[^{}]", &caps[1]));

    Ok(result.to_string())
}

pub fn take_citations(
    hypothesis: &str,
    citation_list: &HashMap<String, String>,
) -> eyre::Result<HashMap<String, String>> {
    let citation_keys = extract_lowercase_in_brackets(hypothesis)?;

    let citations = citation_keys
        .into_iter()
        .map(|c| {
            (
                c.clone(),
                citation_list.get(&c).cloned().unwrap_or_default(),
            )
        })
        .collect::<HashMap<String, String>>();

    Ok(citations)
}

static TITLE_DOI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?mi)^\s*#\s*(?<title>.+?)\s*$[\s\S]*?^\s*DOI:\s*(?<doi>https?://doi\.org/\S+)\s*$",
    )
    .expect("invalid regex")
});

pub fn extract_title_doi(text: &str) -> (Option<&str>, Option<&str>) {
    let Some(caps) = TITLE_DOI_RE.captures(text) else {
        return (None, None);
    };

    let title = caps.name("title").map(|m| m.as_str());
    let doi = caps.name("doi").map(|m| m.as_str());

    (title, doi)
}
