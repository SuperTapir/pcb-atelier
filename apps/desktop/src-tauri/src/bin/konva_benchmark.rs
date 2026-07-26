use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    process::Command,
};

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RssCheckpoint {
    cycle: u32,
    rss_kib: Option<u64>,
}

fn result_path() -> Result<String, String> {
    env::var("PCB_ATELIER_BENCHMARK_RESULT")
        .map_err(|_| "PCB_ATELIER_BENCHMARK_RESULT is not set".to_owned())
}

fn current_rss_kib() -> Option<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

#[tauri::command]
fn record_benchmark_checkpoint(cycle: u32) -> Result<(), String> {
    let path = format!("{}.rss.jsonl", result_path()?);
    let checkpoint = RssCheckpoint {
        cycle,
        rss_kib: current_rss_kib(),
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut file, &checkpoint).map_err(|error| error.to_string())?;
    writeln!(file).map_err(|error| error.to_string())
}

#[tauri::command]
fn report_benchmark(app: tauri::AppHandle, result: Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?;
    fs::write(result_path()?, bytes).map_err(|error| error.to_string())?;
    app.exit(0);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            record_benchmark_checkpoint,
            report_benchmark
        ])
        .run(tauri::generate_context!("tauri.benchmark.conf.json"))
        .expect("failed to run Konva release benchmark");
}
