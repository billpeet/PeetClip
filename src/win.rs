use crate::{
    config::{self, Config},
    history::{History, MAX_TEXT_BYTES, preview},
    ui, updates,
};
#[cfg(feature = "ui-smoke-test")]
#[path = "smoke.rs"]
pub(crate) mod smoke;
use std::{
    cell::{Cell, RefCell},
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{DataExchange::*, LibraryLoader::*, Memory::*, Threading::*},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

const TRAY: u32 = WM_APP + 1;
const RETRY: usize = 1;
const CF_TEXT_UNICODE: u32 = 13;
const SHOW: usize = 100;
const SETTINGS: usize = 101;
const RELOAD: usize = 102;
const CLEAR: usize = 103;
const EXIT: usize = 104;
thread_local! { static STATE: RefCell<Option<App>> = const { RefCell::new(None) }; }
thread_local! { static SETTINGS_OPEN: Cell<bool> = const { Cell::new(false) }; }

struct App {
    hwnd: HWND,
    picker: HWND,
    hook: HHOOK,
    sender: Option<windows_reactor::LocalSender<ui::Message>>,
    previous: HWND,
    history: History,
    config: Config,
    hotkey_id: i32,
    registered: bool,
    taskbar_created: u32,
    retries: u8,
    copied_sequence: Option<u32>,
    exiting: bool,
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe fn message(hwnd: HWND, text: &str) {
    unsafe {
        MessageBoxW(
            hwnd,
            wide(text).as_ptr(),
            wide("PeetClip").as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

unsafe fn register_hotkey(hwnd: HWND, id: i32, config: &mut Config) -> bool {
    unsafe {
        if RegisterHotKey(hwnd, id, config.modifiers | MOD_NOREPEAT, config.key) != 0 {
            return true;
        }
        if config.modifiers == MOD_WIN
            && config.key == b'C' as u32
            && RegisterHotKey(hwnd, id, MOD_ALT | MOD_NOREPEAT, b'C' as u32) != 0
        {
            config.modifiers = MOD_ALT;
            config.hotkey = "Alt+C".into();
            return true;
        }
        false
    }
}

pub fn run() {
    unsafe {
        let mutex = CreateMutexW(null(), 0, wide("Local\\PeetClip.SingleInstance").as_ptr());
        if mutex.is_null() {
            message(null_mut(), "Cannot create the application instance lock.");
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let existing = FindWindowW(wide("PeetClip").as_ptr(), null());
            if !existing.is_null() {
                PostMessageW(existing, WM_APP + 2, 0, 0);
            }
            CloseHandle(mutex);
            return;
        }
        let mut config = config::load().unwrap_or_else(|e| {
            message(
                null_mut(),
                &format!("Settings could not be loaded: {e}\nUsing defaults."),
            );
            Config::default()
        });
        let instance = GetModuleHandleW(null());
        let class = wide("PeetClip");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        // An invisible top-level service window receives clipboard and shell broadcasts.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("PeetClip Service").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            message(null_mut(), "Could not create the clipboard listener.");
            CloseHandle(mutex);
            return;
        }
        let registered = register_hotkey(hwnd, 1, &mut config);
        let mut app = App {
            hwnd,
            picker: null_mut(),
            hook: null_mut(),
            sender: None,
            previous: GetForegroundWindow(),
            history: History::new(config.history_limit),
            config,
            hotkey_id: 1,
            registered,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            retries: 0,
            copied_sequence: None,
            exiting: false,
        };
        if AddClipboardFormatListener(hwnd) == 0 {
            message(hwnd, "Cannot subscribe to clipboard changes.");
            DestroyWindow(hwnd);
            CloseHandle(mutex);
            return;
        }
        app.add_tray();
        app.capture();
        if !registered {
            app.hotkey_warning();
        }
        STATE.with(|state| *state.borrow_mut() = Some(app));
        if updates::check(hwnd as usize) {
            SetTimer(hwnd, updates::TIMER, updates::INTERVAL_MS, None);
        }
        #[cfg(feature = "ui-smoke-test")]
        smoke::start(hwnd);
        if let Err(error) = windows_reactor::App::run_component::<ui::ClipboardView>(()) {
            message(null_mut(), &format!("Could not start WinUI: {error}"));
        }
        STATE.with(|state| {
            if let Some(app) = state.borrow_mut().take() {
                app.cleanup();
                DestroyWindow(app.hwnd);
            }
        });
        CloseHandle(mutex);
        #[cfg(feature = "ui-smoke-test")]
        smoke::finish();
    }
}

pub fn connect(sender: windows_reactor::LocalSender<ui::Message>) {
    STATE.with(|state| {
        if let Some(app) = state.borrow_mut().as_mut() {
            app.sender = Some(sender);
            unsafe {
                app.refresh();
            }
        }
    });
}

pub fn copy(index: usize) {
    STATE.with(|state| {
        if let Some(app) = state.borrow_mut().as_mut() {
            unsafe {
                app.copy(index);
            }
        }
    });
}
pub fn hide() {
    STATE.with(|state| {
        if let Some(app) = state.borrow().as_ref() {
            unsafe {
                app.hide(true);
            }
        }
    });
}

pub fn allow_exit() {
    STATE.with(|state| {
        if let Some(app) = state.borrow_mut().as_mut() {
            app.exiting = true;
        }
    });
}

pub fn begin_settings() -> Config {
    SETTINGS_OPEN.with(|open| open.set(true));
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let app = state.as_mut().unwrap();
        unsafe {
            if IsWindowVisible(app.picker) == 0 {
                app.show();
            } else {
                SetForegroundWindow(app.picker);
            }
        }
        app.config.clone()
    })
}

pub fn end_settings() {
    SETTINGS_OPEN.with(|open| open.set(false));
}

pub fn save_settings(config: Config) -> Result<(), String> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let app = state.as_mut().unwrap();
        unsafe { app.apply_config(config, true) }
    })
}

