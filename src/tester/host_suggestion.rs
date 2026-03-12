use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use wasmtime::component::{bindgen, Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

bindgen!({
    world: "search-suggestion-provider",
    path: "src/wit/suggestion",
});

#[derive(Serialize, Deserialize, Default)]
struct PersistentStorage {
    entries: HashMap<String, String>,
}

impl PersistentStorage {
    fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    http_client: reqwest::blocking::Client,
    storage: HashMap<String, String>,
    storage_path: PathBuf,
}

impl HostState {
    fn new(storage_path: PathBuf) -> Result<Self> {
        let persistent = PersistentStorage::load(&storage_path);
        Ok(Self {
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
            http_client: reqwest::blocking::Client::builder()
                .cookie_store(true)
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .build()?,
            storage: persistent.entries,
            storage_path,
        })
    }
    fn persist(&self) {
        PersistentStorage { entries: self.storage.clone() }.save(&self.storage_path);
    }
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
    fn ctx(&mut self) -> &mut WasiCtx { &mut self.wasi }
}

impl component::search_suggestion_provider::utils::Host for HostState {
    fn http_request(
        &mut self,
        url: String,
        options: component::search_suggestion_provider::utils::RequestOptions,
    ) -> Result<component::search_suggestion_provider::utils::HttpResponse, String> {
        let method = match options.method {
            component::search_suggestion_provider::utils::HttpMethod::Get  => reqwest::Method::GET,
            component::search_suggestion_provider::utils::HttpMethod::Post => reqwest::Method::POST,
            component::search_suggestion_provider::utils::HttpMethod::Put  => reqwest::Method::PUT,
            component::search_suggestion_provider::utils::HttpMethod::Delete => reqwest::Method::DELETE,
            component::search_suggestion_provider::utils::HttpMethod::Head => reqwest::Method::HEAD,
            component::search_suggestion_provider::utils::HttpMethod::Patch => reqwest::Method::PATCH,
            component::search_suggestion_provider::utils::HttpMethod::Options => reqwest::Method::OPTIONS,
        };
        let mut req = self.http_client.request(method, &url);
        if let Some(t) = options.timeout_seconds {
            req = req.timeout(std::time::Duration::from_secs(t as u64));
        }
        if let Some(headers) = options.headers {
            for (k, v) in headers { req = req.header(&k, &v); }
        }
        if let Some(body) = options.body { req = req.body(body); }
        match req.send() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = resp.headers().iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
                Ok(component::search_suggestion_provider::utils::HttpResponse { status, headers, body })
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
        self.storage.insert(key, value);
        self.persist();
        true
    }

    fn storage_get(&mut self, key: String) -> Option<String> {
        self.storage.get(&key).cloned()
    }

    fn storage_delete(&mut self, key: String) -> bool {
        let existed = self.storage.remove(&key).is_some();
        if existed { self.persist(); }
        existed
    }

    fn log(&mut self, message: String) {
        println!("[PLUGIN] {message}");
    }
}

pub fn run(wasm_path: &Path) -> Result<()> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    let mut linker = Linker::<HostState>::new(&engine);
    SearchSuggestionProvider::add_to_linker(&mut linker, |s| s)?;
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;

    let stem = wasm_path.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin");
    let storage_path = PathBuf::from(format!(".bex-storage-{stem}.json"));
    let loaded = PersistentStorage::load(&storage_path).entries.len();
    println!("  Storage: {} ({loaded} cached entries)", storage_path.display());

    let component = Component::from_file(&engine, wasm_path)?;
    let mut store = Store::new(&engine, HostState::new(storage_path)?);
    let (bindings, _) = SearchSuggestionProvider::instantiate(&mut store, &component, &linker)?;
    println!("Plugin loaded.\n");

    println!("Interactive mode. Type 'help' for commands.");
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        print!("> ");
        io::stdout().flush()?;
        let line = match lines.next() {
            Some(Ok(l)) => l.trim().to_string(),
            _ => break,
        };
        if line.is_empty() { continue; }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).copied().unwrap_or("").trim();

        match cmd {
            "exit" | "quit" | "q" => break,
            "help" | "h" => {
                println!("  search <query>          — query suggestions");
                println!("  entities <query>        — entity suggestions (with thumbnails)");
                println!("  tracks <query>          — track entity suggestions");
                println!("  artists <query>         — artist entity suggestions");
                println!("  albums <query>          — album entity suggestions");
                println!("  playlists <query>       — playlist entity suggestions");
                println!("  default                 — default query suggestions");
                println!("  default-entities        — default entity suggestions");
                println!("  storage                 — show stored keys");
                println!("  storage-set <key> <val> — set a storage key (e.g. to inject API tokens)");
                println!("  storage-del <key>       — delete a storage key");
                println!("  clear                   — clear all plugin storage");
                println!("  exit/q                  — exit");
            }
            "search" if !arg.is_empty() => call_suggestions(&bindings, &mut store, arg, false, None, 10)?,
            "entities" if !arg.is_empty() => call_suggestions(&bindings, &mut store, arg, true, None, 10)?,
            "tracks" if !arg.is_empty() => call_suggestions(
                &bindings, &mut store, arg, true,
                Some(vec![exports::component::search_suggestion_provider::types::EntityType::Track]), 10)?,
            "artists" if !arg.is_empty() => call_suggestions(
                &bindings, &mut store, arg, true,
                Some(vec![exports::component::search_suggestion_provider::types::EntityType::Artist]), 10)?,
            "albums" if !arg.is_empty() => call_suggestions(
                &bindings, &mut store, arg, true,
                Some(vec![exports::component::search_suggestion_provider::types::EntityType::Album]), 10)?,
            "playlists" if !arg.is_empty() => call_suggestions(
                &bindings, &mut store, arg, true,
                Some(vec![exports::component::search_suggestion_provider::types::EntityType::Playlist]), 10)?,
            "default" => call_default_suggestions(&bindings, &mut store, false, 10)?,
            "default-entities" => call_default_suggestions(&bindings, &mut store, true, 10)?,
            "storage-set" | "sset" => {
                let mut parts2 = arg.splitn(2, ' ');
                let key = parts2.next().unwrap_or("").trim().to_string();
                let val = parts2.next().unwrap_or("").trim().to_string();
                if key.is_empty() {
                    println!("Usage: storage-set <key> <value>");
                } else {
                    store.data_mut().storage.insert(key.clone(), val.clone());
                    store.data_mut().persist();
                    let preview = if val.len() > 40 { format!("{}…", &val[..40]) } else { val.clone() };
                    println!("Stored: {key} = {preview}");
                }
            }
            "storage-del" | "sdel" => {
                let key = arg.trim().to_string();
                if key.is_empty() {
                    println!("Usage: storage-del <key>");
                } else {
                    let existed = store.data_mut().storage.remove(&key).is_some();
                    store.data_mut().persist();
                    println!("{}: {key}", if existed { "Deleted" } else { "Key not found" });
                }
            }
            "clear" => {
                store.data_mut().storage.clear();
                store.data_mut().persist();
                println!("Storage cleared.");
            }
            "storage" => {
                let keys: Vec<_> = store.data().storage.keys().cloned().collect();
                if keys.is_empty() {
                    println!("Storage is empty.");
                } else {
                    println!("Keys ({}):", keys.len());
                    let mut sorted_keys = keys.clone();
                    sorted_keys.sort();
                    for k in &sorted_keys {
                        let v = &store.data().storage[k];
                        let preview = if v.len() > 80 { format!("{}…({}B)", &v[..80], v.len()) } else { v.clone() };
                        println!("  {k}: {preview}");
                    }
                }
            }
            _ => println!("Unknown: '{line}'. Type 'help'."),
        }
    }
    Ok(())
}

