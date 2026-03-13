use crate::manifest::current_timestamp;
use anyhow::{Context, Result};
use inquire::{Select, Text};
use std::fs;
use std::path::Path;

// ── Embedded bex-core snapshot (bundled at compile time) ─────────────────────
const BCK_CARGO_TOML: &str = include_str!("../assets/bex-core/Cargo.toml");
const BCK_LIB_RS: &str = include_str!("../assets/bex-core/src/lib.rs");
const BCK_MACROS_RS: &str = include_str!("../assets/bex-core/src/macros.rs");
const BCK_RESOLVER_RS: &str = include_str!("../assets/bex-core/src/resolver.rs");
const BCK_LYRICS_RS: &str = include_str!("../assets/bex-core/src/lyrics.rs");
const BCK_CHART_RS: &str = include_str!("../assets/bex-core/src/chart.rs");
const BCK_SUGGESTION_RS: &str = include_str!("../assets/bex-core/src/suggestion.rs");
const BCK_IMPORTER_RS: &str = include_str!("../assets/bex-core/src/importer.rs");
const BCW_RESOLVER: &str = include_str!("../assets/bex-core/wit/resolver/content-resolver.wit");
const BCW_LYRICS: &str = include_str!("../assets/bex-core/wit/lyrics/lyrics-provider.wit");
const BCW_CHART: &str = include_str!("../assets/bex-core/wit/chart/chart-provider.wit");
const BCW_SUGGESTION: &str =
    include_str!("../assets/bex-core/wit/suggestion/search-suggestion-provider.wit");
const BCW_SCROBBLER: &str = include_str!("../assets/bex-core/wit/scrobbler/scrobbler.wit");
const BCW_IMPORTER: &str = include_str!("../assets/bex-core/wit/importer/content-importer.wit");

/// Extract a complete bex-core source tree into `<plugin>/bex-core/`.
/// This makes every created plugin self-contained — no external path needed.
fn extract_bex_core(plugin_dir: &Path) -> Result<()> {
    let core = plugin_dir.join("bex-core");
    let src = core.join("src");
    let wit = core.join("wit");
    for sub in &[
        "resolver",
        "lyrics",
        "chart",
        "suggestion",
        "scrobbler",
        "importer",
    ] {
        fs::create_dir_all(wit.join(sub))?;
    }
    fs::create_dir_all(&src)?;

    fs::write(core.join("Cargo.toml"), BCK_CARGO_TOML)?;
    fs::write(src.join("lib.rs"), BCK_LIB_RS)?;
    fs::write(src.join("macros.rs"), BCK_MACROS_RS)?;
    fs::write(src.join("resolver.rs"), BCK_RESOLVER_RS)?;
    fs::write(src.join("lyrics.rs"), BCK_LYRICS_RS)?;
    fs::write(src.join("chart.rs"), BCK_CHART_RS)?;
    fs::write(src.join("suggestion.rs"), BCK_SUGGESTION_RS)?;
    fs::write(src.join("importer.rs"), BCK_IMPORTER_RS)?;
    fs::write(wit.join("resolver/content-resolver.wit"), BCW_RESOLVER)?;
    fs::write(wit.join("lyrics/lyrics-provider.wit"), BCW_LYRICS)?;
    fs::write(wit.join("chart/chart-provider.wit"), BCW_CHART)?;
    fs::write(
        wit.join("suggestion/search-suggestion-provider.wit"),
        BCW_SUGGESTION,
    )?;
    fs::write(wit.join("scrobbler/scrobbler.wit"), BCW_SCROBBLER)?;
    fs::write(wit.join("importer/content-importer.wit"), BCW_IMPORTER)?;
    Ok(())
}

