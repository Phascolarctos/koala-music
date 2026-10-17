#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{DateTime, Local};
use slint::{ModelRc, VecModel};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ncmmiao::Signal;

mod archive;
mod history;
mod runner;

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("{}", info);
        if msg.contains("metaflac") && msg.contains("out of range for slice") {
            return;
        }
        default_hook(info);
    }));

    let ui = AppWindow::new()?;
    let ui_handle = ui.as_weak();

    refresh_history_ui(&ui);

    // 7. 源目录浏览按钮点击
    ui.on_source_dir_clicked({
        let app_weak = ui_handle.clone();
        move || {
            pick_folder_into(app_weak.clone(), |ui, s| {
                ui.set_source_path(s.clone().into());
                history::add_history_source(&s);
                refresh_history_ui(ui);

                let target = ui.get_target_path().to_string();
                scan_and_update_records(ui, &s, &target);
            })
        }
    });

    // 8. 源目录手动输入变更
    ui.on_source_dir_changed({
        let app_weak = ui_handle.clone();
        move |path| {
            let path_str = path.to_string();
            if !path_str.trim().is_empty() {
                history::add_history_source(&path_str);
                if let Some(ui) = app_weak.upgrade() {
                    refresh_history_ui(&ui);
                    let target = ui.get_target_path().to_string();
                    scan_and_update_records(&ui, &path_str, &target);
                }
            }
        }
    });

    // 9. 目标目录浏览按钮点击
    ui.on_target_dir_clicked({
        let app_weak = ui_handle.clone();
        move || {
            pick_folder_into(app_weak.clone(), |ui, s| {
                ui.set_target_path(s.clone().into());
                history::add_history_target(&s);
                refresh_history_ui(ui);

                let source = ui.get_source_path().to_string();
                scan_and_update_records(ui, &source, &s);
            })
        }
    });

    // 10. 目标目录手动输入变更
    ui.on_target_dir_changed({
        let app_weak = ui_handle.clone();
        move |path| {
            let path_str = path.to_string();
            if !path_str.trim().is_empty() {
                history::add_history_target(&path_str);
                if let Some(ui) = app_weak.upgrade() {
                    refresh_history_ui(&ui);
                    let source = ui.get_source_path().to_string();
                    scan_and_update_records(&ui, &source, &path_str);
                }
            }
        }
    });

    // 监听删除源目录历史事件
    ui.on_remove_source_history({
        let app_weak = ui_handle.clone();
        move |path| {
            let path_str = path.to_string();
            history::remove_history_source(&path_str);
            if let Some(ui) = app_weak.upgrade() {
                refresh_history_ui(&ui);
                println!("已从历史中移除: {}", path_str);
            }
        }
    });

    // 监听删除目标目录历史事件
    ui.on_remove_target_history({
        let app_weak = ui_handle.clone();
        move |path| {
            let path_str = path.to_string();
            history::remove_history_target(&path_str);
            if let Some(ui) = app_weak.upgrade() {
                refresh_history_ui(&ui);
                println!("已从历史中移除: {}", path_str);
            }
        }
    });

    ui.on_trigger_process_records({
        let app_weak = ui_handle.clone();
        move || {
            if let Some(ui) = app_weak.upgrade() {
                let source = ui.get_source_path().to_string();
                let target = ui.get_target_path().to_string();
                scan_and_update_records(&ui, &source, &target);
            }
        }
    });

    // 11. 解密执行事件
    ui.on_do_decode({
        let ui_weak = ui_handle.clone();
        move |args| {
            let source: String = args.source_path.to_string();
            let target: String = args.target_path.to_string();

            if !source.trim().is_empty() {
                history::add_history_source(&source);
            }
            if !target.trim().is_empty() {
                history::add_history_target(&target);
            }

            let cancel = Arc::new(AtomicBool::new(false));
            cancel.store(false, Ordering::SeqCst);
            let cancel_for_thread = cancel.clone();

            let files = ncmmiao::pathparse::pathparse(vec![source.clone()]);
            if files.is_empty() {
                return;
            }

            let initial: Vec<Record> = files
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let name = Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path)
                        .to_string();

                    let file_size_str = fs::metadata(path)
                        .map(|m| format_file_size(m.len()))
                        .unwrap_or_else(|_| "-".into());

                    let output_stem = Path::new(&name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&name);

                    let flac_path = Path::new(&target).join(format!("{}.flac", output_stem));
                    let mp3_path = Path::new(&target).join(format!("{}.mp3", output_stem));
                    let exists =
                        !target.trim().is_empty() && (flac_path.exists() || mp3_path.exists());
                    let actual_ext = if mp3_path.exists() { ".mp3" } else { ".flac" };
                    let default_output = format!("{}{}", output_stem, actual_ext);

                    let target_path_exist = if mp3_path.exists() {
                        mp3_path
                    } else {
                        flac_path
                    };
                    let date_str = if exists {
                        fs::metadata(target_path_exist)
                            .and_then(|m| m.modified())
                            .map(|t| {
                                let datetime: DateTime<Local> = t.into();
                                datetime.format("%Y-%m-%d %H:%M").to_string()
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };

                    Record {
                        id: i as i32,
                        source_name: name.into(),
                        output_name: default_output.into(),
                        status: if exists { 2 } else { 0 },
                        duration: if exists {
                            "已跳过".into()
                        } else {
                            "-".into()
                        },
                        error_message: "".into(),
                        file_size: file_size_str.into(),
                        decrypt_date: date_str.into(),
                    }
                })
                .collect();

            let records = Arc::new(Mutex::new(initial));

            {
                let snapshot = records.lock().unwrap().clone();
                let ui_weak = ui_weak.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_records(ModelRc::from(Rc::new(VecModel::from(snapshot))));
                    }
                })
                .ok();
            }

            let task_timers = Arc::new(Mutex::new(vec![None::<Instant>; files.len()]));
            let timers_sig = task_timers.clone();
            let records_sig = records.clone();
            let ui_weak_sig = ui_weak.clone();

            std::thread::spawn(move || {
                let _result = runner::run_once(
                    vec![source],
                    &target,
                    4,
                    true,
                    cancel_for_thread,
                    Box::new(move |i, signal| {
                        let snapshot = {
                            let mut r = records_sig.lock().unwrap();
                            let mut timers = timers_sig.lock().unwrap();

                            match signal {
                                Signal::Start => {
                                    if r[i].status == 2 {
                                        return;
                                    }
                                    r[i].status = 1;
                                    timers[i] = Some(Instant::now());
                                }
                                Signal::Decrypt(_) => {}
                                Signal::End => {
                                    r[i].status = 2;
                                    if let Some(start_time) = timers[i].take() {
                                        let elapsed = start_time.elapsed();
                                        r[i].duration = format_duration(elapsed).into();
                                    }
                                    r[i].decrypt_date =
                                        Local::now().format("%Y-%m-%d %H:%M").to_string().into();
                                }
                                Signal::Err(e) => {
                                    r[i].status = 3;
                                    if let Some(start_time) = timers[i].take() {
                                        let elapsed = start_time.elapsed();
                                        r[i].duration = format_duration(elapsed).into();
                                    }
                                    r[i].error_message = format!("{}", e).into();
                                }
                                _ => return,
                            }
                            r.clone()
                        };

                        let ui_weak = ui_weak_sig.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_weak.upgrade() {
                                ui.set_records(ModelRc::from(Rc::new(VecModel::from(snapshot))));
                            }
                        })
                        .ok();
                    }),
                );
            });
        }
    });

    ui.run()?;
    Ok(())
}

