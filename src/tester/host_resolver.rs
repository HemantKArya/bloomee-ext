use anyhow::Result;
use inquire::{Select, Text};
use std::collections::HashMap;
use std::path::Path;
use wasmtime::component::{bindgen, Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

bindgen!({
    world: "content-resolver",
    path: "src/wit/resolver",
});

#[derive(Clone)]
struct CachedHttpResponse {
    expires_at: u64,
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    http_client: reqwest::blocking::Client,
    storage: HashMap<String, String>,
    http_cache: HashMap<String, CachedHttpResponse>,
    http_cache_ttl_secs: u64,
}

impl HostState {
    fn new() -> Result<Self> {
        Ok(Self {
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
            http_client: reqwest::blocking::Client::new(),
            storage: HashMap::new(),
            http_cache: HashMap::new(),
            http_cache_ttl_secs: 120,
        })
    }
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
    fn ctx(&mut self) -> &mut WasiCtx { &mut self.wasi }
}

impl component::content_resolver::utils::Host for HostState {
    fn http_request(
        &mut self,
        url: String,
        options: component::content_resolver::utils::RequestOptions,
    ) -> Result<component::content_resolver::utils::HttpResponse, String> {
        let method = match options.method {
            component::content_resolver::utils::HttpMethod::Get    => reqwest::Method::GET,
            component::content_resolver::utils::HttpMethod::Post   => reqwest::Method::POST,
            component::content_resolver::utils::HttpMethod::Put    => reqwest::Method::PUT,
            component::content_resolver::utils::HttpMethod::Delete => reqwest::Method::DELETE,
            component::content_resolver::utils::HttpMethod::Head   => reqwest::Method::HEAD,
            component::content_resolver::utils::HttpMethod::Patch  => reqwest::Method::PATCH,
            component::content_resolver::utils::HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        let now = self.current_unix_timestamp();
        let cacheable = method == reqwest::Method::GET;
        let cache_key = if cacheable {
            let mut parts: Vec<String> = options.headers.as_ref()
                .map(|h| h.iter().map(|(k, v)| format!("{}:{}", k.to_ascii_lowercase(), v)).collect())
                .unwrap_or_default();
            parts.sort();
            Some(format!("GET|{}|{}", url, parts.join("|")))
        } else { None };

        if let Some(ref key) = cache_key {
            if let Some(hit) = self.http_cache.get(key) {
                if now <= hit.expires_at {
                    return Ok(component::content_resolver::utils::HttpResponse {
                        status: hit.status, headers: hit.headers.clone(), body: hit.body.clone(),
                    });
                }
            }
        }

        let mut req = self.http_client.request(method, &url);
        if let Some(t) = options.timeout_seconds {
            req = req.timeout(std::time::Duration::from_secs(t as u64));
        }
        if let Some(headers) = options.headers {
            for (k, v) in headers { req = req.header(k, v); }
        }
        if let Some(body) = options.body { req = req.body(body); }

        match req.send() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers: Vec<(String, String)> = resp.headers().iter()
                    .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
                if let Some(key) = cache_key {
                    if status >= 200 && status < 300 {
                        self.http_cache.insert(key, CachedHttpResponse {
                            expires_at: now + self.http_cache_ttl_secs,
                            status, headers: headers.clone(), body: body.clone(),
                        });
                    }
                }
                Ok(component::content_resolver::utils::HttpResponse { status, headers, body })
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

// ── Formatting helpers ────────────────────────────────────────────────────────

fn sep(label: &str) {
    let w = 72usize;
    let label = format!("  {} ", label);
    let dashes = if label.len() + 2 < w { w - label.len() - 2 } else { 2 };
    println!("━━{}{}", label, "━".repeat(dashes));
}

fn fmt_duration(ms: u64) -> String {
    let secs = ms / 1000;
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02} ({ms}ms)")
}

fn fmt_expires(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    if ts >= now {
        let diff = ts - now;
        let h = diff / 3600;
        let m = (diff % 3600) / 60;
        let s = diff % 60;
        if h > 0 { format!("{ts}  (in {h}h{m:02}m)") }
        else if m > 0 { format!("{ts}  (in {m}m{s:02}s)") }
        else { format!("{ts}  (in {s}s)") }
    } else {
        format!("{ts}  (EXPIRED)")
    }
}

fn fmt_layout(l: &exports::component::content_resolver::types::ImageLayout) -> &'static str {
    use exports::component::content_resolver::types::ImageLayout;
    match l {
        ImageLayout::Square    => "square",
        ImageLayout::Portrait  => "portrait",
        ImageLayout::Landscape => "landscape",
        ImageLayout::Banner    => "banner",
        ImageLayout::Circular  => "circular",
    }
}

fn fmt_segment_kind(k: &exports::component::content_resolver::types::SegmentKind) -> &'static str {
    use exports::component::content_resolver::types::SegmentKind;
    match k {
        SegmentKind::Chapter     => "chapter",
        SegmentKind::Sponsor     => "sponsor",
        SegmentKind::Intro       => "intro",
        SegmentKind::Outro       => "outro",
        SegmentKind::Interaction => "interaction",
        SegmentKind::Silence     => "silence",
    }
}

fn print_segment(s: &exports::component::content_resolver::types::MediaSegment, idx: usize, total: usize) {
    sep(&format!("SEGMENT {idx}/{total}"));
    let start_s = s.start_ms / 1000;
    let end_s   = s.end_ms   / 1000;
    println!("  kind     : {}", fmt_segment_kind(&s.kind));
    println!("  start    : {}:{:02} ({}ms)", start_s / 60, start_s % 60, s.start_ms);
    println!("  end      : {}:{:02} ({}ms)", end_s   / 60, end_s   % 60, s.end_ms);
    if let Some(t) = &s.title { println!("  title    : {t}"); }
}

fn fmt_quality(q: &exports::component::content_resolver::data_source::Quality) -> &'static str {
    use exports::component::content_resolver::data_source::Quality;
    match q {
        Quality::Low      => "Low",
        Quality::Medium   => "Medium",
        Quality::High     => "High",
        Quality::Lossless => "Lossless",
    }
}

