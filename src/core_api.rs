//! Rust 解密内核导出接口（UniFFI / Kotlin 绑定）。
//! 对应 UDL 定义见 src/koala.udl；桌面 Slint 端不使用本模块。

use crate::app::{format_duration, format_file_size, get_audio_duration};
use chrono::{DateTime, Local};
use ncmmiao::Ncmfile;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

/// 扫描源目录中的一条待处理记录（trait 实现由 UDL scaffolding 自动生成）
#[derive(Clone)]
pub struct FileEntry {
    pub index: u32,
    pub source_path: String,
    pub source_name: String,
    pub output_name: String,
    pub file_size: String,
    pub already_exists: bool,
    pub music_length: String,
    pub decrypt_date: String,
}

/// 单个文件的解密结果（trait 实现由 UDL scaffolding 自动生成）
#[derive(Clone)]
pub struct DecryptOutcome {
    pub success: bool,
    pub error_message: String,
    pub output_name: String,
    pub duration_text: String,
}

/// 扫描源目录（目录或单个 ncm 文件均可），返回待解密文件列表，
/// 并检查目标目录中是否已存在同名输出（.flac/.mp3）。
pub fn scan_ncm_folder(source: String, target: String) -> Vec<FileEntry> {
    let files = ncmmiao::pathparse::pathparse(vec![source]);
    files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .to_string();
            let file_size = std::fs::metadata(path)
                .map(|m| format_file_size(m.len()))
                .unwrap_or_else(|_| "-".into());
            let stem = Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name);
            let flac = Path::new(&target).join(format!("{}.flac", stem));
            let mp3 = Path::new(&target).join(format!("{}.mp3", stem));
            let mp3_exists = mp3.exists();
            let flac_exists = flac.exists();
            let already_exists = !target.trim().is_empty() && (flac_exists || mp3_exists);
            let ext = if mp3_exists { ".mp3" } else { ".flac" };
            let out = format!("{}{}", stem, ext);
            let exist_path = if mp3_exists { mp3 } else { flac };

            let (music_length, decrypt_date) = if already_exists {
                (
                    get_audio_duration(&exist_path),
                    std::fs::metadata(&exist_path)
                        .and_then(|m| m.modified())
                        .map(|t| {
                            let dt: DateTime<Local> = t.into();
                            dt.format("%Y-%m-%d %H:%M").to_string()
                        })
                        .unwrap_or_default(),
                )
            } else {
                ("-".to_string(), String::new())
            };

            FileEntry {
                index: i as u32,
                source_path: path.clone(),
                source_name: name,
                output_name: out,
                file_size,
                already_exists,
                music_length,
                decrypt_date,
            }
        })
        .collect()
}

/// 解密单个 ncm 文件到 output_dir，输出 .flac/.mp3。
/// 任何失败（含损坏文件导致的 panic）都以 DecryptOutcome 形式返回，不 panic。
pub fn decrypt_ncm_file(source_path: String, output_dir: String) -> DecryptOutcome {
    let started = Instant::now();
    let output_dir_for_task = output_dir.clone();
    let source_path_for_task = source_path.clone();
    let result = std::panic::catch_unwind(move || {
        match Ncmfile::new(&source_path_for_task) {
            Ok(mut n) => n
                .dump_to_file(
                    Path::new(&output_dir_for_task),
                    None,
                    true,
                    Arc::new(AtomicBool::new(false)),
                )
                .map_err(|e| format!("{}", e)),
            Err(e) => Err(format!("{}", e)),
        }
    });

    let failure = |msg: String| DecryptOutcome {
        success: false,
        error_message: msg,
        output_name: String::new(),
        duration_text: format_duration(started.elapsed()),
    };

    match result {
        Ok(Ok(())) => {
            let stem = Path::new(&source_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let flac = Path::new(&output_dir).join(format!("{}.flac", stem));
            let mp3 = Path::new(&output_dir).join(format!("{}.mp3", stem));
            let out_path = if mp3.exists() { mp3 } else { flac };
            if out_path.exists() {
                DecryptOutcome {
                    success: true,
                    error_message: String::new(),
                    output_name: out_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    duration_text: format_duration(started.elapsed()),
                }
            } else {
                failure("解密流程结束但未找到输出文件".into())
            }
        }
        Ok(Err(e)) => failure(e),
        Err(_) => failure("文件解析触发严重错误（可能已损坏）".into()),
    }
}

/// 读取音频时长（mm:ss），失败返回 "-"
pub fn audio_duration(path: String) -> String {
    get_audio_duration(Path::new(&path))
}
