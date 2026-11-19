//! `reborn`: the Reborn desktop app.
//!
//! A thin frontend on `rebornd` (ADR 0002, 0008, 0009): it shows the
//! daemon's state and sends it requests. If no daemon is running, the app
//! starts one from the same folder as its own executable.
#![forbid(unsafe_code)]
// Release builds on Windows are GUI apps without a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod daemon;

use tracing_subscriber::EnvFilter;

fn main() -> iced::Result {
    let from_env = EnvFilter::try_from_default_env();
    // fontdb warns about every unreadable system font (e.g. mstmc.ttf on
    // Windows); that is noise for users.
    let filter = from_env.unwrap_or_else(|_| EnvFilter::new("warn,fontdb=error"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    iced::application(app::App::boot, app::App::update, app::App::view)
        .title("Reborn")
        .theme(iced::Theme::Dark)
        .subscription(app::App::subscription)
        .window_size((1100.0, 720.0))
        .run()
}
