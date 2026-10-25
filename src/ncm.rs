use crossbeam_channel::bounded;
use ncmmiao::{AppError, Ncmfile, Signal, pathparse, threadpool};
use std::collections::HashSet;
use std::panic::catch_unwind;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct RunSummary {
    pub total: usize,
    pub success: usize,
    pub ignored: usize,
    pub failure: usize,
    pub cancelled: usize,
    pub failures: Vec<FailureRecord>,
}

#[derive(Clone, Debug)]
pub enum UiSignal {
    Start,
    MetaInfo,
    Cover,
    Progress(f64),
    Saving,
    Done,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct FailureRecord {
    pub index: usize,
    pub path: String,
    pub name: String,
    pub error: String,
}

pub type SignalHandler = Box<dyn Fn(usize, &Signal) + Send + 'static>;

pub fn run_once(
    input: Vec<String>,
    output_dir: &str,
    max_workers: usize,
    force_save: bool,
    cancel: Arc<AtomicBool>,
    on_signal: SignalHandler,
) -> Result<RunSummary, AppError> {
    let files = pathparse::pathparse(input);
    let task_count = files.len();

    let mut summary = RunSummary {
        total: task_count,
        ..Default::default()
    };

    if task_count == 0 {
        return Ok(summary);
    }

    std::fs::create_dir_all(output_dir).map_err(|_| AppError::CannotCreateDir)?;

    let names: Vec<String> = files
        .iter()
        .map(|p| {
            Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(p.as_str())
                .to_string()
        })
        .collect();

    let pool = threadpool::Pool::new(max_workers);
    let (tx, rx) = bounded::<(usize, Signal)>(max_workers * 6);

    let mut forwarders: Vec<JoinHandle<()>> = Vec::with_capacity(task_count);
    for (i, filepath) in files.iter().enumerate() {
        let (file_tx, file_rx) = bounded::<Signal>(16);
        let tx = tx.clone();
        forwarders.push(thread::spawn(move || {
            while let Ok(s) = file_rx.recv() {
                if tx.send((i, s)).is_err() {
                    break;
                }
            }
        }));

        let output = output_dir.to_string();
        let cancel = cancel.clone();
        let filepath = filepath.clone();
        let filepath_for_log = filepath.clone();
        let tx_fallback = file_tx.clone();
        
        pool.execute(move || {
            let result = catch_unwind(move || match Ncmfile::new(filepath.as_str()) {
                Ok(mut n) => {
                    if let Err(e) = n.dump_to_file(
                        Path::new(&output),
                        Some(file_tx.clone()),
                        force_save,
                        cancel,
                    ) {
                        let _ = file_tx.send(Signal::Err(e));
                    }
                }
                Err(e) => {
                    let _ = file_tx.send(Signal::Err(e));
                }
            });

            if result.is_err() {
                eprintln!(
                    "警告: 文件解析触发严重错误（可能已损坏），已安全跳过: {}",
                    filepath_for_log
                );
                let _ = tx_fallback.send(Signal::Err(ncmmiao::AppError::FileDataError));
            }
        });
    }

    let mut done = 0usize;
    let mut completed_files = HashSet::new();
    let mut last_activity = Instant::now();

    while done < task_count {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok((i, signal)) => {
                last_activity = Instant::now();
                on_signal(i, &signal);

                let is_done = match &signal {
                    Signal::Start | Signal::Decrypt(_) => false,
                    Signal::End => {
                        summary.success += 1;
                        true
                    }
                    Signal::Err(AppError::ProtectFile) => {
                        summary.ignored += 1;
                        true
                    }
                    Signal::Err(AppError::Cancelled) => {
                        summary.cancelled += 1;
                        true
                    }
                    Signal::Err(e) => {
                        summary.failure += 1;
                        summary.failures.push(FailureRecord {
                            index: i,
                            path: files[i].clone(),
                            name: names[i].clone(),
                            error: format!("{}", e),
                        });
                        true
                    }
                    _ => false,
                };

                if is_done && completed_files.insert(i) {
                    done += 1;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if last_activity.elapsed() > Duration::from_secs(30) {
                    eprintln!("警告: 后台解密任务长时间无响应（可能死锁），强制中断当前批次。");
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    for i in 0..task_count {
        if !completed_files.contains(&i) {
            on_signal(i, &Signal::Err(AppError::FileDataError));
            summary.failure += 1;
            summary.failures.push(FailureRecord {
                index: i,
                path: files[i].clone(),
                name: names[i].clone(),
                error: "文件解析错误或任务中断".to_string(),
            });
        }
    }

    for f in forwarders {
        let _ = f.join();
    }

    Ok(summary)
}