fn print_artwork(art: &exports::component::content_resolver::types::Artwork, indent: &str) {
    println!("{}thumb    : {} [{}]", indent, art.url, fmt_layout(&art.layout));
    match (&art.url_low, &art.url_high) {
        (None, None) => {}
        (lo, hi) => {
            let low  = lo.as_deref().unwrap_or("(none)");
            let high = hi.as_deref().unwrap_or("(none)");
            println!("{}           low: {low}", indent);
            println!("{}          high: {high}", indent);
        }
    }
}

fn print_track(t: &exports::component::content_resolver::types::Track, idx: Option<usize>) {
    let label = if let Some(i) = idx { format!("TRACK #{i}") } else { "TRACK".to_string() };
    sep(&label);
    let explicit = if t.is_explicit { " [EXPLICIT]" } else { "" };
    println!("  id       : {}", t.id);
    println!("  title    : {}{explicit}", t.title);
    let dur = t.duration_ms.map(fmt_duration).unwrap_or_else(|| "(unknown)".into());
    println!("  duration : {dur}");
    if let Some(u) = &t.url { println!("  url      : {u}"); }
    let artists: Vec<String> = t.artists.iter()
        .map(|a| format!("{} [id:{}]", a.name, a.id))
        .collect();
    println!("  artists  : {}", artists.join(", "));
    if let Some(alb) = &t.album {
        let year = alb.year.map(|y| format!(" ({y})")).unwrap_or_default();
        let alb_artists: Vec<&str> = alb.artists.iter().map(|a| a.name.as_str()).collect();
        println!("  album    : {}{year} — {} [id:{}]", alb.title, alb_artists.join(", "), alb.id);
        if let Some(sub) = &alb.subtitle { println!("             {sub}"); }
    }
    print_artwork(&t.thumbnail, "  ");
    if let Some(lyr) = &t.lyrics {
        if let Some(plain) = &lyr.plain { println!("  lyrics   : {} chars (plain)", plain.len()); }
        if let Some(synced) = &lyr.synced { println!("  lyrics   : {} chars (synced)", synced.len()); }
        if let Some(copy) = &lyr.copyright { println!("  lyr-copy : {copy}"); }
    }
}

fn print_album_summary(a: &exports::component::content_resolver::types::AlbumSummary, indent: &str) {
    let year = a.year.map(|y| format!(" ({y})")).unwrap_or_default();
    let artists: Vec<&str> = a.artists.iter().map(|x| x.name.as_str()).collect();
    println!("{indent}id       : {}", a.id);
    println!("{indent}title    : {}{year} — {}", a.title, artists.join(", "));
    if let Some(sub) = &a.subtitle { println!("{indent}subtitle : {sub}"); }
    if let Some(u) = &a.url { println!("{indent}url      : {u}"); }
    if let Some(art) = &a.thumbnail { print_artwork(art, indent); }
}

