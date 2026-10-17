// src/runner.rs
use crossbeam_channel::bounded;
use ncmmiao::{AppError, Ncmfile, Signal, pathparse, threadpool};
use std::panic::catch_unwind;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::{self, JoinHandle};

/// 一次执行的汇总结果
#[derive(Debug, Default, Clone)]
pub struct RunSummary {
    pub total: usize,
    pub success: usize,
    pub ignored: usize, // ProtectFile
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

/// 信号回调：每收到一条 (文件索引, 信号) 就调用一次
pub type SignalHandler = Box<dyn Fn(usize, &Signal) + Send + 'static>;

/// 执行一次解密流程。
pub fn run_once(
    input: Vec<String>,
    output_dir: &str,
    max_workers: usize,
    force_save: bool,
    cancel: Arc<AtomicBool>,
    on_signal: SignalHandler,
) -> Result<RunSummary, AppError> {
    // 1. 扫描：pathparse 自行判断是文件还是目录
    let files = pathparse::pathparse(input);
    let task_count = files.len();

    let mut summary = RunSummary {
        total: task_count,
        ..Default::default()
    };

    if task_count == 0 {
        return Ok(summary);
    }

    // 2. 创建输出目录
    std::fs::create_dir_all(output_dir).map_err(|_| AppError::CannotCreateDir)?;

    // 3. 预计算文件名（给 UI 显示用）
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

    // 4. 线程池
    let pool = threadpool::Pool::new(max_workers);

    // 5. 全局通道
    let (tx, rx) = bounded::<(usize, Signal)>(max_workers * 6);

    // 6. 每文件一个转发线程 + 提交任务
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
        let file_tx_for_err = file_tx.clone();
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
                let _ = file_tx_for_err.send(Signal::Err(ncmmiao::AppError::FileDataError));
            }
        });
    }

    // 7. 消费信号
    let mut done = 0usize;
    for (i, signal) in rx {
        on_signal(i, &signal);

        match &signal {
            Signal::Start | Signal::Decrypt(_) => continue,
            Signal::End => summary.success += 1,
            Signal::Err(AppError::ProtectFile) => summary.ignored += 1,
            Signal::Err(AppError::Cancelled) => summary.cancelled += 1,
            Signal::Err(e) => {
                summary.failure += 1;
                summary.failures.push(FailureRecord {
                    index: i,
                    path: files[i].clone(),
                    name: names[i].clone(),
                    error: format!("{}", e),
                });
            }
            _ => {}
        }

        done += 1;
        if done >= task_count {
            break;
        }
    }

    // 8. 等所有转发线程退出
    for f in forwarders {
        let _ = f.join();
    }

    Ok(summary)
}