fn pick_folder_into(app_weak: slint::Weak<AppWindow>, apply: fn(&AppWindow, String)) {
    slint::spawn_local(async move {
        if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
            if let Some(app) = app_weak.upgrade() {
                apply(&app, folder.path().to_string_lossy().into_owned());
            }
        }
    })
    .unwrap();
}

fn scan_and_update_records(ui: &AppWindow, source_path: &str, target_path: &str) {
    if source_path.trim().is_empty() {
        return;
    }

    let files = ncmmiao::pathparse::pathparse(vec![source_path.to_string()]);
    let initial_records: Vec<Record> = files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .to_string();

            let file_size_str = fs::metadata(path)
                .map(|m| format_file_size(m.len()))
                .unwrap_or_else(|_| "-".into());

            let output_stem = Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name);

            let flac_path = Path::new(target_path).join(format!("{}.flac", output_stem));
            let mp3_path = Path::new(target_path).join(format!("{}.mp3", output_stem));
            let exists =
                !target_path.trim().is_empty() && (flac_path.exists() || mp3_path.exists());
            let actual_ext = if mp3_path.exists() { ".mp3" } else { ".flac" };
            let default_output = format!("{}{}", output_stem, actual_ext);

            let target_path_exist = if mp3_path.exists() {
                mp3_path
            } else {
                flac_path
            };
            let date_str = if exists {
                fs::metadata(target_path_exist)
                    .and_then(|m| m.modified())
                    .map(|t| {
                        let datetime: DateTime<Local> = t.into();
                        datetime.format("%Y-%m-%d %H:%M").to_string()
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            Record {
                id: i as i32,
                source_name: name.into(),
                output_name: default_output.into(),
                status: if exists { 2 } else { 0 },
                duration: if exists {
                    "已跳过".into()
                } else {
                    "-".into()
                },
                error_message: "".into(),
                file_size: file_size_str.into(),
                decrypt_date: date_str.into(),
            }
        })
        .collect();

    ui.set_records(ModelRc::from(Rc::new(VecModel::from(initial_records))));
}