pub fn attach() {
    unsafe {
        let mut picker: HWND = null_mut();
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find_picker),
            (&mut picker as *mut HWND) as isize,
        );
        if picker.is_null() {
            return;
        }
        STATE.with(|state| {
            if let Some(app) = state.borrow_mut().as_mut() {
                if !app.picker.is_null() {
                    return;
                }
                app.picker = picker;
                SetWindowSubclass(picker, Some(picker_proc), 1, 0);
                app.hook = SetWindowsHookExW(
                    WH_GETMESSAGE,
                    Some(keyboard_hook),
                    null_mut(),
                    GetCurrentThreadId(),
                );
                let style = GetWindowLongPtrW(picker, GWL_EXSTYLE);
                SetWindowLongPtrW(
                    picker,
                    GWL_EXSTYLE,
                    (style | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
                );
                let style = GetWindowLongPtrW(picker, GWL_STYLE);
                SetWindowLongPtrW(
                    picker,
                    GWL_STYLE,
                    // Keep the resize frame, but remove caption and system buttons.
                    style & !((WS_CAPTION | WS_SYSMENU | WS_MAXIMIZEBOX | WS_MINIMIZEBOX) as isize),
                );
                SendMessageW(
                    picker,
                    WM_SETICON,
                    ICON_SMALL as usize,
                    load_icon(20) as isize,
                );
                SendMessageW(
                    picker,
                    WM_SETICON,
                    ICON_BIG as usize,
                    load_icon(32) as isize,
                );
                SetWindowPos(
                    picker,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
                // Finish startup on a subsequent message, after Reactor's first activation.
                PostMessageW(app.hwnd, WM_APP + 3, 0, 0);
            }
        });
    }
}

unsafe extern "system" fn find_picker(hwnd: HWND, param: LPARAM) -> i32 {
    unsafe {
        let mut title = [0u16; 128];
        let length = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        if String::from_utf16_lossy(&title[..length as usize]) == "PeetClip" {
            *(param as *mut HWND) = hwnd;
            return 0;
        }
        1
    }
}

