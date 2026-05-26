#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod lightweight_http_client;
mod md_editor_app;

fn main() {
    md_editor_app::run();
}