fn format_file_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

fn format_duration(duration: Duration) -> String {
    let micros = duration.as_micros();
    if micros < 1000 {
        format!("{}µs", micros)
    } else {
        let millis = duration.as_millis();
        if millis < 1000 {
            format!("{}ms", millis)
        } else {
            format!("{:.2}s", duration.as_secs_f64())
        }
    }
}

// 辅助函数：将单个文件大小字符串转换回 bytes 用于累加
fn parse_size_to_bytes(size_str: &str) -> u64 {
    let parts: Vec<&str> = size_str.trim().split_whitespace().collect();
    if parts.len() != 2 {
        return 0;
    }
    let val: f64 = parts[0].parse().unwrap_or(0.0);
    match parts[1].to_uppercase().as_str() {
        "GB" => (val * 1024.0 * 1024.0 * 1024.0) as u64,
        "MB" => (val * 1024.0 * 1024.0) as u64,
        "KB" => (val * 1024.0) as u64,
        "B" => val as u64,
        _ => 0,
    }
}

// 辅助函数：将单个文件耗时字符串转换回秒数用于累加
fn parse_duration_to_secs(dur_str: &str) -> f64 {
    if dur_str.ends_with("ms") {
        dur_str.trim_end_matches("ms").parse::<f64>().unwrap_or(0.0) / 1000.0
    } else if dur_str.ends_with('s') {
        dur_str.trim_end_matches('s').parse::<f64>().unwrap_or(0.0)
    } else if dur_str.ends_with("µs") {
        dur_str.trim_end_matches("µs").parse::<f64>().unwrap_or(0.0) / 1000000.0
    } else {
        0.0
    }
}

fn refresh_history_ui(ui: &AppWindow) {
    let source_dirs = history::load_history_sources();
    let source_strings: Vec<slint::SharedString> = source_dirs.iter().map(|s| s.into()).collect();
    ui.set_history_sources(ModelRc::from(Rc::new(VecModel::from(source_strings))));

    let target_dirs = history::load_history_targets();
    let target_strings: Vec<slint::SharedString> = target_dirs.iter().map(|s| s.into()).collect();
    ui.set_history_targets(ModelRc::from(Rc::new(VecModel::from(target_strings))));
}
