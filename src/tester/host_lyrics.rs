use anyhow::Result;
use inquire::{Select, Text};
use std::collections::HashMap;
use std::path::Path;
use wasmtime::component::{bindgen, Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

bindgen!({
    world: "lyrics-provider",
    path: "src/wit/lyrics",
});

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    http_client: reqwest::blocking::Client,
    storage: HashMap<String, String>,
}

impl HostState {
    fn new() -> Self {
        Self {
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
            http_client: reqwest::blocking::Client::new(),
            storage: HashMap::new(),
        }
    }
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
    fn ctx(&mut self) -> &mut WasiCtx { &mut self.wasi }
}

impl component::lyrics_provider::utils::Host for HostState {
    fn http_request(
        &mut self,
        url: String,
        options: component::lyrics_provider::utils::RequestOptions,
    ) -> Result<component::lyrics_provider::utils::HttpResponse, String> {
        let method = match options.method {
            component::lyrics_provider::utils::HttpMethod::Get  => reqwest::Method::GET,
            component::lyrics_provider::utils::HttpMethod::Post => reqwest::Method::POST,
            _ => reqwest::Method::GET,
        };
        let mut req = self.http_client.request(method, &url);
        if let Some(headers) = options.headers {
            for (k, v) in headers { req = req.header(k, v); }
        }
        if let Some(body) = options.body { req = req.body(body); }
        match req.send() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = resp.headers().iter()
                    .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
                Ok(component::lyrics_provider::utils::HttpResponse { status, headers, body })
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn random_number(&mut self) -> u64 { rand::random() }

    fn current_unix_timestamp(&mut self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    }

    fn storage_set(&mut self, key: String, value: String) -> bool {
        self.storage.insert(key, value); true
    }

    fn storage_get(&mut self, key: String) -> Option<String> {
        self.storage.get(&key).cloned()
    }

    fn storage_delete(&mut self, key: String) -> bool {
        self.storage.remove(&key).is_some()
    }
}

pub fn run(wasm_path: &Path) -> Result<()> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    let mut linker = Linker::<HostState>::new(&engine);
    LyricsProvider::add_to_linker(&mut linker, |s| s)?;
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;

    let component = Component::from_file(&engine, wasm_path)?;
    let mut store = Store::new(&engine, HostState::new());
    let (bindings, _) = LyricsProvider::instantiate(&mut store, &component, &linker)?;
    println!("Plugin loaded.\n");

    loop {
        let choice = Select::new("Action:", vec![
            "1. Get lyrics by track ID",
            "2. Get lyrics by metadata",
            "3. Search lyrics",
            "Exit",
        ]).prompt()?;

        match choice {
            "1. Get lyrics by track ID" => {
                let id = Text::new("Track ID:").prompt()?;
                match bindings.component_lyrics_provider_lyrics_api()
                    .call_get_lyrics_by_id(&mut store, &id)? {
                    Ok((lyrics, meta)) => print_full_lyrics(&lyrics, &meta),
                    Err(e) => println!("Error: {e}"),
                }
            }
            "2. Get lyrics by metadata" => {
                let title  = Text::new("Title:")   .with_default("Blinding Lights").prompt()?;
                let artist = Text::new("Artist:")  .with_default("The Weeknd").prompt()?;
                let album  = Text::new("Album (optional):").with_default("").prompt()?;
                let dur    = Text::new("Duration ms (optional):").with_default("").prompt()?;
                let meta = exports::component::lyrics_provider::types::TrackMetadata {
                    title,
                    artist,
                    album:       if album.is_empty() { None } else { Some(album) },
                    duration_ms: dur.parse::<u64>().ok(),
                };
                match bindings.component_lyrics_provider_lyrics_api()
                    .call_get_lyrics(&mut store, &meta)? {
                    Ok(Some((lyrics, meta))) => print_full_lyrics(&lyrics, &meta),
                    Ok(None) => println!("No lyrics found."),
                    Err(e)   => println!("Error: {e}"),
                }
            }
            "3. Search lyrics" => {
                let q = Text::new("Query:").prompt()?;
                match bindings.component_lyrics_provider_lyrics_api()
                    .call_search(&mut store, &q)? {
                    Ok(results) => {
                        if results.is_empty() { println!("  (no results)"); continue; }
                        println!("\n{} results:\n", results.len());
                        for (i, r) in results.iter().enumerate() {
                            use exports::component::lyrics_provider::types::LyricsSyncType;
                            let sync = match r.sync_type {
                                LyricsSyncType::None      => "plain",
                                LyricsSyncType::Line      => "line-synced",
                                LyricsSyncType::Syllable  => "syllable-synced",
                            };
                            let dur = r.duration_ms.map(|d| {
                                let s = d / 1000;
                                format!(" {}:{:02}", s / 60, s % 60)
                            }).unwrap_or_default();
                            let alb = r.album.as_deref().map(|a| format!(" — {a}")).unwrap_or_default();
                            println!("  {:>2}. [{}] {} — {}{}{dur}", i + 1, r.id, r.artist, r.title, alb);
                            println!("      sync: {sync}");
                        }
                        // Let user open one
                        let labels: Vec<String> = results.iter()
                            .map(|r| format!("{} — {} [{}]", r.artist, r.title, r.id))
                            .collect();
                        let mut labels_with_back = labels;
                        labels_with_back.push("« Back".into());
                        let sel = Select::new("Open?", labels_with_back).prompt()?;
                        if sel == "« Back" { continue; }
                        if let Some(r) = results.iter().find(|r| sel.ends_with(&format!("[{}]", r.id))) {
                            match bindings.component_lyrics_provider_lyrics_api()
                                .call_get_lyrics_by_id(&mut store, &r.id)? {
                                Ok((lyrics, meta)) => print_full_lyrics(&lyrics, &meta),
                                Err(e) => println!("Error: {e}"),
                            }
                        }
                    }
                    Err(e) => println!("Error: {e}"),
                }
            }
            _ => break,
        }
    }
    Ok(())
}