/// Interactive wizard to create a new BEX plugin project.
pub fn run_create() -> Result<()> {
    println!("╔══════════════════════════════════════╗");
    println!("║  Bloomee Plugin Creator  (bex create) ║");
    println!("╚══════════════════════════════════════╝\n");

    let type_options = vec![
        "content-resolver      — Search, browse, stream music from a service",
        "lyrics-provider       — Fetch plain/synced lyrics for tracks",
        "chart-provider        — Provide music charts and trending lists",
        "search-suggestion-provider — Autocomplete + entity suggestions",
        "content-importer      — Import playlists/albums from Spotify, YT Music, etc.",
    ];

    let choice = Select::new("Plugin type:", type_options.clone()).prompt()?;

    let archetype = if choice.starts_with("content-resolver ") {
        "content-resolver"
    } else if choice.starts_with("lyrics") {
        "lyrics-provider"
    } else if choice.starts_with("chart") {
        "chart-provider"
    } else if choice.starts_with("content-importer") {
        "content-importer"
    } else {
        "search-suggestion-provider"
    };

    let plugin_name = Text::new("Plugin name (kebab-case, e.g. my-music-service):").prompt()?;
    if plugin_name.is_empty() {
        anyhow::bail!("Plugin name cannot be empty.");
    }
    if plugin_name.contains(' ') {
        anyhow::bail!("Plugin name must not contain spaces — use hyphens instead (e.g. my-music).");
    }

    let author_name = Text::new("Author / publisher name:").prompt()?;
    let publisher_url = Text::new("Publisher URL (optional, Enter to skip):")
        .with_default("")
        .prompt()
        .unwrap_or_default();
    let publisher_contact = Text::new("Contact email or social (optional, Enter to skip):")
        .with_default("")
        .prompt()
        .unwrap_or_default();
    let description = Text::new("Short description:").prompt()?;
    let thumbnail_url = Text::new("Thumbnail URL (optional, Enter to skip):")
        .with_default("")
        .prompt()
        .unwrap_or_default();

    println!("\nCreating `{}`…", plugin_name);

    let dir = Path::new(&plugin_name);
    if dir.exists() {
        anyhow::bail!("Directory `{}` already exists.", plugin_name);
    }

    fs::create_dir_all(dir.join("src"))?;

    let feature = match archetype {
        "content-resolver" => "resolver",
        "lyrics-provider" => "lyrics",
        "chart-provider" => "chart",
        "search-suggestion-provider" => "suggestion",
        "content-importer" => "importer",
        _ => unreachable!(),
    };

    // ── Cargo.toml ────────────────────────────────────────────────────────
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
description = "{desc}"

[dependencies]
bex-core = {{ path = "bex-core", features = ["{feature}"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
anyhow = "1.0"

[lib]
crate-type = ["cdylib"]

[package.metadata.component]
package = "component:{world}"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
panic = "abort"
debug = false
"#,
        name = plugin_name,
        desc = description,
        feature = feature,
        world = archetype,
    );

    // ── manifest.json ─────────────────────────────────────────────────────
    let author_id = author_name
        .to_lowercase()
        .split_whitespace()
        .collect::<String>();
    let plugin_id = plugin_name.replace('-', "");
    let timestamp = current_timestamp();
    let maybe_thumbnail = if thumbnail_url.is_empty() {
        "null".to_string()
    } else {
        format!("\"{}\"", thumbnail_url)
    };
    let maybe_url = if publisher_url.is_empty() {
        String::new()
    } else {
        format!(", \"url\": \"{}\"", publisher_url)
    };
    let maybe_contact = if publisher_contact.is_empty() {
        String::new()
    } else {
        format!(", \"contact\": \"{}\"", publisher_contact)
    };
    let resolver_field = if archetype == "content-resolver" {
        ",\n  \"resolver\": true".to_string()
    } else {
        String::new()
    };
    let manifest = format!(
        r#"{{
  "id": "{archetype}.{author_id}.{plugin_id}",
  "name": "{display_name}",
  "version": "1",
  "type": "{archetype}",
  "publisher": {{"name": "{author_name}"{maybe_url}{maybe_contact}}},
  "description": "{desc}"{resolver_field},
  "manifest_version": "1.0",
  "created_at": "{timestamp}",
  "last_updated": "{timestamp}",
  "thumbnail_url": {maybe_thumbnail}
}}
"#,
        archetype = archetype,
        author_id = author_id,
        plugin_id = plugin_id,
        display_name = plugin_name,
        author_name = author_name,
        desc = description,
        maybe_url = maybe_url,
        maybe_contact = maybe_contact,
        resolver_field = resolver_field,
        timestamp = timestamp,
        maybe_thumbnail = maybe_thumbnail,
    );

    // ── src/lib.rs template ───────────────────────────────────────────────
    let lib_rs = template_lib_rs(archetype, &plugin_name);

    // ── Write files ───────────────────────────────────────────────────────
    fs::write(dir.join("Cargo.toml"), cargo_toml).context("Writing Cargo.toml")?;
    fs::write(dir.join("manifest.json"), manifest).context("Writing manifest.json")?;
    fs::write(dir.join("src/lib.rs"), lib_rs).context("Writing src/lib.rs")?;

    // Extract bex-core alongside the new plugin (makes it self-contained)
    print!("  Bundling bex-core… ");
    extract_bex_core(dir).context("Extracting bex-core")?;
    println!("done");

    println!();
    println!("✓ Created `{}/`", plugin_name);
    println!();
    println!("Next steps:");
    println!("  cd {}", plugin_name);
    println!("  bex build    # compile to WASM component");
    println!("  bex test     # run interactively against the embedded host");
    println!("  bex pack     # create distributable {}.bex", plugin_name);

    Ok(())
}

