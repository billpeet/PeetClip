# PeetClip

A keyboard-first Windows clipboard manager written in Rust using Microsoft's
**Windows Reactor 0.100 / WinUI 3**. Reactor owns the visible UI and message loop;
a small `windows-sys` layer handles the tray, clipboard notifications, global
shortcuts, and window placement. No webview or history database is used.

The compact, titlebar-free picker appears at the **bottom right of the active monitor**,
with a margin above the taskbar. Use Escape or switch focus to dismiss it.
WinUI supplies modern controls, a dense single-line list, a clear
accent-colored selection, and automatic Windows light/dark app theme support.
The clipboard icon is embedded in the executable and used by the window and tray.

## Install

Download **PeetClip-win-Setup.exe** from the [latest release](https://github.com/billpeet/PeetClip/releases/latest).
The Windows x64 installer includes the WinUI runtime and creates Start menu and
Startup shortcuts. PeetClip starts in the tray at sign-in. Disable it through
Windows Startup Apps or remove its shortcut from `shell:startup`.

Installed builds check this repository for updates at launch and every six hours.
Updates download in the background and apply on the next launch; Settings shows
the update status. Development builds do not auto-update.
The installer is currently unsigned.

## Build and run

Use Windows 10/11, Rust 1.98.1 with the MSVC toolchain, and Visual Studio C++ Build
Tools including the Windows SDK resource compiler, then:

```powershell
cargo build --release
.\target\release\peetclip.exe
```

The first build downloads and stages the Windows App Runtime using Microsoft's
`windows-reactor-setup`. The app is **self-contained**: distribute the executable
with the runtime DLLs, resources, and subdirectories staged beside it in
`target/release`. Copying only the executable is insufficient. WinUI has a larger
disk and memory footprint than the original Win32 list box implementation.

The app starts in the notification area. **Win+C** opens history; if Windows
rejects that shortcut, the app automatically uses **Alt+C**. Hover over its tray
icon to see the active shortcut. If both are occupied, a message explains how to
configure another shortcut; the tray icon still works.

Use `peetclip.exe --show` to open the window at startup. Launching a
second instance opens the existing instance instead of starting another listener.

| Key | Action |
| --- | --- |
| Up / Down | Select an item (newest first) |
| Enter | Copy the selected item and return to the previous window |
| 1–9 / numpad 1–9 | Copy that numbered item and return |
| Esc | Hide and return to the previous window |
| F2 | Open Settings |
| Global shortcut | Open / hide history |

Copying an item does **not** automatically paste it: use Ctrl+V in the destination.
Clicking a row also copies. Escape or switching away hides the window. Right-click
the tray icon for settings, clearing history, or exiting.

## Configuration

On first launch, the app creates `%APPDATA%\PeetClip\config.txt`:

```ini
hotkey = Win+C
history_limit = 50
```

Open **Settings** from the popup footer or tray menu (or press **F2**). Edit the
keyboard shortcut and history limit, then choose **Save** to persist and apply
them immediately. **Cancel** discards edits. Validation errors and shortcut
conflicts stay in the dialog, leaving the previous settings active. While the
dialog is open, typing and navigation keys belong to its controls; switching away
does not dismiss your edits. Escape dismisses the dialog.

You can also edit the configuration file directly and choose **Reload settings
from file** in the tray menu.
Modifiers are `Win`, `Alt`, `Ctrl`, and `Shift`; keys are A–Z, 0–9, or F1–F11.
For example, `hotkey = Alt+C` or `hotkey = Ctrl+Shift+V`. The history limit is
1–200. Invalid settings or conflicting shortcuts leave the running configuration
unchanged. The Win+C → Alt+C fallback also applies when reloading and does not
rewrite the configuration file.

## Scope

- Unicode plain text only; images, files, HTML, and rich formatting are not retained.
- History is in memory and disappears on exit. Duplicate text moves to the top.
- Each entry is limited to 1 MiB of UTF-8 text; oversized entries are skipped.
- Previews are shortened to 160 characters; copying preserves the full text,
  including line breaks.
- Clipboard changes use `AddClipboardFormatListener`; brief contention is retried
  with a short-lived timer. There is no polling while idle.
- Copied sensitive text can appear in history. There is no password detection or
  exclusion system. **Clear history** clears the app's history, not the clipboard.
- The installer registers launch at sign-in; development builds do not.

## Verification

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

For the opt-in live WinUI integration check (does not modify the system clipboard):

```powershell
cargo build --features ui-smoke-test
$test = Start-Process .\target\debug\peetclip.exe --smoke-test -PassThru -Wait
Get-Content .\target\debug\smoke-result.txt
```

Exit any running instance before this check. It verifies startup, actual WinUI
window attachment and keyboard focus, embedded icon loading, bottom-right
placement, navigation-key dispatch, Escape, reopen, close-to-tray, and graceful
exit. Test fixtures exist only in the test process's in-memory history.

Manual Windows smoke test:

1. Launch the app and copy three distinct texts, including multiline Unicode.
2. Open with the active global shortcut; check newest-first numbering.
3. Press Down then Enter; paste into an editor and check the exact text.
4. Reopen, press 2, and verify it copies the second item and restores focus.
5. Verify Esc and switching apps hide the window while capture continues.
6. Open Settings from the footer and tray; change to Alt+C, save, and verify the
   new shortcut. Try an invalid or occupied shortcut and verify the error remains
   visible and previous settings remain usable. Cancel and reopen to check edits
   were discarded. Restart after saving to verify persistence.
7. Clear history, verify the empty state, and copy new text.
8. Launch a second instance; verify only one process remains. Exit via the tray.
9. Check resizing, mixed-DPI monitors, and tray recovery after Explorer restarts.
10. Change Windows' app theme between light and dark while the picker is open;
    check the background, text, and selection colors.

Unit tests cover configuration validation and saving, history ordering, duplicate handling,
limits, and Unicode previews. The live integration check exercises the WinUI
lifecycle plus settings opening, validation, and cancellation. Visual inspection,
physical hotkey input, copying through the new UI,
and live system-theme switching still need a manual desktop check.

`assets/clipboard.ico` is the original multi-resolution application icon.
To regenerate it, run `python tools/generate_icon.py` with Pillow installed;
normal Rust builds use the checked-in icon and do not require Python.

## Releases

Every push to `main` runs formatting, tests, and Clippy, builds the self-contained
Windows x64 app, then publishes a GitHub release with a Velopack installer, portable
ZIP, update packages/feed, and SHA-256 checksums. Pull requests build and validate
without publishing. Release versions use Cargo's major/minor and the workflow run
number (`0.1.1`, `0.1.2`, ...).

To package locally, install the .NET 10 SDK and run:

```powershell
dotnet tool install vpk --tool-path ./target/tools --version 1.2.161
./tools/package.ps1 -Version 0.1.0 -Vpk ./target/tools/vpk.exe
```

Artifacts are written to `target/releases/0.1.0`. Settings from the earlier
`%APPDATA%\ClipboardManager` location are imported on first launch if needed.

## License

[MIT](LICENSE). Third-party dependencies retain their respective licenses.