fn sep(label: &str) {
    let w = 72usize;
    let label = format!("  {} ", label);
    let dashes = if label.len() + 2 < w { w - label.len() - 2 } else { 2 };
    println!("━━{}{}", label, "━".repeat(dashes));
}

fn fmt_timestamp(ms: u32) -> String {
    let total_s = ms / 1000;
    let m = total_s / 60;
    let s = total_s % 60;
    let cs = (ms % 1000) / 10; // centiseconds
    format!("{m}:{s:02}.{cs:02}")
}

fn print_full_lyrics(
    lyrics: &exports::component::lyrics_provider::types::Lyrics,
    meta:   &exports::component::lyrics_provider::types::LyricsMetadata,
) {
    use exports::component::lyrics_provider::types::LyricsSyncType;

    sep("LYRICS METADATA");
    if let Some(src)  = &meta.source    { println!("  source    : {src}"); }
    if let Some(auth) = &meta.author    { println!("  author    : {auth}"); }
    if let Some(lang) = &meta.language  { println!("  language  : {lang}"); }
    if let Some(copy) = &meta.copyright { println!("  copyright : {copy}"); }
    let verified = if meta.is_verified { "yes" } else { "no" };
    println!("  verified  : {verified}");
    let sync_label = match lyrics.sync_type {
        LyricsSyncType::None      => "none (plain)",
        LyricsSyncType::Line      => "line-synced",
        LyricsSyncType::Syllable  => "syllable-synced",
    };
    println!("  sync type : {sync_label}");
    println!("  instrumental: {}", if lyrics.is_instrumental { "yes" } else { "no" });

    if lyrics.is_instrumental {
        println!("\n  ♪ Instrumental track — no lyrics ♪");
        return;
    }

    // ── Synced lines view (preferred) ────────────────────────────────────
    if let Some(lines) = &lyrics.lines {
        sep(&format!("SYNCED LINES  ({} lines)", lines.len()));
        for line in lines {
            let ts = fmt_timestamp(line.start_ms);
            let dur_str = line.duration_ms
                .map(|d| format!("+{}ms", d))
                .unwrap_or_default();
            println!("  [{ts}]{:>8}  {}", dur_str, line.content);
            // Show syllable tokens if present
            if let Some(tokens) = &line.tokens {
                if !tokens.is_empty() {
                    let toks: Vec<String> = tokens.iter()
                        .map(|tk| format!("[+{}ms]{}", tk.offset_ms, tk.text))
                        .collect();
                    println!("    tokens: {}", toks.join(" "));
                }
            }
        }
    }

    // ── LRC (raw) ────────────────────────────────────────────────────────
    if let Some(lrc) = &lyrics.lrc {
        sep(&format!("LRC  ({} chars)", lrc.len()));
        for line in lrc.lines().take(10) {
            println!("  {line}");
        }
        let total = lrc.lines().count();
        if total > 10 {
            println!("  … {} more lines (showing 10/{total})", total - 10);
        }
    }

    // ── Plain text ────────────────────────────────────────────────────────
    if let Some(plain) = &lyrics.plain {
        sep(&format!("PLAIN TEXT  ({} chars)", plain.len()));
        for line in plain.lines() {
            println!("  {line}");
        }
    }
}