unsafe fn load_icon(size: i32) -> HICON {
    unsafe {
        LoadImageW(
            GetModuleHandleW(null()),
            std::ptr::without_provenance::<u16>(1), // MAKEINTRESOURCEW(1), not a dereferenced pointer.
            IMAGE_ICON,
            size,
            size,
            LR_SHARED,
        ) as HICON
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && wparam == PM_REMOVE as usize {
            let msg = &mut *(lparam as *mut MSG);
            if msg.message == WM_KEYDOWN {
                let target = STATE.with(|state| {
                    state.try_borrow().ok().and_then(|state| {
                        state.as_ref().and_then(|app| {
                            if !SETTINGS_OPEN.with(Cell::get)
                                && !app.picker.is_null()
                                && IsWindowVisible(app.picker) != 0
                                && GetAncestor(msg.hwnd, GA_ROOT) == app.picker
                            {
                                app.sender.clone()
                            } else {
                                None
                            }
                        })
                    })
                });
                if let Some(sender) = target {
                    let action = match msg.wParam as u32 {
                        0x1b => Some(ui::Message::Hide),
                        // Let WinUI activate the focused row or Settings link on Enter.
                        0x71 => Some(ui::Message::Settings), // F2
                        0x26 => Some(ui::Message::Move(-1)),
                        0x28 => Some(ui::Message::Move(1)),
                        key @ 0x31..=0x39 => Some(ui::Message::Copy((key - 0x31) as usize)),
                        key @ 0x61..=0x69 => Some(ui::Message::Copy((key - 0x61) as usize)),
                        _ => None,
                    };
                    if let Some(action) = action {
                        let _ = sender.send(action);
                        msg.message = WM_NULL;
                    }
                }
            }
        }
        CallNextHookEx(null_mut(), code, wparam, lparam)
    }
}

unsafe extern "system" fn picker_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        let exiting = STATE.with(|state| {
            state
                .try_borrow()
                .ok()
                .is_some_and(|state| state.as_ref().is_some_and(|app| app.exiting))
        });
        if exiting {
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        }
        if SETTINGS_OPEN.with(Cell::get) && msg == WM_CLOSE {
            STATE.with(|state| {
                if let Ok(state) = state.try_borrow()
                    && let Some(sender) = state.as_ref().and_then(|app| app.sender.as_ref())
                {
                    let _ = sender.send(ui::Message::CancelSettings);
                }
            });
            return 0;
        }
        if !SETTINGS_OPEN.with(Cell::get)
            && (msg == WM_CLOSE || (msg == WM_ACTIVATE && wparam & 0xffff == WA_INACTIVE as usize))
        {
            ShowWindow(hwnd, SW_HIDE);
            if msg == WM_CLOSE {
                hide();
            }
            return 0;
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(picker_proc), 1);
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