// ── Template generators ───────────────────────────────────────────────────────

fn template_lib_rs(archetype: &str, plugin_name: &str) -> String {
    match archetype {
        "chart-provider" => template_chart(plugin_name),
        "lyrics-provider" => template_lyrics(plugin_name),
        "content-resolver" => template_resolver(plugin_name),
        "search-suggestion-provider" => template_suggestion(plugin_name),
        "content-importer" => template_importer(plugin_name),
        _ => unreachable!(),
    }
}

fn template_chart(name: &str) -> String {
    format!(
        r#"//! {name} — BEX chart-provider plugin
//!
//! Implement `get_charts` to return a list of available charts,
//! and `get_chart_details` to return the ranked items for a given chart.
//!
//! HTTP calls go through `ext::http`; persistent data through `ext::storage`.

// Available: use bex_core::chart::ext::http;  // for HTTP calls
use bex_core::chart::{{ChartItem, ChartSummary, Guest}};

struct Component;

impl Guest for Component {{
    /// Return all charts this plugin provides.
    fn get_charts() -> Result<Vec<ChartSummary>, String> {{
        // Example: fetch a JSON index from your service
        // let index: Vec<...> = http::get("https://example.com/charts")
        //     .json().map_err(|e| e.to_string())?;
        Ok(vec![ChartSummary {{
            id: "top-50".to_string(),
            title: "Top 50".to_string(),
            description: Some("The 50 most-played tracks this week.".to_string()),
            thumbnail: None,
        }}])
    }}

    /// Return the ranked items for chart `chart_id`.
    fn get_chart_details(chart_id: String) -> Result<Vec<ChartItem>, String> {{
        // TODO: fetch and parse the chart data
        let _ = chart_id;
        Ok(vec![])
    }}
}}

bex_core::export_chart!(Component);
"#,
        name = name
    )
}