fn print_artist_summary(a: &exports::component::content_resolver::types::ArtistSummary, indent: &str) {
    println!("{indent}id       : {}", a.id);
    println!("{indent}name     : {}", a.name);
    if let Some(sub) = &a.subtitle { println!("{indent}subtitle : {sub}"); }
    if let Some(u) = &a.url { println!("{indent}url      : {u}"); }
    if let Some(art) = &a.thumbnail { print_artwork(art, indent); }
}

fn print_stream(s: &exports::component::content_resolver::data_source::StreamSource, idx: usize, total: usize) {
    sep(&format!("STREAM {idx}/{total}"));
    println!("  quality  : {}", fmt_quality(&s.quality));
    println!("  format   : {}", s.format);
    println!("  url      : {}", s.url);
    if let Some(exp) = s.expires_at { println!("  expires  : {}", fmt_expires(exp)); }
    match &s.headers {
        Some(h) if !h.is_empty() => {
            println!("  headers  :");
            for (k, v) in h { println!("    {k}: {v}"); }
        }
        _ => println!("  headers  : (none)"),
    }
}

fn print_section(s: &exports::component::content_resolver::discovery::Section, idx: usize, total: usize) {
    use exports::component::content_resolver::discovery::SectionType;
    let card = match s.card_type {
        SectionType::Carousel => "carousel",
        SectionType::Grid     => "grid",
        SectionType::Vlist    => "vlist",
    };
    sep(&format!("SECTION {idx}/{total}"));
    println!("  id       : {}", s.id);
    println!("  title    : {}", s.title);
    if let Some(sub) = &s.subtitle { println!("  subtitle : {sub}"); }
    println!("  layout   : {card}  ({} items)", s.items.len());
    if let Some(ml) = &s.more_link { println!("  more_link: {ml}"); }
}

fn print_media_item(item: &exports::component::content_resolver::types::MediaItem, idx: Option<usize>) {
    use exports::component::content_resolver::types::MediaItem;
    match item {
        MediaItem::Track(t)    => print_track(t, idx),
        MediaItem::Album(a)    => {
            let label = if let Some(i) = idx { format!("ALBUM #{i}") } else { "ALBUM".to_string() };
            sep(&label);
            print_album_summary(a, "  ");
        }
        MediaItem::Artist(a)   => {
            let label = if let Some(i) = idx { format!("ARTIST #{i}") } else { "ARTIST".to_string() };
            sep(&label);
            print_artist_summary(a, "  ");
        }
        MediaItem::Playlist(p) => {
            let label = if let Some(i) = idx { format!("PLAYLIST #{i}") } else { "PLAYLIST".to_string() };
            sep(&label);
            println!("  id       : {}", p.id);
            println!("  title    : {}", p.title);
            if let Some(o) = &p.owner { println!("  owner    : {o}"); }
            if let Some(u) = &p.url { println!("  url      : {u}"); }
            print_artwork(&p.thumbnail, "  ");
        }
    }
}

// ── Main interactive loop ─────────────────────────────────────────────────────

pub fn run(wasm_path: &Path) -> Result<()> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    let mut linker = Linker::<HostState>::new(&engine);
    ContentResolver::add_to_linker(&mut linker, |s| s)?;
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;

    let component = Component::from_file(&engine, wasm_path)?;
    let mut store = Store::new(&engine, HostState::new()?);
    let (bindings, _) = ContentResolver::instantiate(&mut store, &component, &linker)?;
    println!("Plugin loaded.\n");

    loop {
        let choice = Select::new("Action:", vec![
            "1. Home sections",
            "2. Search",
            "3. Album details",
            "4. Artist details",
            "5. Playlist details",
            "6. Get streams (track ID)",
            "7. Radio tracks",
            "8. Get segments (track ID)",
            "Exit",
        ]).prompt()?;

        match choice {
            "1. Home sections" => cmd_home(&bindings, &mut store)?,
            "2. Search"        => cmd_search(&bindings, &mut store)?,
            "3. Album details" => cmd_album(&bindings, &mut store)?,
            "4. Artist details"=> cmd_artist(&bindings, &mut store)?,
            "5. Playlist details" => cmd_playlist(&bindings, &mut store)?,
            "6. Get streams (track ID)" => cmd_streams(&bindings, &mut store)?,
            "7. Radio tracks"  => cmd_radio(&bindings, &mut store)?,
            "8. Get segments (track ID)" => cmd_segments(&bindings, &mut store)?,
            _ => break,
        }
    }
    Ok(())
}

