use crate::{AppWindow, Record};
use chrono::{DateTime, Local};
use lofty::file::AudioFile;
use lofty::read_from_path;
use ncmmiao::Signal;
use slint::{ModelRc, VecModel};
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn get_audio_duration(path: &Path) -> String {
    if let Ok(tagged_file) = read_from_path(path) {
        let properties = tagged_file.properties();
        let secs = properties.duration().as_secs();
        return format!("{:02}:{:02}", secs / 60, secs % 60);
    }
    "-".to_string()
}

pub fn build_initial_records(files: &[String], target: &str) -> Vec<Record> {
    files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
                .to_string();
            let file_size = fs::metadata(path)
                .map(|m| format_file_size(m.len()))
                .unwrap_or_else(|_| "-".into());
            let stem = Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name);
            let flac = Path::new(target).join(format!("{}.flac", stem));
            let mp3 = Path::new(target).join(format!("{}.mp3", stem));
            let exists = !target.trim().is_empty() && (flac.exists() || mp3.exists());
            let ext = if mp3.exists() { ".mp3" } else { ".flac" };
            let out = format!("{}{}", stem, ext);
            let exist_path = if mp3.exists() { mp3 } else { flac };

            let music_length = if exists {
                get_audio_duration(&exist_path)
            } else {
                "-".to_string()
            };

            let date = if exists {
                fs::metadata(exist_path)
                    .and_then(|m| m.modified())
                    .map(|t| {
                        let dt: DateTime<Local> = t.into();
                        dt.format("%Y-%m-%d %H:%M").to_string()
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };

            Record {
                id: i as i32,
                source_name: name.into(),
                output_name: out.into(),
                status: if exists { 2 } else { 0 },
                duration: if exists {
                    "已跳过".into()
                } else {
                    "-".into()
                },
                music_length: music_length.into(),
                error_message: "".into(),
                file_size: file_size.into(),
                decrypt_date: date.into(),
            }
        })
        .collect()
}

pub fn run_decode_task(source: String, target: String, ui_weak: slint::Weak<AppWindow>) {
    let files = ncmmiao::pathparse::pathparse(vec![source.clone()]);
    if files.is_empty() {
        return;
    }

    let records = Arc::new(Mutex::new(build_initial_records(&files, &target)));
    push_snapshot(&ui_weak, records.lock().unwrap().clone());

    let cancel = Arc::new(AtomicBool::new(false));
    let timers = Arc::new(Mutex::new(vec![None::<Instant>; files.len()]));

    let cancel_th = cancel.clone();
    let records_th = records.clone();
    let timers_th = timers.clone();
    let ui_th = ui_weak.clone();
    let target_th = target.clone();

    std::thread::spawn(move || {
        let _ = crate::ncm::run_once(
            vec![source],
            &target,
            4,
            true,
            cancel_th,
            Box::new(move |i, signal| {
                let snapshot = {
                    let mut r = records_th.lock().unwrap();
                    let mut t = timers_th.lock().unwrap();
                    match signal {
                        Signal::Start => {
                            if r[i].status == 2 {
                                return;
                            }
                            r[i].status = 1;
                            t[i] = Some(Instant::now());
                        }
                        Signal::Decrypt(_) => {}
                        Signal::End => {
                            r[i].status = 2;
                            if let Some(st) = t[i].take() {
                                r[i].duration = format_duration(st.elapsed()).into();
                            }
                            r[i].decrypt_date =
                                Local::now().format("%Y-%m-%d %H:%M").to_string().into();

                            let source_name_str = r[i].source_name.to_string();
                            let stem = Path::new(&source_name_str)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");
                            let flac = Path::new(&target_th).join(format!("{}.flac", stem));
                            let mp3 = Path::new(&target_th).join(format!("{}.mp3", stem));
                            let out_path = if mp3.exists() { mp3 } else { flac };

                            if out_path.exists() {
                                r[i].music_length = get_audio_duration(&out_path).into();
                            } else {
                                r[i].music_length = "未知".into();
                            }
                        }
                        Signal::Err(e) => {
                            r[i].status = 3;
                            if let Some(st) = t[i].take() {
                                r[i].duration = format_duration(st.elapsed()).into();
                            }
                            r[i].error_message = format!("{}", e).into();
                        }
                        _ => return,
                    }
                    r.clone()
                };
                push_snapshot(&ui_th, snapshot);
            }),
        );
    });
}

fn push_snapshot(ui_weak: &slint::Weak<AppWindow>, snapshot: Vec<Record>) {
    let ui_weak = ui_weak.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_records(ModelRc::from(Rc::new(VecModel::from(snapshot))));
        }
    })
    .ok();
}

pub fn format_file_size(size: u64) -> String {
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

pub fn format_duration(d: Duration) -> String {
    let micros = d.as_micros();
    if micros < 1000 {
        format!("{}µs", micros)
    } else if d.as_millis() < 1000 {
        format!("{}ms", d.as_millis())
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}
