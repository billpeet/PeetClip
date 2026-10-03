//! Installed builds download updates off the UI thread; Velopack applies them on next launch.
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use velopack::{UpdateCheck, UpdateManager, sources::GithubSource};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const CHANGED: u32 = WM_APP + 4;
pub const TIMER: usize = 2;
pub const INTERVAL_MS: u32 = 6 * 60 * 60 * 1000;
pub const REPOSITORY: &str = "https://github.com/billpeet/PeetClip";
static CHECKING: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<String> = Mutex::new(String::new());

pub fn status() -> String {
    STATUS
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default()
}

fn set_status(hwnd: usize, value: String) {
    if let Ok(mut status) = STATUS.lock() {
        *status = value;
    }
    unsafe {
        PostMessageW(hwnd as _, CHANGED, 0, 0);
    }
}

/// Returns false for development builds, which do not install or contact the update service.
pub fn check(hwnd: usize) -> bool {
    let Ok(manager) = UpdateManager::new(GithubSource::new(REPOSITORY, None, false), None, None)
    else {
        set_status(
            hwnd,
            "Updates are available in the installed version.".into(),
        );
        return false;
    };
    if CHECKING.swap(true, Ordering::AcqRel) {
        return true;
    }
    let worker = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            let version = manager.get_current_version_as_string();
            let result = (|| -> Result<String, velopack::Error> {
                if manager.get_update_pending_restart().is_some() {
                    return Ok(
                        "Update downloaded. It will install next time PeetClip starts.".into(),
                    );
                }
                match manager.check_for_updates()? {
                    UpdateCheck::UpdateAvailable(update) => {
                        manager.download_updates(&update, None)?;
                        Ok("Update downloaded. It will install next time PeetClip starts.".into())
                    }
                    _ => Ok(format!(
                        "Version {version} · Up to date. Checks automatically every 6 hours."
                    )),
                }
            })();
            set_status(
                hwnd,
                result.unwrap_or_else(|_| {
                    format!("Version {version} · Update check failed; will retry automatically.")
                }),
            );
            CHECKING.store(false, Ordering::Release);
        });
    if worker.is_err() {
        CHECKING.store(false, Ordering::Release);
        set_status(
            hwnd,
            "Update check could not start; will retry automatically.".into(),
        );
    }
    true
}
