use anyhow::Result;
use inquire::Select;
use std::collections::HashMap;
use std::path::Path;
use wasmtime::component::{Component, Linker, ResourceTable, bindgen};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

bindgen!({
    world: "chart-provider",
    path: "src/wit/chart",
});

struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    http_client: reqwest::blocking::Client,
    storage: HashMap<String, String>,
}

impl HostState {
    fn new() -> Result<Self> {
        Ok(Self {
            wasi: WasiCtxBuilder::new().inherit_stdio().build(),
            table: ResourceTable::new(),
            http_client: reqwest::blocking::Client::new(),
            storage: HashMap::new(),
        })
    }
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

impl component::chart_provider::utils::Host for HostState {
    fn http_request(
        &mut self,
        url: String,
        options: component::chart_provider::utils::RequestOptions,
    ) -> Result<component::chart_provider::utils::HttpResponse, String> {
        let method = match options.method {
            component::chart_provider::utils::HttpMethod::Get => reqwest::Method::GET,
            component::chart_provider::utils::HttpMethod::Post => reqwest::Method::POST,
            component::chart_provider::utils::HttpMethod::Put => reqwest::Method::PUT,
            component::chart_provider::utils::HttpMethod::Delete => reqwest::Method::DELETE,
            component::chart_provider::utils::HttpMethod::Head => reqwest::Method::HEAD,
            component::chart_provider::utils::HttpMethod::Patch => reqwest::Method::PATCH,
            component::chart_provider::utils::HttpMethod::Options => reqwest::Method::OPTIONS,
        };
        let mut req = self.http_client.request(method, &url);
        if let Some(t) = options.timeout_seconds {
            req = req.timeout(std::time::Duration::from_secs(t as u64));
        }
        if let Some(headers) = options.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        if let Some(body) = options.body {
            req = req.body(body);
        }
        match req.send() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
                Ok(component::chart_provider::utils::HttpResponse {
                    status,
                    headers,
                    body,
                })
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn random_number(&mut self) -> u64 {
        rand::random()
    }

    fn current_unix_timestamp(&mut self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn storage_set(&mut self, key: String, value: String) -> bool {
        self.storage.insert(key, value);
        true
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
    ChartProvider::add_to_linker(&mut linker, |s| s)?;
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;

    let component = Component::from_file(&engine, wasm_path)?;
    let mut store = Store::new(&engine, HostState::new()?);
    let (bindings, _) = ChartProvider::instantiate(&mut store, &component, &linker)?;
    println!("Plugin loaded.\n");

    loop {
        let choice = Select::new(
            "Action:",
            vec![
                "1. List charts",
                "2. Browse chart (select from list)",
                "3. Browse chart (enter ID)",
                "Exit",
            ],
        )
        .prompt()?;

        match choice {
            "1. List charts" => list_charts(&bindings, &mut store)?,
            "2. Browse chart (select from list)" => {
                let charts = match bindings
                    .component_chart_provider_chart_api()
                    .call_get_charts(&mut store)?
                {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Plugin error: {e}");
                        continue;
                    }
                };
                if charts.is_empty() {
                    println!("No charts available.");
                    continue;
                }
                let labels: Vec<String> = charts
                    .iter()
                    .map(|c| format!("{} [{}]", c.title, c.id))
                    .collect();
                let sel = Select::new("Chart:", labels).prompt()?;
                let id = sel
                    .rsplit_once('[')
                    .map(|(_, r)| r.trim_end_matches(']').trim().to_string())
                    .unwrap_or_default();
                show_chart(&bindings, &mut store, &id)?;
            }
            "3. Browse chart (enter ID)" => {
                let id = inquire::Text::new("Chart ID:").prompt()?;
                show_chart(&bindings, &mut store, &id)?;
            }
            _ => break,
        }
    }
    Ok(())
}

fn sep(label: &str) {
    let w = 72usize;
    let label = format!("  {} ", label);
    let dashes = if label.len() + 2 < w {
        w - label.len() - 2
    } else {
        2
    };
    println!("━━{}{}", label, "━".repeat(dashes));
}

fn fmt_trend(t: exports::component::chart_provider::chart_api::Trend) -> &'static str {
    use exports::component::chart_provider::chart_api::Trend;
    match t {
        Trend::Up => "↑ UP",
        Trend::Down => "↓ DOWN",
        Trend::Same => "= SAME",
        Trend::NewEntry => "✦ NEW",
        Trend::ReEntry => "↩ RE-ENTRY",
        Trend::Unknown => "? -",
    }
}

fn list_charts(bindings: &ChartProvider, store: &mut Store<HostState>) -> Result<()> {
    match bindings
        .component_chart_provider_chart_api()
        .call_get_charts(store)?
    {
        Err(e) => println!("Plugin error: {e}"),
        Ok(list) => {
            println!("\nCharts available: {}\n", list.len());
            for (i, c) in list.iter().enumerate() {
                sep(&format!("CHART #{}", i + 1));
                println!("  id          : {}", c.id);
                println!("  title       : {}", c.title);
                if let Some(d) = &c.description {
                    println!("  description : {d}");
                }
                if let Some(t) = &c.thumbnail {
                    println!("  thumbnail   : {}", t.url);
                    if let Some(lo) = &t.url_low {
                        println!("              low: {lo}");
                    }
                    if let Some(hi) = &t.url_high {
                        println!("             high: {hi}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn show_chart(bindings: &ChartProvider, store: &mut Store<HostState>, id: &str) -> Result<()> {
    println!("\nFetching chart '{id}'…");
    match bindings
        .component_chart_provider_chart_api()
        .call_get_chart_details(store, id)?
    {
        Err(e) => println!("Plugin error: {e}"),
        Ok(items) => {
            println!("  {} chart items\n", items.len());
            for item in &items {
                print_chart_item(item);
            }
        }
    }
    Ok(())
}

fn print_chart_item(item: &exports::component::chart_provider::chart_api::ChartItem) {
    use exports::component::chart_provider::types::MediaItem;
    let trend = fmt_trend(item.trend);

    sep(&format!("#{:>4}  {}", item.rank, trend));

    let change_str = item
        .change
        .map(|c| format!("{c}"))
        .unwrap_or_else(|| "–".into());
    let peak_str = item
        .peak_rank
        .map(|p| format!("#{p}"))
        .unwrap_or_else(|| "–".into());
    let weeks_str = item
        .weeks_on_chart
        .map(|w| format!("{w}"))
        .unwrap_or_else(|| "–".into());
    println!(
        "  change: {:<6}  peak: {:<6}  weeks on chart: {}",
        change_str, peak_str, weeks_str
    );

    match &item.item {
        MediaItem::Track(t) => {
            println!("  type    : track");
            println!("  id      : {}", t.id);
            let explicit = if t.is_explicit { " [EXPLICIT]" } else { "" };
            println!("  title   : {}{explicit}", t.title);
            println!("  artists : {}", t.artists);
            if let Some(alb) = &t.album {
                println!("  album   : {alb}");
            }
            if let Some(ms) = t.duration_ms {
                let s = ms / 1000;
                println!("  duration: {}:{:02} ({ms}ms)", s / 60, s % 60);
            }
            if let Some(th) = &t.thumbnail {
                println!("  thumb   : {}", th.url);
                if let Some(lo) = &th.url_low {
                    println!("            low:  {lo}");
                }
                if let Some(hi) = &th.url_high {
                    println!("            high: {hi}");
                }
            }
        }
        MediaItem::Album(a) => {
            println!("  type    : album");
            println!("  id      : {}", a.id);
            println!("  title   : {}", a.title);
            let artists = a.artists.join(", ");
            println!("  artists : {artists}");
            if let Some(y) = a.year {
                println!("  year    : {y}");
            }
            if let Some(th) = &a.thumbnail {
                println!("  thumb   : {}", th.url);
                if let Some(lo) = &th.url_low {
                    println!("            low:  {lo}");
                }
                if let Some(hi) = &th.url_high {
                    println!("            high: {hi}");
                }
            }
        }
        MediaItem::Artist(a) => {
            println!("  type    : artist");
            println!("  id      : {}", a.id);
            println!("  name    : {}", a.name);
            if let Some(th) = &a.thumbnail {
                println!("  thumb   : {}", th.url);
                if let Some(lo) = &th.url_low {
                    println!("            low:  {lo}");
                }
                if let Some(hi) = &th.url_high {
                    println!("            high: {hi}");
                }
            }
        }
    }
}
