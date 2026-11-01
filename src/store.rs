/*
 * The file-backed store the game already uses under the Electron shell.
 *
 * The page speaks localStorage's five words (length / key / getItem /
 * setItem / removeItem) through its own seam; the desktop shell backs them
 * with real files in DFU's layout:
 *
 *   <root>/Saves/SAVE<n>/SaveData.txt     dagger.save.<n>
 *   <root>/Saves/SAVE<n>/SaveInfo.txt     dagger.saveinfo.<n>
 *   <root>/Saves/SAVE<n>/Screenshot.jpg   dagger.saveshot.<n>
 *   <root>/Prefs/<key>                    everything else
 *
 * The spelling rules, the canonical slot gate, the percent-encoded pref
 * names, the screenshot translation and temp-then-rename are copied from the
 * Electron shell's lib/fileStorage.cjs, because both shells must read the
 * same files while the comparison runs.
 */

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::Serialize;

const SAVE_DATA_PREFIX: &str = "dagger.save.";
const SAVE_INFO_PREFIX: &str = "dagger.saveinfo.";
const SAVE_SHOT_PREFIX: &str = "dagger.saveshot.";
const SAVES_DIR: &str = "Saves";
const PREFS_DIR: &str = "Prefs";
const DATA_FILE: &str = "SaveData.txt";
const INFO_FILE: &str = "SaveInfo.txt";
const TMP_MARK: &str = "~tmp";
const SHOT_FILES: [(&str, Option<&str>); 3] = [
    ("Screenshot.jpg", Some("image/jpeg")),
    ("Screenshot.png", Some("image/png")),
    ("Screenshot.dataurl", None),
];

/// The shell's userData folder - the same one the Electron shell pins.
pub fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("DAGGER_USER_DATA") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".config"));
    base.join("Daggerfall JavaScript")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Data,
    Info,
    Shot,
}

/// 'dagger.save.3' -> (Data, 3). Canonical integers only, as the shell's
/// slotOf: '03' is not a slot and lives in Prefs.
fn slot_of(key: &str) -> Option<(SlotKind, u32)> {
    for (prefix, kind) in [
        (SAVE_DATA_PREFIX, SlotKind::Data),
        (SAVE_INFO_PREFIX, SlotKind::Info),
        (SAVE_SHOT_PREFIX, SlotKind::Shot),
    ] {
        if let Some(tail) = key.strip_prefix(prefix) {
            let n: u32 = tail.parse().ok()?;
            if n.to_string() == tail {
                return Some((kind, n));
            }
        }
    }
    None
}

/* ------------------------------------------------------------- pref names */

