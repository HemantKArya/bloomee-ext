use serde::{Deserialize, Serialize};

/// The five active plugin archetypes supported by bex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginArchetype {
    #[serde(rename = "content-resolver")]
    ContentResolver,
    #[serde(rename = "lyrics-provider")]
    LyricsProvider,
    #[serde(rename = "chart-provider")]
    ChartProvider,
    #[serde(rename = "search-suggestion-provider")]
    SearchSuggestionProvider,
    #[serde(rename = "content-importer")]
    ContentImporter,
}

impl PluginArchetype {
    pub fn wit_world(&self) -> &'static str {
        match self {
            Self::ContentResolver => "content-resolver",
            Self::LyricsProvider => "lyrics-provider",
            Self::ChartProvider => "chart-provider",
            Self::SearchSuggestionProvider => "search-suggestion-provider",
            Self::ContentImporter => "content-importer",
        }
    }
}

impl std::fmt::Display for PluginArchetype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.wit_world())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub plugin_type: PluginArchetype,
    pub publisher: Publisher,
    pub description: Option<String>,
    // Whether this plugin implements `get_streams` (true) or is metadata-only (false).
    #[serde(default = "default_true")]
    pub resolver: bool,
    pub created_at: Option<String>,
    pub last_updated: Option<String>,
    // The runtime always expects the built component to be named `plugin.wasm`.
    // The manifest no longer includes an `entry` field.
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Publisher {
    pub name: String,
    pub url: Option<String>,
    pub contact: Option<String>,
}
