use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const HISTORY_FILE: &str = "history.json";

#[derive(Serialize, Deserialize, Default, Clone)]
struct HistoryData {
    sources: Vec<String>,
    targets: Vec<String>,
}

// 内部通用加载函数
fn load_history() -> HistoryData {
    if Path::new(HISTORY_FILE).exists() {
        if let Ok(content) = fs::read_to_string(HISTORY_FILE) {
            if let Ok(data) = serde_json::from_str::<HistoryData>(&content) {
                return data;
            }
        }
    }
    HistoryData::default()
}

// 内部通用保存函数
fn save_history(data: &HistoryData) {
    if let Ok(serialized) = serde_json::to_string_pretty(data) {
        let _ = fs::write(HISTORY_FILE, serialized);
    }
}

// 1. 原始文件目录（Source）相关接口
pub fn load_history_sources() -> Vec<String> {
    load_history().sources
}

pub fn add_history_source(new_dir: &str) {
    if new_dir.trim().is_empty() {
        return;
    }
    let mut data = load_history();
    data.sources.retain(|d| d != new_dir);
    data.sources.insert(0, new_dir.to_string());
    if data.sources.len() > 10 {
        data.sources.truncate(10);
    }
    save_history(&data);
}

pub fn remove_history_source(target_dir: &str) {
    let mut data = load_history();
    data.sources.retain(|d| d != target_dir);
    save_history(&data);
}

// 2. 目标文件目录（Target）相关接口
pub fn load_history_targets() -> Vec<String> {
    load_history().targets
}

pub fn add_history_target(new_dir: &str) {
    if new_dir.trim().is_empty() {
        return;
    }
    let mut data = load_history();
    data.targets.retain(|d| d != new_dir);
    data.targets.insert(0, new_dir.to_string());
    if data.targets.len() > 10 {
        data.targets.truncate(10);
    }
    save_history(&data);
}

pub fn remove_history_target(target_dir: &str) {
    let mut data = load_history();
    data.targets.retain(|d| d != target_dir);
    save_history(&data);
}
