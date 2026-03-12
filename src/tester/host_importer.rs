use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use wasmtime::component::{bindgen, Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

bindgen!({
    world: "content-importer",
    path: "src/wit/importer",
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
                .timeout(std::time::Duration::from_secs(45))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36")
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

impl component::content_importer::utils::Host for HostState {
    fn http_request(
        &mut self,
        url: String,
        options: component::content_importer::utils::RequestOptions,
    ) -> Result<component::content_importer::utils::HttpResponse, String> {
        let method = match options.method {
            component::content_importer::utils::HttpMethod::Get     => reqwest::Method::GET,
            component::content_importer::utils::HttpMethod::Post    => reqwest::Method::POST,
            component::content_importer::utils::HttpMethod::Put     => reqwest::Method::PUT,
            component::content_importer::utils::HttpMethod::Delete  => reqwest::Method::DELETE,
            component::content_importer::utils::HttpMethod::Head    => reqwest::Method::HEAD,
            component::content_importer::utils::HttpMethod::Patch   => reqwest::Method::PATCH,
            component::content_importer::utils::HttpMethod::Options => reqwest::Method::OPTIONS,
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
                Ok(component::content_importer::utils::HttpResponse { status, headers, body })
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
    ContentImporter::add_to_linker(&mut linker, |s| s)?;
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;

    let stem = wasm_path.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin");
    let storage_path = PathBuf::from(format!(".bex-storage-{stem}.json"));
    let loaded = PersistentStorage::load(&storage_path).entries.len();
    println!("  Storage: {} ({loaded} cached entries)", storage_path.display());

    let component = Component::from_file(&engine, wasm_path)?;
    let mut store = Store::new(&engine, HostState::new(storage_path)?);
    let (bindings, _) = ContentImporter::instantiate(&mut store, &component, &linker)?;
    println!("Plugin loaded.\n");

    println!("Interactive mode. Type 'help' for commands.");
    let stdin = io::stdin();

    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;  // EOF
        }
        let line = line.trim().to_string();
        if line.is_empty() { continue; }

        let (cmd, rest) = if let Some(idx) = line.find(' ') {
            (&line[..idx], line[idx + 1..].trim())
        } else {
            (line.as_str(), "")
        };

        match cmd {
            "exit" | "quit" | "q" => break,

            "help" | "h" | "?" => {
                println!("Commands:");
                println!("  check  <url>  — test if plugin can handle the URL");
                println!("  info   <url>  — get collection summary (title, track count, etc.)");
                println!("  tracks <url>  — fetch all track metadata");
                println!("  import <url>  — info + tracks combined (full import)");
                println!("  storage-set <key> <val>  — set a storage key (alias: sset)");
                println!("  storage-del <key>        — delete a storage key (alias: sdel)");
                println!("  storage                  — show all storage entries");
                println!("  clear                    — clear all storage");
                println!("  exit                     — quit");
            }

            "storage" => {
                let s = &store.data().storage;
                if s.is_empty() {
                    println!("Storage is empty.");
                } else {
                    let mut keys: Vec<&String> = s.keys().collect();
                    keys.sort();
                    println!("Keys ({}):", keys.len());
                    for k in keys {
                        let v = &s[k];
                        let disp = if v.len() > 80 { format!("{}…({}B)", &v[..80], v.len()) } else { v.clone() };
                        println!("  {k}: {disp}");
                    }
                }
            }

            "clear" => {
                store.data_mut().storage.clear();
                store.data_mut().persist();
                println!("Storage cleared.");
            }

            "storage-set" | "sset" => {
                let mut parts = rest.splitn(2, ' ');
                let key = parts.next().unwrap_or("").trim();
                let val = parts.next().unwrap_or("").trim();
                if key.is_empty() {
                    println!("Usage: storage-set <key> <value>");
                } else {
                    store.data_mut().storage.insert(key.to_string(), val.to_string());
                    store.data_mut().persist();
                    println!("Stored: {key} = {}…", &val[..val.len().min(50)]);
                }
            }

            "storage-del" | "sdel" => {
                let key = rest.trim();
                if key.is_empty() {
                    println!("Usage: storage-del <key>");
                } else {
                    let existed = store.data_mut().storage.remove(key).is_some();
                    if existed { store.data_mut().persist(); println!("Deleted: {key}"); }
                    else { println!("Key not found: {key}"); }
                }
            }

            "check" => {
                if rest.is_empty() { println!("Usage: check <url>"); continue; }
                match bindings.component_content_importer_importer()
                    .call_can_handle_url(&mut store, rest)
                {
                    Ok(true)  => println!("✓ Plugin CAN handle this URL"),
                    Ok(false) => println!("✗ Plugin cannot handle this URL"),
                    Err(e)    => println!("Error: {e}"),
                }
            }

            "info" => {
                if rest.is_empty() { println!("Usage: info <url>"); continue; }
                match bindings.component_content_importer_importer()
                    .call_get_collection_info(&mut store, rest)
                {
                    Ok(Ok(info)) => print_collection_info(&info),
                    Ok(Err(e))   => println!("Plugin error: {e}"),
                    Err(e)       => println!("Host error: {e}"),
                }
            }

            "tracks" => {
                if rest.is_empty() { println!("Usage: tracks <url>"); continue; }
                match bindings.component_content_importer_importer()
                    .call_get_tracks(&mut store, rest)
                {
                    Ok(Ok(tracks)) => print_tracks(&tracks),
                    Ok(Err(e))     => println!("Plugin error: {e}"),
                    Err(e)         => println!("Host error: {e}"),
                }
            }

            "import" => {
                if rest.is_empty() { println!("Usage: import <url>"); continue; }
                // Full import: info + tracks
                match bindings.component_content_importer_importer()
                    .call_get_collection_info(&mut store, rest)
                {
                    Ok(Ok(info)) => print_collection_info(&info),
                    Ok(Err(e))   => { println!("Info error: {e}"); continue; }
                    Err(e)       => { println!("Host error: {e}"); continue; }
                }
                match bindings.component_content_importer_importer()
                    .call_get_tracks(&mut store, rest)
                {
                    Ok(Ok(tracks)) => print_tracks(&tracks),
                    Ok(Err(e))     => println!("Tracks error: {e}"),
                    Err(e)         => println!("Host error: {e}"),
                }
            }

            _ => println!("Unknown command: {cmd}. Type 'help' for commands."),
        }
    }
    println!("Goodbye!");
    Ok(())
}

// ── Formatting ────────────────────────────────────────────────────────────────

fn sep(label: &str) {
    let w = 72usize;
    let label = format!("  {} ", label);
    let dashes = if label.len() + 2 < w { w - label.len() - 2 } else { 2 };
    println!("━━{}{}", label, "━".repeat(dashes));
}

fn print_collection_info(info: &exports::component::content_importer::types::CollectionSummary) {
    use exports::component::content_importer::types::CollectionType;
    sep("COLLECTION INFO");
    println!("  title       : {}", info.title);
    println!("  kind        : {}", match info.kind { CollectionType::Playlist => "playlist", CollectionType::Album => "album" });
    if let Some(ref d) = info.description { println!("  description : {}", d.chars().take(120).collect::<String>()); }
    if let Some(ref o) = info.owner        { println!("  owner       : {o}"); }
    if let Some(ref t) = info.thumbnail_url { println!("  thumbnail   : {}", t.chars().take(80).collect::<String>()); }
    if let Some(n) = info.track_count      { println!("  tracks      : {n}"); }
}

fn print_tracks(tracks: &exports::component::content_importer::types::Tracks) {
    let items = &tracks.items;
    sep(&format!("TRACKS — {} total", items.len()));
    for (i, t) in items.iter().enumerate().take(30) {
        let dur = t.duration_ms.map(|ms| {
            let s = ms / 1000;
            format!("{}:{:02}", s / 60, s % 60)
        }).unwrap_or_default();
        let artists = t.artists.join(", ");
        let expl = if t.is_explicit == Some(true) { " [E]" } else { "" };
        println!("  {:>3}. {}{} — {} ({})", i + 1, t.title, expl, artists, dur);
        if let Some(ref id) = t.source_id { println!("       id: {id}"); }
    }
    if items.len() > 30 {
        println!("  ... and {} more tracks", items.len() - 30);
    }
}