fn template_lyrics(name: &str) -> String {
    format!(
        r#"//! {name} — BEX lyrics-provider plugin
//!
//! Implement the three methods below to power lyrics lookup in Bloomee.
//! - `get_lyrics`       — best-effort match from track metadata
//! - `search`           — full-text search by query string
//! - `get_lyrics_by_id` — direct fetch when you already have a song ID

// Available: use bex_core::lyrics::ext::http;  // for HTTP calls
use bex_core::lyrics::{{
    types::{{Lyrics, LyricsMatch, LyricsMetadata, TrackMetadata}},
    Guest,
}};

struct Component;

impl Guest for Component {{
    /// Try to find lyrics for a track using its title / artist / album / duration.
    /// Return `Ok(None)` if no match is found (not an error).
    fn get_lyrics(meta: TrackMetadata) -> Result<Option<(Lyrics, LyricsMetadata)>, String> {{
        // Example using lrclib.net:
        // let url = format!(
        //     "https://lrclib.net/api/get?artist_name={{}}&track_name={{}}",
        //     urlencoding::encode(&meta.artist),
        //     urlencoding::encode(&meta.title),
        // );
        // let resp = http::get(&url).send().map_err(|e| e.to_string())?;
        // if resp.status == 404 {{ return Ok(None); }}
        let _ = meta;
        Ok(None)
    }}

    /// Search for lyrics by a user-provided query string.
    fn search(query: String) -> Result<Vec<LyricsMatch>, String> {{
        let _ = query;
        Ok(vec![])
    }}

    /// Fetch lyrics directly by provider-specific ID.
    fn get_lyrics_by_id(id: String) -> Result<(Lyrics, LyricsMetadata), String> {{
        let _ = id;
        Err("get_lyrics_by_id not yet implemented".to_string())
    }}
}}

bex_core::export_lyrics!(Component);
"#,
        name = name
    )
}

fn template_resolver(name: &str) -> String {
    format!(
        r#"//! {name} — BEX content-resolver plugin
//!
//! A content-resolver must implement two trait objects:
//!   DiscoveryGuest  — home page sections and infinite scroll
//!   DataSourceGuest — search, album/artist/playlist pages, and streams
//!
//! All HTTP calls use `ext::http`; cache tokens with `ext::storage`.

// Available: use bex_core::resolver::ext::http;  // for HTTP calls
use bex_core::resolver::{{
    data_source::{{
        AlbumDetails, ArtistDetails, Guest as DataSourceGuest,
        PagedAlbums, PagedMediaItems, PagedTracks, PlaylistDetails,
        SearchFilter, StreamSource,
    }},
    discovery::{{Guest as DiscoveryGuest, Section}},
    types::MediaItem,
}};

struct Component;

// ── Discovery (home page) ─────────────────────────────────────────────────────

impl DiscoveryGuest for Component {{
    fn get_home_sections() -> Result<Vec<Section>, String> {{
        // Fetch and map your service's home/featured sections here
        Ok(vec![])
    }}

    fn load_more(section_id: String, page_token: String) -> Result<Vec<MediaItem>, String> {{
        let _ = (section_id, page_token);
        Ok(vec![])
    }}
}}

// ── Data source (search, streams, browse) ─────────────────────────────────────

impl DataSourceGuest for Component {{
    fn search(query: String, filter: SearchFilter, page_token: Option<String>) -> Result<PagedMediaItems, String> {{
        let _ = (query, filter, page_token);
        Ok(PagedMediaItems {{ items: vec![], next_page_token: None }})
    }}

    fn get_streams(track_id: String) -> Result<Vec<StreamSource>, String> {{
        let _ = track_id;
        Err("get_streams not yet implemented".to_string())
    }}

    fn get_segments(_track_id: String) -> Result<Vec<bex_core::resolver::types::MediaSegment>, String> {{
        Ok(vec![])
    }}

    fn get_album_details(id: String) -> Result<AlbumDetails, String> {{
        let _ = id;
        Err("get_album_details not yet implemented".to_string())
    }}

    fn more_album_tracks(id: String, page_token: String) -> Result<PagedTracks, String> {{
        let _ = (id, page_token);
        Ok(PagedTracks {{ items: vec![], next_page_token: None }})
    }}

    fn get_artist_details(id: String) -> Result<ArtistDetails, String> {{
        let _ = id;
        Err("get_artist_details not yet implemented".to_string())
    }}

    fn more_artist_albums(id: String, page_token: String) -> Result<PagedAlbums, String> {{
        let _ = (id, page_token);
        Ok(PagedAlbums {{ items: vec![], next_page_token: None }})
    }}

    fn get_playlist_details(id: String) -> Result<PlaylistDetails, String> {{
        let _ = id;
        Err("get_playlist_details not yet implemented".to_string())
    }}

    fn more_playlist_tracks(id: String, page_token: String) -> Result<PagedTracks, String> {{
        let _ = (id, page_token);
        Ok(PagedTracks {{ items: vec![], next_page_token: None }})
    }}

    fn get_radio_tracks(reference_id: String, page_token: Option<String>) -> Result<PagedTracks, String> {{
        let _ = (reference_id, page_token);
        Ok(PagedTracks {{ items: vec![], next_page_token: None }})
    }}
}}

bex_core::export_resolver!(Component);
"#,
        name = name
    )
}

