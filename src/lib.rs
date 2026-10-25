pub mod app;
pub mod archive;
pub mod history;
pub mod ncm;
pub mod ui;

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