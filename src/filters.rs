use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct FilterDef {
    pub name: String,
    pub gmail_query: String,
}

#[derive(Debug, Deserialize)]
struct FiltersConfig {
    filters: Vec<FilterDef>,
}

pub fn load_filters(path: &str) -> Result<Vec<FilterDef>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read filters config '{}': {}", path, e))?;
    let config: FiltersConfig = toml::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse filters config: {}", e))?;
    if config.filters.is_empty() {
        return Err(anyhow!("No filters defined in config"));
    }
    for filter in &config.filters {
        if filter.name.trim().is_empty() {
            return Err(anyhow!("Filter has empty name"));
        }
        if filter.gmail_query.trim().is_empty() {
            return Err(anyhow!(
                "Filter '{}' has empty gmail_query",
                filter.name
            ));
        }
    }
    Ok(config.filters)
}
