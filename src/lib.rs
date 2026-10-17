#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).unwrap();
    let app = AppWindow::new.unwrap();

    app.run().unwrap();
}
