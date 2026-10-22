//! The program's start (Python `main`): what the arguments ask for, and the steps that run before
//! the window opens. None of these steps stops the launcher.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use indexmap::IndexMap;

use crate::core::env::start_up_environment;
use crate::core::launch::python_command;
use crate::core::paths::Paths;
use crate::core::settings::load_app_settings;
use crate::core::update::{is_packaged, update_dir};
use crate::core::{desktop, legacy, platform};
use crate::APP_NAME;

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `--version`: the name and the version.
    Version,
    /// `--run`: the Rust runner.
    Run,
    /// `--play`: the Rust runner's play role.
    Play,
    /// `--script` or `--install-update`: the Python launcher does these.
    Python,
    /// Anything else: the window.
    Launcher,
}

/// `--run`, `--script` and `--install-update` count first (what follows them is theirs); `--play`
/// counts anywhere, as in the Python `main`.
pub fn route<S: AsRef<OsStr>>(args: &[S]) -> Route {
    let is = |arg: &S, flag: &str| arg.as_ref() == OsStr::new(flag);
    match args.first() {
        Some(first) if is(first, "--run") => return Route::Run,
        Some(first) if is(first, "--script") || is(first, "--install-update") => {
            return Route::Python;
        }
        _ => {}
    }
    if args.iter().any(|arg| is(arg, "--version")) {
        return Route::Version;
    }
    if args.iter().any(|arg| is(arg, "--play")) {
        return Route::Play;
    }
    Route::Launcher
}

/// Runs the Python launcher of `port_dir` with `args`, from `port_dir`, and returns its exit code;
/// 1 when it cannot start.
pub fn forward_to_python<S: AsRef<OsStr>>(port_dir: &Path, args: &[S]) -> i32 {
    let python = match python_command(&|key| std::env::var(key).ok()) {
        Ok(python) => python,
        Err(error) => {
            eprintln!("{APP_NAME}: {error}");
            return 1;
        }
    };
    let Some((program, interpreter_args)) = python.split_first() else {
        return 1;
    };
    let mut command = Command::new(program);
    // From Steam or a shortcut no console window opens with it; a terminal still gets its output.
    platform::no_window(&mut command);
    let status = command
        .args(interpreter_args)
        .arg(
            port_dir
                .join("launcher")
                .join("spartanport_launcher_win.py"),
        )
        .args(args)
        .current_dir(port_dir)
        .status();
    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("{APP_NAME}: {program}: {error}");
            1
        }
    }
}

/// `env::start_up_environment` on this process's environment, before anything reads it (the
/// Python does it on import). Runs while the program has a single thread.
pub fn forward_environment() {
    let mut env: IndexMap<String, String> = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let before = env.clone();
    start_up_environment(&mut env, cfg!(windows));
    for (key, value) in env {
        if before.get(&key) != Some(&value) {
            std::env::set_var(key, value);
        }
    }
}

/// The steps between the arguments and the window, in the Python `main`'s order: the old name's
/// files, the settings, the Linux menu entry and what a finished update left behind. The
/// language is the window's (`State::new`).
pub fn prepare(paths: &Paths) {
    // Python `WINDOWS and FROZEN and UPDATE_DIR not in PORT_DIR.parents`: the installed Windows
    // launcher, not the copy that runs from a downloaded update.
    let installed = cfg!(windows) && is_packaged() && !runs_from_update_dir(&paths.port_dir);
    // Before the settings are loaded: their copy must not mask the old data folder's.
    move_from_legacy_name(paths, installed);
    let settings = load_app_settings(paths);
    if !cfg!(windows) {
        let env = |key: &str| std::env::var(key).ok();
        let appimage = env("APPIMAGE");
        let icon = paths.port_dir.join("launcher").join("spartanport.png");
        desktop::integrate_desktop(
            settings.get_bool("desktop_integration"),
            appimage.as_deref(),
            &icon,
            &paths.data_dir,
            None,
            &env,
        );
    }
    if installed {
        // In use or already gone: tried again at the next start.
        let _ = std::fs::remove_dir_all(update_dir());
    }
}

/// Python `move_from_legacy_name`: settings, saves and shortcuts from before the rename; the
/// installed Windows launcher (`installed`) also cleans its folder and desktop shortcut. A failed
/// copy is tried again at the next start.
fn move_from_legacy_name(paths: &Paths, installed: bool) {
    for line in legacy::migrate(&paths.data_dir, None, cfg!(windows)) {
        println!("Rename: {line}");
    }
    if installed {
        legacy::remove_leftovers(&paths.port_dir);
        legacy::retarget_desktop_shortcut(&paths.port_dir, None, Some(&paths.data_dir));
    }
}

/// Python `UPDATE_DIR in PORT_DIR.parents`: this copy runs from a downloaded update. Both folders
/// are compared in their canonical form, so a short (8.3) temporary folder still matches.
fn runs_from_update_dir(port_dir: &Path) -> bool {
    let port = port_dir
        .canonicalize()
        .unwrap_or_else(|_| port_dir.to_path_buf());
    let update = update_dir();
    let update = update.canonicalize().unwrap_or(update);
    port != update && port.starts_with(&update)
}