fn template_suggestion(name: &str) -> String {
    format!(
        r#"//! {name} — BEX search-suggestion-provider plugin
//!
//! Two methods to implement:
//!   get_suggestions         — realtime autocomplete while the user types
//!   get_default_suggestions — shown when the search bar is focused but empty
//!
//! Return `Suggestion::Query(text)` for plain text completions, or
//! `Suggestion::Entity(...)` for rich visual cards (track/artist/album thumbnails).

// Available: use bex_core::suggestion::ext::http;  // for HTTP calls
use bex_core::suggestion::{{
    types::{{Suggestion, SuggestionOptions}},
    Guest,
}};

struct Component;

impl Guest for Component {{
    /// Called on every keystroke with the current partial query.
    fn get_suggestions(query: String, options: SuggestionOptions) -> Result<Vec<Suggestion>, String> {{
        let limit = options.limit.unwrap_or(10) as usize;

        // Example: fetch from your service's autocomplete endpoint
        // let url = format!("https://example.com/suggest?q={{}}", urlencoding::encode(&query));
        // let resp = http::get(&url).send().map_err(|e| e.to_string())?;
        // ...
        let _ = (query, limit);

        Ok(vec![
            Suggestion::Query("example suggestion 1".to_string()),
            Suggestion::Query("example suggestion 2".to_string()),
        ])
    }}

    /// Called when the search bar opens with an empty query (trending / history).
    fn get_default_suggestions(options: SuggestionOptions) -> Result<Vec<Suggestion>, String> {{
        let _ = options;
        Ok(vec![])
    }}
}}

bex_core::export_suggestion!(Component);
"#,
        name = name
    )
}

fn template_importer(name: &str) -> String {
    format!(
        r#"//! {name} — BEX content-importer plugin
//!
//! Implement the three methods below to import music collections from an
//! external service into Bloomee for fuzzy-matching.
//!
//! Methods:
//!   can_handle_url     — return true if this plugin knows how to handle a URL
//!   get_collection_info — return title, kind (playlist/album), description, owner, thumbnail
//!   get_tracks          — fetch all track metadata (title, artists, duration, etc.)
//!
//! HTTP calls go through `ext::http`;  persistent data through `ext::storage`.

// Available: use bex_core::importer::{{CollectionType, TrackItem, ext::http}};
// Available: use serde_json::Value;
use bex_core::importer::{{CollectionSummary, Guest, Tracks}};

struct Component;

impl Guest for Component {{
    fn can_handle_url(url: String) -> bool {{
        // TODO: return true for URLs your service handles
        // e.g. url.contains("your-service.com")
        let _ = url;
        false
    }}

    fn get_collection_info(url: String) -> Result<CollectionSummary, String> {{
        // Fetch and parse the collection's title, track count, thumbnail, etc.
        // Example:
        // let id = parse_id(&url).ok_or("Could not parse ID from URL")?;
        // let data: Value = http::get(&format!("https://api.example.com/playlists/{{id}}"))
        //     .json().map_err(|e| e.to_string())?;
        let _ = url;
        Err("get_collection_info not yet implemented".to_string())
    }}

    fn get_tracks(url: String) -> Result<Tracks, String> {{
        // Fetch ALL tracks from the collection (handle pagination if needed).
        // Each TrackItem should provide at minimum: title, artists, duration_ms.
        let _ = url;
        Ok(Tracks {{ items: vec![] }})
    }}
}}

bex_core::export_importer!(Component);
"#,
        name = name
    )
}
