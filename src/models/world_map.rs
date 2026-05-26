use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WorldEntry {
    index: String,
    unique_name: String,
}

#[derive(Debug)]
pub struct WorldMap {
    by_index: HashMap<String, String>,
    by_name: HashMap<String, String>,
}

impl WorldMap {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        let entries: Vec<WorldEntry> = serde_json::from_str(json)?;

        let mut by_index = HashMap::new();
        let mut by_name = HashMap::new();

        for entry in entries {
            by_name.insert(entry.unique_name.clone(), entry.index.clone());
            by_index.insert(entry.index, entry.unique_name);
        }

        Ok(Self { by_index, by_name })
    }

    pub fn name_from_index(&self, index: &str) -> Option<&str> {
        self.by_index.get(index).map(String::as_str)
    }

    pub fn index_from_name(&self, name: &str) -> Option<&str> {
        self.by_name.get(name).map(String::as_str)
    }
}