// Spike: the launcher's shell (tab bar, drawer) and Home page on egui, over the same
// `core` as the iced launcher. The drawer pages, splash, setup, PLAY and saving are not ported.
#![windows_subsystem = "windows"]

mod bidi;
mod fire;
mod fonts;
mod home;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{
    CornerRadius, FontData, FontFamily, FontId, Key, Order, Pos2, Rect, Sense, Stroke, Ui, pos2,
    vec2,
};
use spartanport_launcher::core::i18n::{
    LANGS, is_rtl, launcher_language, native_name, system_language, tr,
};
use spartanport_launcher::core::ini::{Ini, read_ini};
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::platform;
use spartanport_launcher::core::settings::{Settings, read_app_settings};
use spartanport_launcher::core::update::{installed_release, stamp_path};
use spartanport_launcher::startup::{self, Route};
use spartanport_launcher::{APP_NAME, VERSION};

use crate::fire::{Fire, intensity_for};
use crate::home::{CINZEL_BLACK, CINZEL_BOLD, HomeAction, HomeFacts, Textures, cinzel};

/// The launcher's colours (spec section 3.3).
pub mod palette {
    use egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(0x0d, 0x0b, 0x0b);
    pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x12, 0x12);
    pub const CARD: Color32 = Color32::from_rgb(0x1d, 0x17, 0x17);
    pub const LINE: Color32 = Color32::from_rgb(0x2a, 0x22, 0x22);
    pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe0, 0xd8);
    pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x81, 0x7a);
    pub const RED: Color32 = Color32::from_rgb(0xa4, 0x16, 0x1a);
    pub const RED_HI: Color32 = Color32::from_rgb(0xc8, 0x1e, 0x25);
    pub const SILVER: Color32 = Color32::from_rgb(0xc9, 0xc4, 0xbd);
    pub const GOLD: Color32 = Color32::from_rgb(0xb8, 0x86, 0x2b);
}

pub static MONOGRAM_PNG: &[u8] = include_bytes!("../../launcher-rust/assets/spartanport.png");
const BAR_HEIGHT: f32 = 52.0;
const DRAWER_SLIDE: f32 = 0.22;
/// The iced launcher's clock while the fire burns.
const FIRE_FRAME: Duration = Duration::from_millis(33);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Graphics,
    Performance,
    Game,
    Trophies,
    Controls,
    Mods,
    Advanced,
    Log,
}

impl Page {
    const ALL: [Page; 9] = [
        Page::Home,
        Page::Graphics,
        Page::Performance,
        Page::Game,
        Page::Trophies,
        Page::Controls,
        Page::Mods,
        Page::Advanced,
        Page::Log,
    ];

    fn label_en(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Graphics => "Graphics",
            Page::Performance => "Performance",
            Page::Game => "Game",
            Page::Trophies => "Trophies",
            Page::Controls => "Controls",
            Page::Mods => "Mods",
            Page::Advanced => "Advanced",
            Page::Log => "Log",
        }
    }

    /// The drawer's share of the window (ruling R81).
    fn drawer_fraction(self) -> f32 {
        match self {
            Page::Performance | Page::Trophies | Page::Controls | Page::Mods | Page::Log => 0.72,
            _ => 0.46,
        }
    }

    fn step(self, by: isize) -> Page {
        let at = Page::ALL.iter().position(|p| *p == self).unwrap_or(0) as isize;
        Page::ALL[(at + by).rem_euclid(Page::ALL.len() as isize) as usize]
    }
}

pub struct App {
    pub paths: Paths,
    pub settings: Settings,
    pub ini: Ini,
    pub lang: String,
    pub rtl: bool,
    pub home: HomeFacts,
    pub textures: Textures,
    pub installed_label: String,
    page: Page,
    /// The page the drawer shows, kept while it slides out.
    drawer_page: Page,
    lang_menu: Option<Rect>,
    status: Option<String>,
    fire: Fire,
    effects: bool,
    started: Instant,
    last_frame: Instant,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, paths: Paths) -> Self {
        let (settings, _) = read_app_settings(&paths);
        let (ini, _) = read_ini(&paths.ini_path());
        let lang = launcher_language(&settings.get_str("ui_language"), system_language);
        set_fonts(&cc.egui_ctx, &lang);
        let effects = !matches!(
            settings.get("launcher_effects"),
            Some(serde_json::Value::Bool(false))
        );
        let appdir = std::env::var_os("APPDIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from);
        let installed = installed_release(&stamp_path(
            &paths.port_dir,
            appdir.as_deref(),
            cfg!(windows),
        ));
        let mut app = App {
            installed_label: installed.label(&lang),
            rtl: is_rtl(&lang),
            lang,
            paths,
            settings,
            ini,
            home: HomeFacts::default(),
            textures: Textures::load(&cc.egui_ctx),
            page: Page::Home,
            drawer_page: Page::Home,
            lang_menu: None,
            status: None,
            fire: Fire::default(),
            effects,
            started: Instant::now(),
            last_frame: Instant::now(),
        };
        app.home = HomeFacts::read(&app);
        app
    }

    fn show(&mut self, page: Page) {
        self.page = page;
        if page != Page::Home {
            self.drawer_page = page;
        }
    }

    fn set_language(&mut self, lang: &str) {
        self.lang = lang.to_string();
        self.rtl = is_rtl(lang);
        self.lang_menu = None;
    }
}

