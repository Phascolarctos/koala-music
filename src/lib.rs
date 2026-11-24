pub mod app;
pub mod archive;
pub mod core_api;
pub mod history;
pub mod ncm;
pub mod ui;

#[cfg(target_os = "android")]
pub mod android_bridge;

// UniFFI 脚手架（build.rs 由 src/koala.udl 生成）
uniffi::include_scaffolding!("koala");

// 供 Kotlin 跨平台使用的内核接口，re-export 到 crate 根以匹配 UDL 命名空间
pub use core_api::{
    audio_duration, decrypt_ncm_file, scan_ncm_folder, DecryptOutcome, FileEntry,
};

slint::include_modules!();

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
        android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    if let Err(e) = ui::run_android(app) {
        log::error!("android_main failed: {e}");
    }
}