fn cmd_home(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    match bindings.component_content_resolver_discovery()
        .call_get_home_sections(&mut *store)? {
        Err(e) => println!("Plugin error: {e}"),
        Ok(sections) => {
            println!("\nHome sections: {}", sections.len());
            for (i, s) in sections.iter().enumerate() {
                print_section(s, i + 1, sections.len());
                for (j, item) in s.items.iter().enumerate() {
                    print_media_item(item, Some(j + 1));
                }
                // Offer to load more if a more_link exists
                if let Some(tok) = &s.more_link {
                    let ans = Select::new(
                        &format!("Load more for '{}'?", s.title),
                        vec!["Yes", "Skip"],
                    ).prompt()?;
                    if ans == "Yes" {
                        match bindings.component_content_resolver_discovery()
                            .call_load_more(&mut *store, &s.id, tok)? {
                            Ok(items) => {
                                println!("  + {} more items:", items.len());
                                for (j, item) in items.iter().enumerate() {
                                    print_media_item(item, Some(s.items.len() + j + 1));
                                }
                            }
                            Err(e) => println!("  Plugin error: {e}"),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_search(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let query = Text::new("Query:").prompt()?;
    let filter_str = Select::new("Filter:", vec!["All", "Songs", "Albums", "Artists", "Playlists"]).prompt()?;
    use exports::component::content_resolver::data_source::SearchFilter;
    let filter = match filter_str {
        "Songs"     => SearchFilter::Track,
        "Albums"    => SearchFilter::Album,
        "Artists"   => SearchFilter::Artist,
        "Playlists" => SearchFilter::Playlist,
        _           => SearchFilter::All,
    };
    let mut token: Option<String> = None;
    let mut page = 1usize;
    loop {
        match bindings.component_content_resolver_data_source()
            .call_search(&mut *store, &query, filter, token.as_deref())? {
            Err(e) => { println!("Plugin error: {e}"); break; }
            Ok(paged) => {
                println!("\nPage {page} — {} results:", paged.items.len());
                for (i, item) in paged.items.iter().enumerate() {
                    print_media_item(item, Some(i + 1));
                }
                token = paged.next_page_token;
                if token.is_none() { println!("  (end of results)"); break; }
                let next = Select::new("", vec!["Next Page", "Back to Menu"]).prompt()?;
                if next == "Back to Menu" { break; }
                page += 1;
            }
        }
    }
    Ok(())
}

fn cmd_album(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let id = Text::new("Album ID:").prompt()?;
    match bindings.component_content_resolver_data_source()
        .call_get_album_details(&mut *store, &id)? {
        Err(e) => println!("Plugin error: {e}"),
        Ok(d) => {
            sep("ALBUM DETAILS");
            print_album_summary(&d.summary, "  ");
            if let Some(desc) = &d.description { println!("  desc     : {desc}"); }
            println!();
            println!("  Tracks: {}", d.tracks.items.len());
            for (i, t) in d.tracks.items.iter().enumerate() {
                print_track(t, Some(i + 1));
            }
            // Pagination
            let mut token = d.tracks.next_page_token;
            while let Some(tok) = token {
                println!("  (page token: {tok})");
                let ans = Select::new("Load more tracks?", vec!["Yes", "Stop"]).prompt()?;
                if ans == "Stop" { break; }
                match bindings.component_content_resolver_data_source()
                    .call_more_album_tracks(&mut *store, &id, &tok)? {
                    Err(e) => { println!("Plugin error: {e}"); break; }
                    Ok(p) => {
                        for (i, t) in p.items.iter().enumerate() { print_track(t, Some(i + 1)); }
                        token = p.next_page_token;
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_artist(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let id = Text::new("Artist ID:").prompt()?;
    match bindings.component_content_resolver_data_source()
        .call_get_artist_details(&mut *store, &id)? {
        Err(e) => println!("Plugin error: {e}"),
        Ok(d) => {
            sep("ARTIST DETAILS");
            print_artist_summary(&d.summary, "  ");
            if let Some(desc) = &d.description { println!("  desc     : {desc}"); }

            println!("\n  Top tracks: {}", d.top_tracks.len());
            for (i, t) in d.top_tracks.iter().enumerate() { print_track(t, Some(i + 1)); }

            println!("\n  Albums: {}", d.albums.items.len());
            for (i, a) in d.albums.items.iter().enumerate() {
                sep(&format!("ALBUM #{}", i + 1));
                print_album_summary(a, "  ");
            }
            // More albums pagination
            let mut alb_tok = d.albums.next_page_token;
            while let Some(tok) = alb_tok {
                let ans = Select::new("Load more albums?", vec!["Yes", "Stop"]).prompt()?;
                if ans == "Stop" { break; }
                match bindings.component_content_resolver_data_source()
                    .call_more_artist_albums(&mut *store, &id, &tok)? {
                    Err(e) => { println!("Plugin error: {e}"); break; }
                    Ok(p) => {
                        for (i, a) in p.items.iter().enumerate() {
                            sep(&format!("ALBUM #{}", d.albums.items.len() + i + 1));
                            print_album_summary(a, "  ");
                        }
                        alb_tok = p.next_page_token;
                    }
                }
            }

            if !d.related_artists.is_empty() {
                println!("\n  Related artists: {}", d.related_artists.len());
                for (i, a) in d.related_artists.iter().enumerate() {
                    sep(&format!("ARTIST #{}", i + 1));
                    print_artist_summary(a, "  ");
                }
            }
        }
    }
    Ok(())
}

fn cmd_playlist(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let id = Text::new("Playlist ID:").prompt()?;
    match bindings.component_content_resolver_data_source()
        .call_get_playlist_details(&mut *store, &id)? {
        Err(e) => println!("Plugin error: {e}"),
        Ok(d) => {
            sep("PLAYLIST DETAILS");
            println!("  id       : {}", d.summary.id);
            println!("  title    : {}", d.summary.title);
            if let Some(o) = &d.summary.owner { println!("  owner    : {o}"); }
            if let Some(u) = &d.summary.url { println!("  url      : {u}"); }
            print_artwork(&d.summary.thumbnail, "  ");
            if let Some(desc) = &d.description { println!("  desc     : {desc}"); }
            println!("\n  Tracks: {}", d.tracks.items.len());
            for (i, t) in d.tracks.items.iter().enumerate() { print_track(t, Some(i + 1)); }
            let mut tok = d.tracks.next_page_token;
            while let Some(t) = tok {
                let ans = Select::new("Load more tracks?", vec!["Yes", "Stop"]).prompt()?;
                if ans == "Stop" { break; }
                match bindings.component_content_resolver_data_source()
                    .call_more_playlist_tracks(&mut *store, &id, &t)? {
                    Err(e) => { println!("Plugin error: {e}"); break; }
                    Ok(p) => {
                        for (i, t) in p.items.iter().enumerate() { print_track(t, Some(i + 1)); }
                        tok = p.next_page_token;
                    }
                }
            }
        }
    }
    Ok(())
}

fn cmd_streams(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let id = Text::new("Track ID:").prompt()?;
    match bindings.component_content_resolver_data_source()
        .call_get_streams(&mut *store, &id)? {
        Err(e) => println!("Plugin error: {e}"),
        Ok(streams) => {
            println!("\n{} stream(s) for track '{id}':", streams.len());
            for (i, s) in streams.iter().enumerate() {
                print_stream(s, i + 1, streams.len());
            }
        }
    }
    Ok(())
}

fn cmd_segments(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let id = Text::new("Track ID:").prompt()?;
    match bindings.component_content_resolver_data_source()
        .call_get_segments(&mut *store, &id)? {
        Err(e) => println!("Plugin error: {e}"),
        Ok(segs) => {
            if segs.is_empty() {
                println!("  (no segments returned)");
            } else {
                println!("\n{} segment(s) for track '{id}':", segs.len());
                for (i, s) in segs.iter().enumerate() {
                    print_segment(s, i + 1, segs.len());
                }
            }
        }
    }
    Ok(())
}

fn cmd_radio(bindings: &ContentResolver, store: &mut Store<HostState>) -> Result<()> {
    let id = Text::new("Reference ID:").prompt()?;
    let mut tok: Option<String> = None;
    let mut page = 1usize;
    loop {
        match bindings.component_content_resolver_data_source()
            .call_get_radio_tracks(&mut *store, &id, tok.as_deref())? {
        Err(e) => { println!("Plugin error: {e}"); break; }
        Ok(paged) => {
            println!("\nRadio page {page} — {} tracks:", paged.items.len());
            for (i, t) in paged.items.iter().enumerate() { print_track(t, Some(i + 1)); }
            tok = paged.next_page_token;
            if tok.is_none() { println!("  (end)"); break; }
            let ans = Select::new("", vec!["Next Page", "Stop"]).prompt()?;
            if ans == "Stop" { break; }
            page += 1;
        }}
    }
    Ok(())
}