/// Cinzel for the titles and tabs, the system's faces for the rest (`fonts.rs`) behind it.
fn set_fonts(ctx: &egui::Context, lang: &str) {
    let mut fonts = fonts::definitions(lang);
    let faces: [(&str, &'static [u8]); 2] = [
        (
            CINZEL_BLACK,
            include_bytes!("../../launcher-rust/assets/Cinzel-Black.ttf"),
        ),
        (
            CINZEL_BOLD,
            include_bytes!("../../launcher-rust/assets/Cinzel-Bold.ttf"),
        ),
    ];
    let proportional = fonts.families[&FontFamily::Proportional].clone();
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
        let family = std::iter::once(name.to_string())
            .chain(proportional.iter().cloned())
            .collect();
        fonts.families.insert(FontFamily::Name(name.into()), family);
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(palette::TEXT);
    });
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        palette::BG.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let window = ui.max_rect();
        let scene = Rect::from_min_max(
            window.min,
            pos2(window.right(), window.bottom() - BAR_HEIGHT),
        );
        let (focused, esc, left, right) = ctx.input(|i| {
            (
                i.viewport().focused.unwrap_or(true),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
            )
        });
        if esc {
            if self.lang_menu.is_some() {
                self.lang_menu = None
            } else {
                self.show(Page::Home)
            }
        }
        if left || right {
            self.show(self.page.step(if right { 1 } else { -1 }));
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.fire
            .set_intensity(intensity_for(focused, self.effects));
        self.fire.tick(dt, scene.size());

        let painter = ui.painter().clone();
        let elapsed = now.duration_since(self.started).as_secs_f32();
        home::paint_background(&painter, &self.textures, scene, elapsed);
        self.fire
            .paint(&painter, scene.min, self.textures.glow.id());

        let open = self.page != Page::Home;
        let progress = ctx.animate_bool_with_time(ui.id().with("drawer"), open, DRAWER_SLIDE);
        if progress > 0.0 {
            painter.rect_filled(scene, 0.0, home::bg(0.45 * progress));
            if open
                && ui
                    .interact(scene, ui.id().with("dim"), Sense::click())
                    .clicked()
            {
                self.show(Page::Home);
            }
        }
        let wide = open && self.page.drawer_fraction() > 0.5;
        match home::block(ui, self, scene, !wide) {
            Some(HomeAction::Play) => {
                self.status = Some("PLAY is not wired in this spike.".into());
            }
            Some(HomeAction::ToggleLanguages(rect)) => {
                self.lang_menu = if self.lang_menu.is_some() {
                    None
                } else {
                    Some(rect)
                };
            }
            None => {}
        }
        if progress > 0.0 {
            self.drawer(ui, scene, progress);
        }
        self.bar(ui, window);
        self.language_menu(&ctx);

        if self.fire.burning() {
            ctx.request_repaint_after(FIRE_FRAME);
        }
    }
}

