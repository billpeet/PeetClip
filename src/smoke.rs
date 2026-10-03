//! Opt-in live WinUI integration check. Never writes the system clipboard.
use super::*;
use std::cell::Cell;

pub const TIMER: usize = 42;
thread_local! {
    static STAGE: Cell<u8> = const { Cell::new(0) };
    static SELECTION: Cell<usize> = const { Cell::new(0) };
    static SETTINGS_ERROR: Cell<bool> = const { Cell::new(false) };
}

fn enabled() -> bool {
    std::env::args().any(|arg| arg == "--smoke-test")
}
fn report(text: &str) {
    let path = std::env::current_exe()
        .unwrap()
        .with_file_name("smoke-result.txt");
    std::fs::write(path, text).unwrap();
}
fn check(ok: bool, description: &str) {
    if !ok {
        report(&format!("FAIL: {description}"));
        std::process::exit(1);
    }
}
pub fn selected(index: usize) {
    SELECTION.with(|v| v.set(index));
}
pub fn settings_status(error: bool) {
    SETTINGS_ERROR.with(|value| value.set(error));
}
pub fn start(hwnd: HWND) {
    if enabled() {
        report("RUNNING");
        unsafe {
            SetTimer(hwnd, TIMER, 700, None);
        }
    }
}
pub fn finish() {
    if enabled() {
        check(
            STAGE.with(Cell::get) == 10,
            "Reactor exited before all checks completed",
        );
        report(
            "PASS: WinUI attachment, embedded icon, hidden startup, bottom-right placement, keyboard navigation, hide/reopen, settings open/validation/cancel, graceful exit.",
        );
    }
}

pub(super) fn tick(app: &mut App) {
    if !enabled() {
        return;
    }
    unsafe {
        let stage = STAGE.with(Cell::get);
        report(&format!("RUNNING: stage {stage}"));
        check(!app.picker.is_null(), "Reactor window attachment");
        check(!app.hook.is_null(), "UI-thread keyboard hook installed");
        match stage {
            0 => {
                check(IsWindowVisible(app.picker) == 0, "startup stays hidden");
                check(!load_icon(32).is_null(), "embedded icon loads");
                app.history.items.clear();
                for text in ["First test item", "Second test item", "Third test item"] {
                    app.history.push(text.into());
                }
                app.show();
            }
            1 => {
                check(IsWindowVisible(app.picker) != 0, "picker opens");
                let mut rect: RECT = zeroed();
                GetWindowRect(app.picker, &mut rect);
                let mut info: MONITORINFO = zeroed();
                info.cbSize = size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(
                    MonitorFromWindow(app.picker, MONITOR_DEFAULTTONEAREST),
                    &mut info,
                );
                let gap_x = info.rcWork.right - rect.right;
                let gap_y = info.rcWork.bottom - rect.bottom;
                check(
                    (0..=60).contains(&gap_x) && (0..=60).contains(&gap_y),
                    "bottom-right work-area position",
                );
                let focused = GetFocus();
                check(
                    !focused.is_null() && GetAncestor(focused, GA_ROOT) == app.picker,
                    "WinUI receives keyboard focus",
                );
                PostMessageW(focused, WM_KEYDOWN, 0x28, 0);
            }
            2 => {
                check(SELECTION.with(Cell::get) == 1, "Down selects second item");
                PostMessageW(app.picker, WM_KEYDOWN, 0x26, 0);
                PostMessageW(app.picker, WM_KEYDOWN, 0x1b, 0);
            }
            3 => {
                check(SELECTION.with(Cell::get) == 0, "Up selects first item");
                check(IsWindowVisible(app.picker) == 0, "Escape hides picker");
                PostMessageW(app.hwnd, WM_HOTKEY, app.hotkey_id as usize, 0);
            }
            4 => {
                check(
                    IsWindowVisible(app.picker) != 0,
                    "global-hotkey dispatch reopens picker",
                );
                check(
                    SELECTION.with(Cell::get) == 0,
                    "reopening selects newest item",
                );
                // An out-of-range number exercises dispatch without changing the clipboard.
                PostMessageW(app.picker, WM_KEYDOWN, 0x39, 0);
                PostMessageW(app.picker, WM_CLOSE, 0, 0);
            }
            5 => {
                check(
                    IsWindowVisible(app.picker) == 0,
                    "close button hides picker",
                );
                app.show();
                PostMessageW(app.picker, WM_KEYDOWN, 0x71, 0);
            }
            6 => {
                check(SETTINGS_OPEN.with(Cell::get), "F2 opens settings");
                let sender = app.sender.as_ref().unwrap();
                let _ = sender.send(ui::Message::ShortcutChanged("C".into()));
                let _ = sender.send(ui::Message::SaveSettings);
            }
            7 => {
                check(
                    SETTINGS_ERROR.with(Cell::get) && SETTINGS_OPEN.with(Cell::get),
                    "invalid settings remain open with an error",
                );
                PostMessageW(app.picker, WM_CLOSE, 0, 0);
            }
            8 => {
                check(
                    !SETTINGS_OPEN.with(Cell::get),
                    "cancel settings closes dialog",
                );
                check(IsWindowVisible(app.picker) != 0, "cancel returns to popup");
                // Same queued command as the tray Settings item.
                let _ = app.sender.as_ref().unwrap().send(ui::Message::Settings);
            }
            9 => {
                check(
                    SETTINGS_OPEN.with(Cell::get),
                    "tray settings command opens dialog",
                );
                let sender = app.sender.as_ref().unwrap();
                let _ = sender.send(ui::Message::CancelSettings);
                let _ = sender.send(ui::Message::Exit);
                KillTimer(app.hwnd, TIMER);
            }
            _ => check(false, "unexpected smoke stage"),
        }
        STAGE.with(|s| s.set(stage + 1));
    }
}
