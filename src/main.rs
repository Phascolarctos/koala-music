#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    koala_rust::ui::run_desktop()
}