/// A pref key as a filename. Mirrors encodePrefName: every char outside
/// [A-Za-z0-9._%-] becomes %XX (UTF-8 bytes, uppercase); a literal '%' is
/// kept; the empty key is a lone '%'; Windows device names and a trailing
/// dot get their first/last byte escaped.
fn encode_pref_name(key: &str) -> String {
    if key.is_empty() {
        return "%".into();
    }
    let mut out = String::new();
    for ch in key.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '%') {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            for b in s.as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    let escape_first = |s: &mut String| {
        let first = s.remove(0);
        let mut buf = [0u8; 4];
        let bytes = first.encode_utf8(&mut buf).as_bytes().to_vec();
        let mut prefix = String::new();
        for b in bytes {
            prefix.push_str(&format!("%{b:02X}"));
        }
        s.insert_str(0, &prefix);
    };
    // /^(CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])(\.|$)/i
    let head = out.split('.').next().unwrap_or("");
    let upper = head.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit());
    if reserved {
        escape_first(&mut out);
    }
    if out.ends_with('.') {
        let dot = out.pop().expect("checked");
        let mut buf = [0u8; 4];
        for b in dot.encode_utf8(&mut buf).bytes() {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// decodeURIComponent over the name; null when it is not valid encoding.
fn decode_pref_name(name: &str) -> Option<String> {
    if name == "%" {
        return Some(String::new());
    }
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/* ------------------------------------------------------------ data urls */

/// 'data:image/jpeg;base64,...' -> (mime, bytes).
fn parse_data_url(value: &str) -> Option<(&str, Vec<u8>)> {
    let rest = value.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let (mime, encoding) = meta.split_once(';')?;
    if !encoding.eq_ignore_ascii_case("base64") || !mime.contains('/') {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some((mime, bytes))
}

/* --------------------------------------------------------------- atomic */

fn write_atomic(file: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = file.as_os_str().to_os_string();
    tmp.push(format!("{TMP_MARK}{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, file)
}

/* ---------------------------------------------------------------- store */

pub struct Store {
    pub root: PathBuf,
}

#[derive(Serialize)]
pub struct All {
    pub keys: Vec<String>,
    pub values: BTreeMap<String, String>,
    pub saves_path: String,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn saves(&self) -> PathBuf {
        self.root.join(SAVES_DIR)
    }

    fn prefs(&self) -> PathBuf {
        self.root.join(PREFS_DIR)
    }

    /// The disk as a map, in the shell's own enumeration order: slots
    /// (ascending, data/info/shot), then Prefs by name.
    pub fn all(&self) -> All {
        let mut values = BTreeMap::new();
        let mut keys = Vec::new();

        let mut slots: Vec<u32> = Vec::new();
        if let Ok(entries) = fs::read_dir(self.saves()) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(digits) = name.strip_prefix("SAVE") else {
                    continue;
                };
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    continue;
                }
                let Ok(n) = digits.parse::<u32>() else {
                    continue;
                };
                if n.to_string() != digits {
                    continue; // 'SAVE03' is not canonical
                }
                slots.push(n);
            }
        }
        slots.sort_unstable();
        for n in slots {
            let dir = self.saves().join(format!("SAVE{n}"));
            if dir.join(DATA_FILE).is_file()
                && let Ok(v) = fs::read_to_string(dir.join(DATA_FILE))
            {
                keys.push(format!("{SAVE_DATA_PREFIX}{n}"));
                values.insert(format!("{SAVE_DATA_PREFIX}{n}"), v);
            }
            if dir.join(INFO_FILE).is_file()
                && let Ok(v) = fs::read_to_string(dir.join(INFO_FILE))
            {
                keys.push(format!("{SAVE_INFO_PREFIX}{n}"));
                values.insert(format!("{SAVE_INFO_PREFIX}{n}"), v);
            }
            if let Some(v) = self.shot_value(&dir) {
                keys.push(format!("{SAVE_SHOT_PREFIX}{n}"));
                values.insert(format!("{SAVE_SHOT_PREFIX}{n}"), v);
            }
        }

        let mut prefs: Vec<(String, String)> = Vec::new();
        if let Ok(entries) = fs::read_dir(self.prefs()) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if !entry.path().is_file() || name.contains(TMP_MARK) {
                    continue;
                }
                let Some(key) = decode_pref_name(name) else {
                    continue;
                };
                if slot_of(&key).is_some() {
                    continue; // a Prefs file spelt like a slot is a ghost
                }
                let Ok(v) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                prefs.push((key, v));
            }
        }
        prefs.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in prefs {
            keys.push(k.clone());
            values.insert(k, v);
        }

        All {
            keys,
            values,
            saves_path: self.saves().to_string_lossy().into_owned(),
        }
    }

    pub fn keys(&self) -> Vec<String> {
        self.all().keys
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if let Some((kind, n)) = slot_of(key) {
            let dir = self.saves().join(format!("SAVE{n}"));
            return match kind {
                SlotKind::Data => fs::read_to_string(dir.join(DATA_FILE)).ok(),
                SlotKind::Info => fs::read_to_string(dir.join(INFO_FILE)).ok(),
                SlotKind::Shot => self.shot_value(&dir),
            };
        }
        fs::read_to_string(self.prefs().join(encode_pref_name(key))).ok()
    }

    pub fn set(&self, key: &str, value: &str) -> io::Result<()> {
        if let Some((kind, n)) = slot_of(key) {
            let dir = self.saves().join(format!("SAVE{n}"));
            match kind {
                SlotKind::Data => write_atomic(&dir.join(DATA_FILE), value.as_bytes()),
                SlotKind::Info => write_atomic(&dir.join(INFO_FILE), value.as_bytes()),
                SlotKind::Shot => self.write_shot(&dir, value),
            }
        } else {
            write_atomic(&self.prefs().join(encode_pref_name(key)), value.as_bytes())
        }
    }

    pub fn remove(&self, key: &str) -> io::Result<()> {
        if let Some((kind, n)) = slot_of(key) {
            let dir = self.saves().join(format!("SAVE{n}"));
            match kind {
                SlotKind::Data => {
                    let _ = fs::remove_file(dir.join(DATA_FILE));
                }
                SlotKind::Info => {
                    let _ = fs::remove_file(dir.join(INFO_FILE));
                }
                SlotKind::Shot => {
                    for (name, _) in SHOT_FILES {
                        let _ = fs::remove_file(dir.join(name));
                    }
                }
            }
            self.sweep_slot(&dir);
            return Ok(());
        }
        let _ = fs::remove_file(self.prefs().join(encode_pref_name(key)));
        Ok(())
    }

    fn shot_value(&self, dir: &Path) -> Option<String> {
        let mut found: Vec<(&str, Option<&str>, std::time::SystemTime)> = Vec::new();
        for (name, mime) in SHOT_FILES {
            let path = dir.join(name);
            if let Ok(meta) = fs::metadata(&path)
                && let Ok(mtime) = meta.modified()
            {
                found.push((name, mime, mtime));
            }
        }
        found.sort_by_key(|entry| std::cmp::Reverse(entry.2)); // newest first, a crash's tiebreak
        let (name, mime, _) = found.into_iter().next()?;
        let bytes = fs::read(dir.join(name)).ok()?;
        match mime {
            Some(mime) => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                Some(format!("data:{mime};base64,{encoded}"))
            }
            None => String::from_utf8(bytes).ok(),
        }
    }

    fn write_shot(&self, dir: &Path, value: &str) -> io::Result<()> {
        let parsed = parse_data_url(value);
        let (target, bytes): (&str, Vec<u8>) = match parsed {
            Some((mime, bytes)) if mime.eq_ignore_ascii_case("image/png") => {
                ("Screenshot.png", bytes)
            }
            Some((mime, bytes)) if mime.eq_ignore_ascii_case("image/jpeg") => {
                ("Screenshot.jpg", bytes)
            }
            _ => ("Screenshot.dataurl", value.as_bytes().to_vec()),
        };
        write_atomic(&dir.join(target), &bytes)?;
        for (name, _) in SHOT_FILES {
            if name != target {
                let _ = fs::remove_file(dir.join(name));
            }
        }
        Ok(())
    }

    /// An emptied SAVE<n> folder goes with its last file; a crash's ~tmp
    /// leftovers are swept first, so they never hold it open.
    fn sweep_slot(&self, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut left = 0usize;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.contains(TMP_MARK) {
                let _ = fs::remove_file(entry.path());
            } else {
                left += 1;
            }
        }
        if left == 0 {
            let _ = fs::remove_dir(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pref_names_round_trip() {
        for key in [
            "dagger.settings.v1",
            "dagger.ui.v1",
            "weird key/with:chars",
            "",
            "CON",
            "trailing.",
        ] {
            let name = encode_pref_name(key);
            assert_eq!(
                decode_pref_name(&name).as_deref(),
                Some(key),
                "{key} -> {name}"
            );
        }
    }

    #[test]
    fn slots_are_canonical() {
        assert!(matches!(
            slot_of("dagger.save.3"),
            Some((SlotKind::Data, 3))
        ));
        assert!(slot_of("dagger.save.03").is_none());
        assert!(slot_of("dagger.save.-1").is_none());
        assert!(slot_of("dagger.save.x").is_none());
    }

    #[test]
    fn round_trip_in_a_temp_dir() {
        let dir = std::env::temp_dir().join(format!("dfo-store-test-{}", std::process::id()));
        let store = Store::new(dir.clone());
        store
            .set("dagger.settings.v1", "{\"Video\":{\"VSync\":false}}")
            .unwrap();
        store.set("dagger.save.0", "DATA").unwrap();
        store.set("dagger.saveinfo.0", "INFO").unwrap();
        store
            .set("dagger.saveshot.0", "data:image/jpeg;base64,AAEC")
            .unwrap();
        let all = store.all();
        assert_eq!(
            all.values.get("dagger.settings.v1").unwrap(),
            "{\"Video\":{\"VSync\":false}}"
        );
        assert_eq!(all.values.get("dagger.save.0").unwrap(), "DATA");
        assert_eq!(all.values.get("dagger.saveinfo.0").unwrap(), "INFO");
        assert!(
            all.values
                .get("dagger.saveshot.0")
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        assert_eq!(
            std::fs::read(dir.join("Saves/SAVE0/Screenshot.jpg")).unwrap(),
            vec![0, 1, 2]
        );
        let _ = store.remove("dagger.save.0");
        assert!(store.get("dagger.save.0").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