fn call_suggestions(
    bindings: &SearchSuggestionProvider,
    store: &mut Store<HostState>,
    query: &str,
    include_entities: bool,
    allowed_types: Option<Vec<exports::component::search_suggestion_provider::types::EntityType>>,
    limit: u8,
) -> Result<()> {
    let opts = exports::component::search_suggestion_provider::types::SuggestionOptions {
        limit: Some(limit), include_entities, allowed_types,
    };
    match bindings.component_search_suggestion_provider_suggestion_api()
        .call_get_suggestions(store, query, &opts)? {
        Ok(list) => {
            println!("\nSuggestions for '{query}' — {} results:", list.len());
            for s in &list { print_suggestion(s); }
            if list.is_empty() { println!("  (no results)"); }
        }
        Err(e) => println!("Plugin error: {e}"),
    }
    Ok(())
}

fn call_default_suggestions(
    bindings: &SearchSuggestionProvider,
    store: &mut Store<HostState>,
    include_entities: bool,
    limit: u8,
) -> Result<()> {
    let opts = exports::component::search_suggestion_provider::types::SuggestionOptions {
        limit: Some(limit), include_entities, allowed_types: None,
    };
    match bindings.component_search_suggestion_provider_suggestion_api()
        .call_get_default_suggestions(store, &opts)? {
        Ok(list) => {
            println!("\nDefault suggestions — {} results:", list.len());
            for s in &list { print_suggestion(s); }
            if list.is_empty() { println!("  (no results)"); }
        }
        Err(e) => println!("Plugin error: {e}"),
    }
    Ok(())
}

fn print_suggestion(s: &exports::component::search_suggestion_provider::types::Suggestion) {
    use exports::component::search_suggestion_provider::types::Suggestion;
    match s {
        Suggestion::Query(q) => println!("  [  Query  ] {q}"),
        Suggestion::Entity(e) => {
            use exports::component::search_suggestion_provider::types::EntityType;
            let kind = match e.kind {
                EntityType::Track    => "TRACK   ",
                EntityType::Artist   => "ARTIST  ",
                EntityType::Album    => "ALBUM   ",
                EntityType::Playlist => "PLAYLIST",
                EntityType::Genre    => "GENRE   ",
                EntityType::Unknown  => "?       ",
            };
            let sub = e.subtitle.as_deref().map(|s| format!("  — {s}")).unwrap_or_default();
            println!("  [{kind}] [id: {}]  {}{sub}", e.id, e.title);
            if let Some(art) = &e.thumbnail {
                println!("             thumb: {}", art.url);
                if let Some(lo) = &art.url_low { println!("               low: {lo}"); }
            }
        }
    }
}
