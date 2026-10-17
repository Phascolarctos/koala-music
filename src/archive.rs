use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArchiveRecord {
    pub source_name: String,
    pub output_name: String,
    pub status: i32,
    pub file_size: String,
    pub decrypt_date: String,
    pub duration: String,
    pub error_message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArchiveBatch {
    pub id: String, // 批次唯一标识或 ID
    pub source_path: String,
    pub target_path: String,
    pub total_files: usize,     // 总文件数
    pub total_duration: String, // 💡 新增：批次总耗时（如 "1.25s" 或计算总和）
    pub total_size: String,     // 💡 新增：批次总文件大小（如 "45.20 MB"）
    pub archive_date: String,   // 💡 新增：归档日期（如 "2026-10-18 06:00:00"）
    pub records: Vec<ArchiveRecord>,
}

pub fn save_archive(batch: ArchiveBatch) -> Result<(), Box<dyn std::error::Error>> {
    let filename = "archives.json";

    // 读取现有的历史归档
    let mut batches: Vec<ArchiveBatch> = if Path::new(filename).exists() {
        let mut file = fs::File::open(filename)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    // 把新的批次放到最前面
    batches.insert(0, batch);

    // 写回文件
    let serialized = serde_json::to_string_pretty(&batches)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(filename)?;
    file.write_all(serialized.as_bytes())?;

    Ok(())
}

// 附带你原本的历史目录模块代码
const HISTORY_FILE: &str = "history_dirs.json";

pub fn load_history_dirs() -> Vec<String> {
    if Path::new(HISTORY_FILE).exists() {
        if let Ok(mut file) = fs::File::open(HISTORY_FILE) {
            let mut content = String::new();
            if file.read_to_string(&mut content).is_ok() {
                if let Ok(dirs) = serde_json::from_str::<Vec<String>>(&content) {
                    return dirs;
                }
            }
        }
    }
    Vec::new()
}

pub fn add_history_dir(new_dir: &str) {
    if new_dir.trim().is_empty() {
        return;
    }
    let mut dirs = load_history_dirs();
    dirs.retain(|d| d != new_dir);
    dirs.insert(0, new_dir.to_string());
    if dirs.len() > 10 {
        dirs.truncate(10);
    }

    if let Ok(serialized) = serde_json::to_string_pretty(&dirs) {
        let _ = fs::write(HISTORY_FILE, serialized);
    }
}

// 💡 确保这里有 pub 关键字，允许外部 main.rs 调用
pub fn load_archives() -> Vec<ArchiveBatch> {
    let filename = "archives.json";
    if Path::new(filename).exists() {
        if let Ok(mut file) = fs::File::open(filename) {
            let mut content = String::new();
            if file.read_to_string(&mut content).is_ok() {
                if let Ok(batches) = serde_json::from_str::<Vec<ArchiveBatch>>(&content) {
                    return batches;
                }
            }
        }
    }
    Vec::new()
}
