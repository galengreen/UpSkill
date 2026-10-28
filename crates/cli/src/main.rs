mod mcp;
mod serve;

use anyhow::Result;
use serde_json::{json, Value};
use upskill_core::{Api, Upskill};

const HELP: &str = "upskill — one library for every agent's skills

USAGE
  upskill mcp                    Run the MCP server on stdio (agents control everything)
  upskill status                 Library overview
  upskill list                   List skills
  upskill health                 Health report
  upskill adopt-all              Bring every unmanaged skill into the library
  upskill deploy                 Re-place skills into agent folders
  upskill sync                   Sync with other machines
  upskill ingest                 Read agent logs for usage
  upskill call <method> [json]   Call any API method directly
";

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn api(author: &str) -> Result<Api> {
    let mut api = Api::new(Upskill::open()?, author);
    api.bin = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_string());
    Ok(api)
}

fn print(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let shortcut = |method: &str| -> Result<()> {
        let api = api("you")?;
        print(&api.call(method, &json!({}))?);
        Ok(())
    };
    match cmd {
        "mcp" => mcp::serve(api("agent")?),
        "serve" => serve::serve(api("you")?, args.get(1).and_then(|p| p.parse().ok()).unwrap_or(7777)),
        "status" => shortcut("status"),
        "list" => {
            let api = api("you")?;
            let skills = api.call("list_skills", &json!({}))?;
            for s in skills.as_array().into_iter().flatten() {
                let agents = s["agents"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(",")).unwrap_or_default();
                println!("{:<10} {:<36} {:>4} uses  [{}]", s["kind"].as_str().unwrap_or(""), s["name"].as_str().unwrap_or(""), s["usage"]["recent"], agents);
            }
            Ok(())
        }
        "health" => shortcut("health"),
        "adopt-all" => shortcut("adopt_all"),
        "deploy" => shortcut("deploy"),
        "sync" => shortcut("sync_now"),
        "ingest" => shortcut("ingest_usage"),
        "call" => {
            let method = args.get(1).ok_or_else(|| anyhow::anyhow!("usage: upskill call <method> [json]"))?;
            let params: Value = match args.get(2) {
                Some(s) => serde_json::from_str(s)?,
                None => json!({}),
            };
            print(&api("you")?.call(method, &params)?);
            Ok(())
        }
        _ => {
            print!("{HELP}");
            Ok(())
        }
    }
}