impl App {
    /// The drawer of the open page, sliding in from the left (from the right, right to left).
    fn drawer(&mut self, ui: &mut Ui, scene: Rect, progress: f32) {
        let fraction = ui.ctx().animate_value_with_time(
            ui.id().with("drawer-width"),
            self.drawer_page.drawer_fraction(),
            DRAWER_SLIDE,
        );
        let width = scene.width() * fraction;
        let offset = (progress - 1.0) * width;
        let x = if self.rtl {
            scene.right() - width - offset
        } else {
            scene.left() + offset
        };
        let rect = Rect::from_min_size(pos2(x, scene.top()), vec2(width, scene.height()));
        // Takes the clicks over it, so the dim layer under it does not close it.
        ui.interact(rect, ui.id().with("drawer-panel"), Sense::click());
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, palette::PANEL);
        let edge = if self.rtl {
            rect.left()
        } else {
            rect.right() - 1.0
        };
        painter.rect_filled(
            Rect::from_min_size(pos2(edge, rect.top()), vec2(1.0, rect.height())),
            0.0,
            palette::RED,
        );
        let title = home::galley(
            ui,
            tr(&self.lang, self.drawer_page.label_en()),
            cinzel(CINZEL_BLACK, 30.0),
            palette::TEXT,
        );
        let note = home::galley(
            ui,
            "Not ported in this spike.".into(),
            FontId::proportional(13.0),
            palette::MUTED,
        );
        let (tx, nx) = if self.rtl {
            (
                rect.right() - 28.0 - title.size().x,
                rect.right() - 28.0 - note.size().x,
            )
        } else {
            (rect.left() + 28.0, rect.left() + 28.0)
        };
        let below = rect.top() + 28.0 + title.size().y + 8.0;
        painter.galley(pos2(tx, rect.top() + 28.0), title, palette::TEXT);
        painter.galley(pos2(nx, below), note, palette::MUTED);
    }

    /// The bottom bar: the red line on top, the nine tabs and the version; mirrored right to left.
    fn bar(&mut self, ui: &mut Ui, window: Rect) {
        let rect = Rect::from_min_max(
            pos2(window.left(), window.bottom() - BAR_HEIGHT),
            window.max,
        );
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, palette::PANEL);
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 1.0)),
            0.0,
            palette::RED,
        );
        let label = self.status.clone().unwrap_or_else(|| {
            format!(
                "v{VERSION} · {}",
                if cfg!(windows) { "Windows" } else { "Linux" }
            )
        });
        let version = home::galley(ui, label, FontId::proportional(13.0), palette::MUTED);
        let tabs: Vec<_> = Page::ALL
            .into_iter()
            .map(|page| {
                (
                    page,
                    home::galley(
                        ui,
                        tr(&self.lang, page.label_en()),
                        cinzel(CINZEL_BOLD, 14.0),
                        palette::TEXT,
                    ),
                )
            })
            .collect();
        let tabs_width: f32 = tabs.iter().map(|(_, g)| g.size().x + 28.0).sum::<f32>()
            + 2.0 * (tabs.len() - 1) as f32;
        let (mut x, version_x) = if self.rtl {
            (rect.right() - 16.0 - tabs_width, rect.left() + 16.0)
        } else {
            (rect.left() + 16.0, rect.right() - 16.0 - version.size().x)
        };
        painter.galley(
            pos2(version_x, rect.center().y - version.size().y / 2.0),
            version,
            palette::MUTED,
        );
        for (page, label) in tabs {
            let tab = Rect::from_min_size(
                pos2(x, rect.top() + 1.0),
                vec2(label.size().x + 28.0, BAR_HEIGHT - 3.0),
            );
            let response = ui.interact(tab, ui.id().with(("tab", page.label_en())), Sense::click());
            let active = self.page == page;
            if active {
                painter.rect_filled(tab, 0.0, palette::CARD);
                painter.rect_filled(
                    Rect::from_min_size(pos2(tab.left(), tab.bottom()), vec2(tab.width(), 2.0)),
                    0.0,
                    palette::RED_HI,
                );
            }
            let colour = if active || response.hovered() {
                palette::RED_HI
            } else {
                palette::MUTED
            };
            painter.galley_with_override_text_color(
                tab.center() - label.size() / 2.0,
                label,
                colour,
            );
            if response.clicked() {
                self.show(page);
            }
            x = tab.right() + 2.0;
        }
    }

    /// The language list under its button; picking one switches the window to it (not saved).
    fn language_menu(&mut self, ctx: &egui::Context) {
        let Some(button) = self.lang_menu else { return };
        let width = 200.0;
        let x = if self.rtl {
            button.left()
        } else {
            button.right() - width
        };
        let mut picked = None;
        egui::Area::new(egui::Id::new("languages"))
            .order(Order::Foreground)
            .fixed_pos(Pos2::new(x, button.bottom() + 4.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette::CARD)
                    .stroke(Stroke::new(1.0, palette::LINE))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(6)
                    .show(ui, |ui| {
                        ui.set_width(width - 12.0);
                        for lang in LANGS {
                            let text = egui::RichText::new(native_name(lang)).size(12.5);
                            if ui.selectable_label(lang == self.lang, text).clicked() {
                                picked = Some(lang);
                            }
                        }
                    });
            });
        if let Some(lang) = picked {
            self.set_language(lang);
            set_fonts(ctx, lang);
        }
    }
}

fn run(paths: Paths) -> eframe::Result {
    let (size, rgba) = home::decode(MONOGRAM_PNG);
    let icon = egui::IconData {
        rgba,
        width: size[0] as u32,
        height: size[1] as u32,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpartanPort (PC)")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1280.0, 800.0])
            .with_icon(Arc::new(icon))
            .with_app_id("spartanport"),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc, paths)))),
    )
}

fn main() {
    platform::attach_parent_console();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    match startup::route(&args) {
        Route::Version => println!("{APP_NAME} {VERSION} (egui spike)"),
        Route::Python => {
            let paths = Paths::from_env(&exe_dir, &|key| std::env::var(key).ok());
            std::process::exit(startup::forward_to_python(&paths.port_dir, &args))
        }
        Route::Launcher => {
            startup::forward_environment();
            let paths = Paths::from_env(&exe_dir, &|key| std::env::var(key).ok());
            startup::prepare(&paths);
            if let Err(error) = run(paths) {
                eprintln!("{APP_NAME}: {error}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrows_wrap_around_the_tabs() {
        assert!(Page::Home.step(-1) == Page::Log);
        assert!(Page::Log.step(1) == Page::Home);
        assert!(Page::Home.step(1) == Page::Graphics);
    }
}