impl App {
    unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
        unsafe {
            let mut data: NOTIFYICONDATAW = zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = 1;
            data
        }
    }
    unsafe fn add_tray(&self) {
        unsafe {
            let mut data = self.tray_data();
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            data.uCallbackMessage = TRAY;
            data.hIcon = load_icon(32);
            let tip = wide(&format!(
                "PeetClip — {}",
                if self.registered {
                    &self.config.hotkey
                } else {
                    "click to open"
                }
            ));
            data.szTip[..tip.len()].copy_from_slice(&tip);
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 && !self.picker.is_null() {
                ShowWindow(self.picker, SW_SHOW);
            }
        }
    }
    unsafe fn hotkey_warning(&self) {
        unsafe {
            message(
                self.hwnd,
                &format!(
                    "{} is unavailable (Windows or another app may reserve it).\nFor Win+C, Alt+C was also tried.\nOpen Settings from the tray menu and choose e.g. Ctrl+Alt+C.\nYou can still click the tray icon to open history.",
                    self.config.hotkey
                ),
            );
        }
    }
    unsafe fn refresh(&self) {
        if let Some(sender) = &self.sender {
            let items = self
                .history
                .items
                .iter()
                .map(|text| ui::Item {
                    preview: preview(text),
                })
                .collect();
            let _ = sender.send(ui::Message::History(items, self.config.hotkey.clone()));
        }
    }
    unsafe fn capture(&mut self) {
        unsafe {
            if self.copied_sequence == Some(GetClipboardSequenceNumber()) {
                return;
            }
            if IsClipboardFormatAvailable(CF_TEXT_UNICODE) == 0 {
                return;
            }
            if OpenClipboard(self.hwnd) == 0 {
                if self.retries < 10 {
                    self.retries += 1;
                    SetTimer(self.hwnd, RETRY, 50, None);
                }
                return;
            }
            self.retries = 0;
            let handle = GetClipboardData(CF_TEXT_UNICODE);
            if !handle.is_null() {
                let bytes = GlobalSize(handle);
                if (2..=MAX_TEXT_BYTES * 2 + 2).contains(&bytes) {
                    let ptr = GlobalLock(handle) as *const u16;
                    if !ptr.is_null() {
                        let units = std::slice::from_raw_parts(ptr, bytes / 2);
                        let len = units.iter().position(|&c| c == 0).unwrap_or(units.len());
                        self.history.push(String::from_utf16_lossy(&units[..len]));
                        GlobalUnlock(handle);
                    }
                }
            }
            CloseClipboard();
            self.refresh();
        }
    }
    unsafe fn show(&mut self) {
        unsafe {
            if self.picker.is_null() {
                return;
            }
            if IsWindowVisible(self.picker) != 0 {
                if SETTINGS_OPEN.with(Cell::get) {
                    SetForegroundWindow(self.picker);
                    return;
                }
                self.hide(true);
                return;
            }
            let foreground = GetForegroundWindow();
            if foreground != self.hwnd && foreground != self.picker {
                self.previous = foreground;
            }
            let monitor = MonitorFromWindow(self.previous, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let dpi = GetDpiForWindow(self.previous).max(96) as i32;
                let margin = 12 * dpi / 96;
                let width = (400 * dpi / 96)
                    .min(info.rcWork.right - info.rcWork.left - 2 * margin)
                    .max(1);
                let height = (420 * dpi / 96)
                    .min(info.rcWork.bottom - info.rcWork.top - 2 * margin)
                    .max(1);
                SetWindowPos(
                    self.picker,
                    HWND_TOP,
                    info.rcWork.right - width - margin,
                    info.rcWork.bottom - height - margin,
                    width,
                    height,
                    SWP_NOACTIVATE,
                );
            }
            self.refresh();
            ShowWindow(self.picker, SW_SHOW);
            SetForegroundWindow(self.picker);
            if let Some(sender) = &self.sender {
                let _ = sender.send(ui::Message::Opened);
            }
        }
    }
    unsafe fn hide(&self, restore: bool) {
        unsafe {
            if !self.picker.is_null() {
                ShowWindow(self.picker, SW_HIDE);
            }
            if restore && !self.previous.is_null() && IsWindow(self.previous) != 0 {
                SetForegroundWindow(self.previous);
            }
        }
    }
    unsafe fn copy(&mut self, index: usize) {
        unsafe {
            let Some(text) = self.history.items.get(index).cloned() else {
                return;
            };
            let units = wide(&text);
            let handle = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2);
            if handle.is_null() {
                message(self.hwnd, "Not enough memory to copy this item.");
                return;
            }
            let ptr = GlobalLock(handle) as *mut u16;
            if ptr.is_null() {
                GlobalFree(handle);
                return;
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), ptr, units.len());
            GlobalUnlock(handle);
            if OpenClipboard(self.hwnd) == 0 {
                GlobalFree(handle);
                message(self.hwnd, "Clipboard is busy. Press Enter to try again.");
                return;
            }
            let success =
                EmptyClipboard() != 0 && !SetClipboardData(CF_TEXT_UNICODE, handle).is_null();
            self.copied_sequence = Some(GetClipboardSequenceNumber());
            CloseClipboard();
            if !success {
                GlobalFree(handle);
                message(self.hwnd, "Could not copy this item. Please try again.");
                return;
            }
            // Windows owns the allocation after successful SetClipboardData.
            self.history.push(text);
            self.hide(true);
        }
    }
    unsafe fn reload(&mut self) {
        unsafe {
            if let Err(error) = config::load().and_then(|config| self.apply_config(config, false)) {
                message(self.hwnd, &format!("Settings were not changed: {error}"));
            }
        }
    }

    unsafe fn apply_config(&mut self, mut config: Config, persist: bool) -> Result<(), String> {
        unsafe {
            let requested = config.clone();
            let changed = !self.registered
                || (config.modifiers, config.key) != (self.config.modifiers, self.config.key);
            let new_id = if self.hotkey_id == 1 { 2 } else { 1 };
            // Reserve the replacement before releasing the current shortcut.
            if changed && !register_hotkey(self.hwnd, new_id, &mut config) {
                return Err(format!(
                    "{} is unavailable. Choose another shortcut. Your settings have not changed.",
                    requested.hotkey
                ));
            }
            if persist && let Err(error) = config::save(&requested) {
                if changed {
                    UnregisterHotKey(self.hwnd, new_id);
                }
                return Err(error);
            }
            if changed {
                if self.registered {
                    UnregisterHotKey(self.hwnd, self.hotkey_id);
                }
                self.hotkey_id = new_id;
                self.registered = true;
            }
            self.history.resize(config.history_limit);
            self.config = config;
            self.refresh();
            Shell_NotifyIconW(NIM_DELETE, &self.tray_data());
            self.add_tray();
            Ok(())
        }
    }
    unsafe fn menu(&mut self) {
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground != self.hwnd && foreground != self.picker {
                self.previous = foreground;
            }
            let menu = CreatePopupMenu();
            for (id, label) in [
                (SHOW, "Open history"),
                (SETTINGS, "Settings…"),
                (RELOAD, "Reload settings from file"),
                (CLEAR, "Clear history"),
                (EXIT, "Exit"),
            ] {
                AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
            }
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point);
            SetForegroundWindow(self.hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                self.hwnd,
                null(),
            );
            DestroyMenu(menu);
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            match command as usize {
                SHOW => self.show(),
                SETTINGS => {
                    if let Some(sender) = &self.sender {
                        let _ = sender.send(ui::Message::Settings);
                    }
                }
                RELOAD => self.reload(),
                CLEAR => {
                    self.history.items.clear();
                    self.refresh();
                }
                EXIT => {
                    if let Some(sender) = &self.sender {
                        let _ = sender.send(ui::Message::Exit);
                    }
                }
                _ => (),
            }
        }
    }
    unsafe fn cleanup(&self) {
        unsafe {
            if !self.hook.is_null() {
                UnhookWindowsHookEx(self.hook);
            }
            Shell_NotifyIconW(NIM_DELETE, &self.tray_data());
            UnregisterHotKey(self.hwnd, self.hotkey_id);
            RemoveClipboardFormatListener(self.hwnd);
            KillTimer(self.hwnd, RETRY);
            KillTimer(self.hwnd, updates::TIMER);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        STATE.with(|state| {
            let Ok(mut state) = state.try_borrow_mut() else {
                if msg == WM_CLIPBOARDUPDATE {
                    SetTimer(hwnd, RETRY, 50, None);
                    return 0;
                }
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            };
            let Some(app) = state.as_mut() else {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            };
            #[cfg(feature = "ui-smoke-test")]
            if msg == WM_TIMER && wparam == smoke::TIMER {
                smoke::tick(app);
                return 0;
            }
            if msg == app.taskbar_created {
                app.add_tray();
                return 0;
            }
            match msg {
                updates::CHANGED => {
                    if let Some(sender) = &app.sender {
                        let _ = sender.send(ui::Message::UpdatesChanged);
                    }
                    0
                }
                WM_TIMER if wparam == updates::TIMER => {
                    updates::check(hwnd as usize);
                    0
                }
                msg if msg == WM_APP + 2 => {
                    if IsWindowVisible(app.picker) == 0 {
                        app.show();
                    } else {
                        SetForegroundWindow(app.picker);
                    }
                    0
                }
                msg if msg == WM_APP + 3 => {
                    app.hide(false);
                    if std::env::args().any(|arg| arg == "--show") {
                        app.show();
                    }
                    0
                }
                WM_HOTKEY => {
                    app.show();
                    0
                }
                WM_CLIPBOARDUPDATE => {
                    KillTimer(hwnd, RETRY);
                    app.retries = 0;
                    app.capture();
                    0
                }
                WM_TIMER if wparam == RETRY => {
                    KillTimer(hwnd, RETRY);
                    app.capture();
                    0
                }
                TRAY => {
                    match lparam as u32 {
                        WM_LBUTTONUP => app.show(),
                        WM_RBUTTONUP | WM_CONTEXTMENU => app.menu(),
                        _ => (),
                    }
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        })
    }
}
