use std::{fs, path::PathBuf};

const DEFAULT: &str = "# PeetClip — reload settings from the tray after editing.\n# Modifiers: Win, Ctrl, Alt, Shift. Key: A-Z, 0-9, F1-F11.\nhotkey = Win+C\nhistory_limit = 50\n";

#[derive(Clone, Debug)]
pub struct Config {
    pub hotkey: String,
    pub modifiers: u32,
    pub key: u32,
    pub history_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Win+C".into(),
            modifiers: 8,
            key: b'C' as u32,
            history_limit: 50,
        }
    }
}

impl Config {
    pub fn from_fields(shortcut: &str, limit: &str) -> Result<Self, String> {
        let shortcut = shortcut.trim();
        let (modifiers, key) = hotkey(shortcut)?;
        let history_limit = limit
            .trim()
            .parse::<usize>()
            .map_err(|_| "Enter a whole number from 1 to 200.")?;
        if !(1..=200).contains(&history_limit) {
            return Err("History size must be between 1 and 200.".into());
        }
        Ok(Self {
            hotkey: shortcut
                .split('+')
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("+"),
            modifiers,
            key,
            history_limit,
        })
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    save_to(&path(), config).map_err(|e| format!("Could not save settings: {e}"))
}

fn save_to(path: &std::path::Path, config: &Config) -> std::io::Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        write!(
            file,
            "# PeetClip settings\nhotkey = {}\nhistory_limit = {}\n",
            config.hotkey, config.history_limit
        )?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PeetClip")
        .join("config.txt")
}

pub fn load() -> Result<Config, String> {
    let path = path();
    if !path.exists() {
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let legacy = path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("ClipboardManager/config.txt");
        if legacy.exists() {
            let text = fs::read_to_string(legacy).map_err(|e| e.to_string())?;
            let config = parse(&text)?;
            save_to(&path, &config).map_err(|e| e.to_string())?;
        } else {
            fs::write(&path, DEFAULT).map_err(|e| e.to_string())?;
        }
    }
    parse(&fs::read_to_string(path).map_err(|e| e.to_string())?)
}

fn parse(input: &str) -> Result<Config, String> {
    let mut config = Config::default();
    for line in input.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once('=').ok_or("Expected name = value")?;
        let value = value.trim();
        match name.trim() {
            "hotkey" => {
                let (modifiers, key) = hotkey(value)?;
                config.hotkey = value
                    .split('+')
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join("+");
                config.modifiers = modifiers;
                config.key = key;
            }
            "history_limit" => {
                config.history_limit = value.parse().map_err(|_| "Invalid history_limit")?;
                if !(1..=200).contains(&config.history_limit) {
                    return Err("history_limit must be between 1 and 200".into());
                }
            }
            other => return Err(format!("Unknown setting: {other}")),
        }
    }
    Ok(config)
}

fn hotkey(input: &str) -> Result<(u32, u32), String> {
    let parts: Vec<_> = input
        .split('+')
        .map(|s| s.trim().to_ascii_uppercase())
        .collect();
    let mut modifiers = 0;
    for part in &parts[..parts.len() - 1] {
        let flag = match part.as_str() {
            "ALT" => 1,
            "CTRL" => 2,
            "SHIFT" => 4,
            "WIN" => 8,
            _ => return Err(format!("Unknown modifier: {part}")),
        };
        if modifiers & flag != 0 {
            return Err("Repeated modifier".into());
        }
        modifiers |= flag;
    }
    if modifiers == 0 {
        return Err("Hotkey needs at least one modifier".into());
    }
    let key = parts.last().unwrap();
    let key = if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() {
        key.as_bytes()[0] as u32
    } else if let Some(n) = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u32>().ok())
        .filter(|n| (1..=11).contains(n))
    {
        0x70 + n - 1
    } else {
        return Err("Use A-Z, 0-9, or F1-F11 as the hotkey key".into());
    };
    Ok((modifiers, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_and_shortcuts() {
        let c = parse("hotkey = Ctrl+Alt+F11\nhistory_limit = 12").unwrap();
        assert_eq!((c.modifiers, c.key, c.history_limit), (3, 0x7a, 12));
        assert!(parse(DEFAULT).is_ok());
        for text in [
            "hotkey=C",
            "hotkey=Win+",
            "hotkey=Ctrl+F12",
            "hotkey=Ctrl+Ctrl+C",
            "history_limit=0",
            "unknown=yes",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn dialog_validation_and_save_round_trip() {
        for (key, limit) in [
            ("Alt+C", "0"),
            ("Alt+C", "201"),
            ("Alt+C", "1.5"),
            ("C", "50"),
            ("Alt+C\nhistory_limit=2", "50"),
        ] {
            assert!(Config::from_fields(key, limit).is_err());
        }
        let config = Config::from_fields(" Ctrl + Alt + F11 ", " 75 ").unwrap();
        let path = std::env::temp_dir().join(format!(
            "clipboard-settings-test-{}.txt",
            std::process::id()
        ));
        save_to(&path, &Config::default()).unwrap();
        save_to(&path, &config).unwrap();
        let saved = parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            (saved.modifiers, saved.key, saved.history_limit),
            (3, 0x7a, 75)
        );
        fs::remove_file(path).unwrap();
    }
}
