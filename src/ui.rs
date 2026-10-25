use crate::{app, history, AppWindow};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::error::Error;
use std::rc::Rc;

pub fn run_desktop() -> Result<(), Box<dyn Error>> {
    install_panic_hook();
    let ui = AppWindow::new()?;
    bind_callbacks(&ui);
    ui.run()?;
    Ok(())
}

#[cfg(target_os = "android")]
pub fn run_android(app: slint::android::AndroidApp) -> Result<(), Box<dyn Error>> {
    slint::android::init(app)?;
    let ui = AppWindow::new()?;
    bind_callbacks(&ui);
    ui.run()?;
    Ok(())
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("{}", info);
        if msg.contains("metaflac") && msg.contains("out of range for slice") {
            return;
        }
        default_hook(info);
    }));
}

pub fn bind_callbacks(ui: &AppWindow) {
    let ui_handle = ui.as_weak();
    refresh_history_ui(ui);

    ui.on_source_dir_clicked({
        let ui_handle = ui_handle.clone();
        move || {
            #[cfg(not(target_os = "android"))]
            pick_folder_into(ui_handle.clone(), |ui, s| {
                ui.set_source_path(s.clone().into());
                history::add_history_source(&s);
                refresh_history_ui(ui);
                let target = ui.get_target_path().to_string();
                rescan(ui, &s, &target);
            });
            #[cfg(target_os = "android")]
            {
                let _ = &ui_handle;
                // TODO: Android 端需通过 JNI/Intent 选择目录，见文末说明
            }
        }
    });

    ui.on_source_dir_changed({
        let ui_handle = ui_handle.clone();
        move |path| {
            let p = path.to_string();
            if p.trim().is_empty() { return; }
            history::add_history_source(&p);
            if let Some(ui) = ui_handle.upgrade() {
                refresh_history_ui(&ui);
                let target = ui.get_target_path().to_string();
                rescan(&ui, &p, &target);
            }
        }
    });

    ui.on_target_dir_clicked({
        let ui_handle = ui_handle.clone();
        move || {
            #[cfg(not(target_os = "android"))]
            pick_folder_into(ui_handle.clone(), |ui, s| {
                ui.set_target_path(s.clone().into());
                history::add_history_target(&s);
                refresh_history_ui(ui);
                let source = ui.get_source_path().to_string();
                rescan(ui, &source, &s);
            });
            #[cfg(target_os = "android")]
            {
                let _ = &ui_handle;
            }
        }
    });

    ui.on_target_dir_changed({
        let ui_handle = ui_handle.clone();
        move |path| {
            let p = path.to_string();
            if p.trim().is_empty() { return; }
            history::add_history_target(&p);
            if let Some(ui) = ui_handle.upgrade() {
                refresh_history_ui(&ui);
                let source = ui.get_source_path().to_string();
                rescan(&ui, &source, &p);
            }
        }
    });

    ui.on_remove_source_history({
        let ui_handle = ui_handle.clone();
        move |path| {
            history::remove_history_source(&path.to_string());
            if let Some(ui) = ui_handle.upgrade() { refresh_history_ui(&ui); }
        }
    });

    ui.on_remove_target_history({
        let ui_handle = ui_handle.clone();
        move |path| {
            history::remove_history_target(&path.to_string());
            if let Some(ui) = ui_handle.upgrade() { refresh_history_ui(&ui); }
        }
    });

    ui.on_trigger_process_records({
        let ui_handle = ui_handle.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let source = ui.get_source_path().to_string();
                let target = ui.get_target_path().to_string();
                rescan(&ui, &source, &target);
            }
        }
    });

    ui.on_do_decode({
        let ui_handle = ui_handle.clone();
        move |args| {
            let source = args.source_path.to_string();
            let target = args.target_path.to_string();
            if !source.trim().is_empty() { history::add_history_source(&source); }
            if !target.trim().is_empty() { history::add_history_target(&target); }
            app::run_decode_task(source, target, ui_handle.clone());
        }
    });
}

fn rescan(ui: &AppWindow, source: &str, target: &str) {
    if source.trim().is_empty() { return; }
    let files = ncmmiao::pathparse::pathparse(vec![source.to_string()]);
    let records = app::build_initial_records(&files, target);
    ui.set_records(ModelRc::from(Rc::new(VecModel::from(records))));
}

fn refresh_history_ui(ui: &AppWindow) {
    let src: Vec<slint::SharedString> = history::load_history_sources().iter().map(|s| s.into()).collect();
    ui.set_history_sources(ModelRc::from(Rc::new(VecModel::from(src))));
    let tgt: Vec<slint::SharedString> = history::load_history_targets().iter().map(|s| s.into()).collect();
    ui.set_history_targets(ModelRc::from(Rc::new(VecModel::from(tgt))));
}

#[cfg(not(target_os = "android"))]
fn pick_folder_into(ui_weak: slint::Weak<AppWindow>, apply: fn(&AppWindow, String)) {
    slint::spawn_local(async move {
        if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
            if let Some(ui) = ui_weak.upgrade() {
                apply(&ui, folder.path().to_string_lossy().into_owned());
            }
        }
    }).ok();
}