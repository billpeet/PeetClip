#![windows_subsystem = "windows"]

mod config;
mod history;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod updates;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    velopack::VelopackApp::build().run();
    #[cfg(windows)]
    win::run();
    #[cfg(not(windows))]
    eprintln!("PeetClip requires Windows.");
}
