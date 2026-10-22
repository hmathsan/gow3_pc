//! The fonts egui draws with, in place of fastframe-fonts: the system's interface face, then
//! installed faces for the scripts it lacks (symbols, Chinese, Japanese, Korean), then egui's own.
//! The files are memory-mapped once, so a 20 MB CJK font costs no memory until it is drawn.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use egui::{FontData, FontDefinitions, FontFamily, FontTweak};

/// Chinese, Japanese and Korean share code points but not glyph shapes: the interface language's
/// own face comes first.
fn han_order<T>(lang: &str, [zh, ja, ko]: [T; 3]) -> [T; 3] {
    match lang {
        "ja" => [ja, zh, ko],
        "ko" => [ko, zh, ja],
        _ => [zh, ja, ko],
    }
}

/// The faces to try, in order: file, index in a collection, and scale.
// ponytail: fixed file names (the faces fastframe-fonts picks on Windows 11), no scan of the
// fonts folder; a missing face is skipped and egui's own fonts stay as the last fallback. Scan the
// folder if a Windows edition turns up without them.
#[cfg(windows)]
fn candidates(lang: &str) -> Vec<(PathBuf, u32, f32)> {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
    let dir = Path::new(&windir).join("Fonts");
    let mut faces = Vec::new();
    // Windows 11's interface face, Segoe UI Variable, has no Arabic: classic Segoe UI draws it,
    // 15 % larger so it reads as large as the Latin text (the ratio fastframe measures).
    let variable = dir.join("SegUIVar.ttf");
    let arabic_scale = if variable.exists() {
        faces.push((variable, 0, 1.0));
        1.15
    } else {
        1.0
    };
    faces.push((dir.join("segoeui.ttf"), 0, arabic_scale));
    // The ✓ ✗ ▾ marks.
    faces.push((dir.join("seguisym.ttf"), 0, 1.0));
    // Index 1: the interface cut of each collection (Microsoft YaHei UI, Yu Gothic UI).
    let han = han_order(
        lang,
        [("msyh.ttc", 1), ("YuGothM.ttc", 1), ("malgun.ttf", 0)],
    );
    faces.extend(han.map(|(file, index)| (dir.join(file), index, 1.0)));
    faces
}

/// fontconfig's answer for each script, the face the desktop itself would pick.
#[cfg(not(windows))]
fn candidates(lang: &str) -> Vec<(PathBuf, u32, f32)> {
    let han = han_order(lang, [":lang=zh-cn", ":lang=ja", ":lang=ko"]);
    ["sans-serif", ":lang=ar", "symbol"]
        .into_iter()
        .chain(han)
        .filter_map(|pattern| {
            let out = std::process::Command::new("fc-match")
                .args(["-f", "%{file}\n%{index}", pattern])
                .output()
                .ok()?;
            let text = String::from_utf8(out.stdout).ok()?;
            let (file, index) = text.split_once('\n')?;
            Some((PathBuf::from(file), index.trim().parse().unwrap_or(0), 1.0))
        })
        .collect()
}

/// The file's bytes, mapped once for the life of the process.
fn map(path: &Path) -> Option<&'static [u8]> {
    static MAPPED: Mutex<Vec<(PathBuf, &'static [u8])>> = Mutex::new(Vec::new());
    let mut mapped = MAPPED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, bytes)) = mapped.iter().find(|(known, _)| known == path) {
        return Some(bytes);
    }
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: the system's font files are only replaced by installers; egui reads them as is.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let bytes: &'static [u8] = Box::leak(Box::new(map));
    mapped.push((path.to_path_buf(), bytes));
    Some(bytes)
}

/// egui's fonts with the system's faces in front of its proportional family.
pub fn definitions(lang: &str) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut names = Vec::new();
    for (path, index, scale) in candidates(lang) {
        let name = format!("{}#{index}", path.display());
        if names.contains(&name) {
            continue;
        }
        let Some(bytes) = map(&path) else { continue };
        let data = FontData {
            index,
            tweak: FontTweak {
                scale,
                ..FontTweak::default()
            },
            ..FontData::from_static(bytes)
        };
        fonts.font_data.insert(name.clone(), Arc::new(data));
        names.push(name);
    }
    if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
        proportional.splice(0..0, names);
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_picks_its_own_han_face_first() {
        assert_eq!(han_order("ja", ["zh", "ja", "ko"]), ["ja", "zh", "ko"]);
        assert_eq!(han_order("ko", ["zh", "ja", "ko"]), ["ko", "zh", "ja"]);
        assert_eq!(han_order("en", ["zh", "ja", "ko"]), ["zh", "ja", "ko"]);
    }

    #[test]
    fn missing_faces_leave_egui_fonts_in_place() {
        let fonts = definitions("en");
        let proportional = &fonts.families[&FontFamily::Proportional];
        assert!(proportional.iter().any(|name| name == "Ubuntu-Light"));
    }
}
