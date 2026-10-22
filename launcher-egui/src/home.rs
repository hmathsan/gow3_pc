//! The Home page: the layered background of spec 4.3 and the block over it, as the iced launcher
//! draws them (`ui/home.rs`), with egui's painter. The texts come from `core` the same way.

use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::text::LayoutJob;
use egui::{
    Color32, ColorImage, CornerRadius, FontFamily, FontId, Galley, Painter, Pos2, Rect, Sense,
    Stroke, StrokeKind, TextFormat, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2,
};
use spartanport_launcher::core::game::{
    GameInfo, game_info, game_patchable, game_profile_of, meta_folder,
};
use spartanport_launcher::core::i18n::{tr, tr_fmt};
use spartanport_launcher::core::options::{
    Choice, DISPLAY_MODES, FPS_LIMITS, OUTPUTS, short_label,
};
use spartanport_launcher::core::patches::patch_status;

use crate::{App, palette};

pub const CINZEL_BLACK: &str = "cinzel-black";
pub const CINZEL_BOLD: &str = "cinzel-bold";

pub fn cinzel(name: &str, size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(name.into()))
}

// --- Background (spec 4.3, the iced launcher's constants) ---

const BASE_TOP: Color32 = Color32::from_rgb(0x1b, 0x14, 0x14);
const RED_GLOW: [u8; 3] = [200, 30, 37];
const GREY_GLOW: [u8; 3] = [130, 130, 130];
const ART_OPACITY: f32 = 0.7;
const ART_ANCHOR: (f32, f32) = (0.70, 0.30);
const VEIL_ALPHA: f32 = 0.32;
const LEFT_VEIL_REACH: f32 = 0.58;
const LEFT_VEIL_STOPS: [(f32, f32); 3] = [(0.0, 0.88), (0.30, 0.45), (0.58, 0.0)];
const BOTTOM_RED: [u8; 3] = [140, 16, 20];
const FOG_RED: [u8; 3] = [110, 12, 16];
const HEAT_PERIOD: f32 = 2.8;
/// Heat blobs: centre, radii (shares of the scene), colour, peak alpha.
type HeatBlob = ((f32, f32), (f32, f32), [u8; 3], f32);
const HEAT_BLOBS: [HeatBlob; 3] = [
    ((0.52, 1.04), (0.45, 0.30), [235, 80, 30], 0.55),
    ((0.18, 1.10), (0.20, 0.20), [230, 90, 30], 0.30),
    ((0.84, 0.96), (0.30, 0.30), [235, 90, 30], 0.50),
];

// --- Block (mockup `#home`, `#topright`) ---

const LOGO_HEIGHT: f32 = 54.0;
const MONOGRAM: f32 = 60.0;
const MONOGRAM_EDGE: f32 = 24.0;
const BLOCK_RIGHT: f32 = 40.0;
const BLOCK_BOTTOM: f32 = 26.0;
const TOP_RIGHT: Vec2 = vec2(24.0, 22.0);

fn rgba([r, g, b]: [u8; 3], alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn bg(alpha: f32) -> Color32 {
    let [r, g, b, _] = palette::BG.to_array();
    rgba([r, g, b], alpha)
}

pub fn faded(colour: Color32, opacity: f32) -> Color32 {
    colour.gamma_multiply(opacity)
}

/// The pictures the Home draws, uploaded once.
pub struct Textures {
    art: TextureHandle,
    logo: TextureHandle,
    pub monogram: TextureHandle,
    pub glow: TextureHandle,
}

pub fn decode(png: &[u8]) -> ([usize; 2], Vec<u8>) {
    // The assets are embedded and checked by the iced launcher's tests.
    let image = image::load_from_memory(png)
        .map(|i| i.to_rgba8())
        .unwrap_or_default();
    (
        [image.width() as usize, image.height() as usize],
        image.into_raw(),
    )
}

impl Textures {
    pub fn load(ctx: &egui::Context) -> Self {
        let png = |name: &str, bytes: &[u8]| {
            let (size, pixels) = decode(bytes);
            ctx.load_texture(
                name,
                ColorImage::from_rgba_unmultiplied(size, &pixels),
                TextureOptions::LINEAR,
            )
        };
        // The glow of the iced launcher, white so a tint gives it its colour: `(1 - d²)²`.
        let side = 256;
        let half = side as f32 / 2.0;
        let mut glow = Vec::with_capacity(4 * side * side);
        for y in 0..side {
            for x in 0..side {
                let d2 = ((x as f32 + 0.5 - half) / half).powi(2)
                    + ((y as f32 + 0.5 - half) / half).powi(2);
                let alpha = if d2 >= 1.0 {
                    0.0
                } else {
                    (1.0 - d2) * (1.0 - d2)
                };
                glow.extend_from_slice(&[255, 255, 255, (alpha * 255.0).round() as u8]);
            }
        }
        Textures {
            art: png(
                "art",
                include_bytes!("../../launcher-rust/assets/background.png"),
            ),
            logo: png(
                "logo",
                include_bytes!("../../launcher-rust/assets/spartanport-logo.png"),
            ),
            monogram: png("monogram", crate::MONOGRAM_PNG),
            glow: ctx.load_texture(
                "glow",
                ColorImage::from_rgba_unmultiplied([side, side], &glow),
                TextureOptions::LINEAR,
            ),
        }
    }
}

/// A quad of four corners (clockwise from the top left) with a colour at each.
fn quad(mesh: &mut Mesh, corners: [Pos2; 4], colours: [Color32; 4]) {
    let first = mesh.vertices.len() as u32;
    for (pos, color) in corners.into_iter().zip(colours) {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color,
        });
    }
    mesh.add_triangle(first, first + 1, first + 2);
    mesh.add_triangle(first, first + 2, first + 3);
}

/// A gradient over `rect` along y (`vertical`) or x, between `stops` (share, colour).
fn gradient(mesh: &mut Mesh, rect: Rect, vertical: bool, stops: &[(f32, Color32)]) {
    for pair in stops.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if vertical {
            let (y0, y1) = (
                rect.top() + a * rect.height(),
                rect.top() + b * rect.height(),
            );
            let c = [
                pos2(rect.left(), y0),
                pos2(rect.right(), y0),
                pos2(rect.right(), y1),
                pos2(rect.left(), y1),
            ];
            quad(mesh, c, [ca, ca, cb, cb]);
        } else {
            let (x0, x1) = (
                rect.left() + a * rect.width(),
                rect.left() + b * rect.width(),
            );
            let c = [
                pos2(x0, rect.top()),
                pos2(x1, rect.top()),
                pos2(x1, rect.bottom()),
                pos2(x0, rect.bottom()),
            ];
            quad(mesh, c, [ca, cb, cb, ca]);
        }
    }
}

fn glow(painter: &Painter, tex: &Textures, centre: Pos2, radii: Vec2, colour: [u8; 3], peak: f32) {
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    painter.image(
        tex.glow.id(),
        Rect::from_center_size(centre, 2.0 * radii),
        uv,
        rgba(colour, peak),
    );
}

/// The part of the key art a `cover` fit shows in `scene`, as texture coordinates.
fn art_uv(picture: Vec2, scene: Vec2) -> Rect {
    let scale = (scene.x / picture.x).max(scene.y / picture.y);
    if !scale.is_finite() || scale <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    }
    let shown = (scene / scale).min(picture) / picture;
    let min = pos2(
        (1.0 - shown.x) * ART_ANCHOR.0,
        (1.0 - shown.y) * ART_ANCHOR.1,
    );
    Rect::from_min_size(min, shown)
}

/// The five layers, bottom to top: base and glows, key art, veils, fog, heat.
pub fn paint_background(painter: &Painter, tex: &Textures, scene: Rect, elapsed: f32) {
    let size = scene.size();
    let at = |(x, y): (f32, f32)| scene.min + vec2(x * size.x, y * size.y);
    let radii = |(x, y): (f32, f32)| vec2(x * size.x, y * size.y);
    let mut mesh = Mesh::default();
    gradient(
        &mut mesh,
        scene,
        true,
        &[(0.0, BASE_TOP), (1.0, palette::BG)],
    );
    painter.add(std::mem::take(&mut mesh));
    glow(
        painter,
        tex,
        at((0.68, 0.38)),
        radii((0.38, 0.46)),
        RED_GLOW,
        0.42,
    );
    glow(
        painter,
        tex,
        at((0.22, 0.78)),
        radii((0.25, 0.32)),
        GREY_GLOW,
        0.14,
    );
    painter.image(
        tex.art.id(),
        scene,
        art_uv(tex.art.size_vec2(), size),
        Color32::WHITE.gamma_multiply(ART_OPACITY),
    );
    painter.rect_filled(scene, 0.0, bg(VEIL_ALPHA));
    let reach = Rect::from_min_size(scene.min, vec2(LEFT_VEIL_REACH * size.x, size.y));
    let stops = LEFT_VEIL_STOPS.map(|(share, alpha)| (share / LEFT_VEIL_REACH, bg(alpha)));
    gradient(&mut mesh, reach, false, &stops);
    let red = |a| rgba(BOTTOM_RED, a);
    let floor = [
        (0.0, bg(0.6)),
        (0.28, bg(0.0)),
        (0.5, red(0.0)),
        (0.78, red(0.5)),
        (1.0, bg(0.98)),
    ];
    gradient(&mut mesh, scene, true, &floor);
    // Fog: 140 % × 70 % of the scene, its bottom edge 6 % below it, turned by -7 degrees about
    // that edge's middle; red to the floor colour along its height.
    let pivot = at((0.5, 1.06));
    let (sin, cos) = (-7.0f32).to_radians().sin_cos();
    let turned = |x: f32, y: f32| pivot + vec2(x * cos - y * sin, x * sin + y * cos);
    let (half, tall) = (0.7 * size.x, 0.70 * size.y);
    let rows = [
        (-tall, rgba(FOG_RED, 0.0)),
        (-tall / 2.0, rgba(FOG_RED, 0.38)),
        (0.0, bg(0.96)),
    ];
    for pair in rows.windows(2) {
        let ((y0, c0), (y1, c1)) = (pair[0], pair[1]);
        let corners = [
            turned(-half, y0),
            turned(half, y0),
            turned(half, y1),
            turned(-half, y1),
        ];
        quad(&mut mesh, corners, [c0, c0, c1, c1]);
    }
    painter.add(mesh);
    let pulse = 0.65 + 0.35 * (0.5 + 0.5 * (elapsed / HEAT_PERIOD * TAU).sin());
    for (centre, r, colour, peak) in HEAT_BLOBS {
        glow(painter, tex, at(centre), radii(r), colour, peak * pulse);
    }
}

// --- Texts (the iced launcher's `chips`, `status_line`, `quick_segments`) ---

/// What the chips read from the disk, read once so a frame does no I/O.
#[derive(Default)]
pub struct HomeFacts {
    info: Option<GameInfo>,
    title_id: Option<String>,
    patchable: bool,
    saves: bool,
    skipped: usize,
}

impl HomeFacts {
    pub fn read(app: &App) -> Self {
        let meta = meta_folder(
            &app.settings.get_str("game_dir"),
            &app.settings.get_str("update_dir"),
        );
        let savedata = app
            .paths
            .user_dir(&app.settings.get_str("user_dir"))
            .join("savedata");
        HomeFacts {
            info: game_info(&meta),
            title_id: game_profile_of(&meta).0,
            patchable: game_patchable(&meta),
            saves: has_files(&savedata),
            skipped: patch_status(&app.paths.data_dir.join("out")).map_or(0, |s| s.skipped.len()),
        }
    }
}

fn has_files(folder: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            has_files(&path)
        } else {
            path.is_file()
        }
    })
}

fn chips(app: &App) -> [String; 3] {
    let lang = app.lang.as_str();
    let facts = &app.home;
    // The spike runs no GPU probe.
    let gpu = tr(lang, "• GPU: not checked");
    let game = match &facts.info {
        Some(info) => {
            let id = facts.title_id.as_deref().unwrap_or(&info.title);
            format!("✓ {id} · v{}", info.version)
        }
        None => tr(lang, "✗ No eboot.bin in the game folder"),
    };
    let saves = if facts.saves {
        tr(lang, "✓ Saves found")
    } else {
        tr(lang, "• No saves yet")
    };
    [saves, game, gpu]
}

fn status_line(app: &App) -> String {
    let facts = &app.home;
    if facts.skipped > 0 && facts.info.is_some() && facts.patchable {
        tr_fmt(
            &app.lang,
            "{} patches not found in this eboot.bin at the last start (see Log)",
            &[&facts.skipped.to_string()],
        )
    } else {
        tr(
            &app.lang,
            "In the game: Insert or R3+L2\nopens the port's menu.",
        )
        .replace('\n', " ")
    }
}

fn choice_label(table: &[Choice], value: &str, lang: &str) -> String {
    table
        .iter()
        .find(|choice| choice.value == value)
        .map_or_else(|| value.to_string(), |choice| tr(lang, choice.en))
}

fn quick_segments(app: &App) -> Vec<(String, bool)> {
    let lang = app.lang.as_str();
    let ini = |key: &str| app.ini.values.get(key).map_or("", String::as_str);
    let fps = match ini("fps_limit") {
        "0" => choice_label(FPS_LIMITS, "0", lang),
        limit => format!("{limit} FPS"),
    };
    let monitor = app
        .settings
        .get_str("monitor")
        .parse::<u32>()
        .map_or(1, |i| i.saturating_add(1));
    let items = [
        (
            short_label(&choice_label(OUTPUTS, ini("output_res"), lang)).to_string(),
            true,
        ),
        (fps, true),
        (
            choice_label(DISPLAY_MODES, ini("display_mode"), lang),
            false,
        ),
        (format!("{} {monitor}", tr(lang, "Monitor")), false),
    ];
    let mut segments = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            segments.push((" · ".to_string(), false));
        }
        segments.push(item);
    }
    segments
}

// --- Drawing the block ---

/// One line of text, its runs in visual order when it mixes directions.
pub fn galley(ui: &Ui, text: String, font: FontId, colour: Color32) -> Arc<Galley> {
    let mut galley = ui.painter().layout_no_wrap(text, font, colour);
    crate::bidi::reorder(&mut galley);
    galley
}

/// A flat button (iced `button_flat`): card fill, 1 px edge that turns red on hover.
pub fn flat_button(
    ui: &mut Ui,
    rect: Rect,
    label: Arc<Galley>,
    opacity: f32,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(
        rect,
        ui.id().with(("flat", rect.min.x as i32, rect.min.y as i32)),
        sense,
    );
    let edge = if enabled && response.hovered() {
        palette::RED_HI
    } else {
        palette::LINE
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(4),
        faded(palette::CARD, opacity),
        Stroke::new(1.0, faded(edge, opacity)),
        StrokeKind::Inside,
    );
    painter.galley(
        rect.center() - label.size() / 2.0,
        label,
        faded(palette::TEXT, opacity),
    );
    response
}

pub enum HomeAction {
    Play,
    ToggleLanguages(Rect),
}

/// The corner monogram, the version and language buttons, and the block with the logotype, PLAY,
/// chips, settings line and status line; mirrored right to left.
pub fn block(ui: &mut Ui, app: &App, scene: Rect, enabled: bool) -> Option<HomeAction> {
    let opacity = if enabled { 1.0 } else { 0.35 };
    let rtl = app.rtl;
    let mut action = None;
    let painter = ui.painter().clone();

    let corner = if rtl {
        pos2(
            scene.right() - MONOGRAM_EDGE - MONOGRAM,
            scene.top() + MONOGRAM_EDGE,
        )
    } else {
        scene.min + Vec2::splat(MONOGRAM_EDGE)
    };
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    painter.image(
        app.textures.monogram.id(),
        Rect::from_min_size(corner, Vec2::splat(MONOGRAM)),
        uv,
        Color32::WHITE,
    );

    // Top corner: version then language, the language against the edge.
    let small = FontId::proportional(12.0);
    let version = galley(
        ui,
        app.installed_label.clone(),
        small.clone(),
        palette::TEXT,
    );
    let language = galley(
        ui,
        format!(
            "{} ▾",
            spartanport_launcher::core::i18n::native_name(&app.lang)
        ),
        small,
        palette::TEXT,
    );
    let pad = vec2(10.0, 3.0);
    let (vs, ls) = (version.size() + 2.0 * pad, language.size() + 2.0 * pad);
    let top = scene.top() + TOP_RIGHT.y;
    let (lang_x, version_x) = if rtl {
        let l = scene.left() + TOP_RIGHT.x;
        (l, l + ls.x + 8.0)
    } else {
        let l = scene.right() - TOP_RIGHT.x - ls.x;
        (l, l - 8.0 - vs.x)
    };
    flat_button(
        ui,
        Rect::from_min_size(pos2(version_x, top), vs),
        version,
        opacity,
        false,
    );
    let lang_rect = Rect::from_min_size(pos2(lang_x, top), ls);
    if flat_button(ui, lang_rect, language, opacity, enabled).clicked() {
        action = Some(HomeAction::ToggleLanguages(lang_rect));
    }

    // The block, laid out from the bottom up against the right edge (the left one mirrored).
    let edge = if rtl {
        scene.left() + BLOCK_RIGHT
    } else {
        scene.right() - BLOCK_RIGHT
    };
    let place = |width: f32| if rtl { edge } else { edge - width };
    let mut bottom = scene.bottom() - BLOCK_BOTTOM;

    let status = galley(
        ui,
        status_line(app),
        FontId::proportional(12.0),
        faded(palette::MUTED, opacity),
    );
    bottom -= status.size().y;
    painter.galley(pos2(place(status.size().x), bottom), status, palette::MUTED);

    let mut job = LayoutJob::default();
    for (piece, bright) in quick_segments(app) {
        let colour = if bright {
            palette::TEXT
        } else {
            palette::MUTED
        };
        job.append(
            &piece,
            0.0,
            TextFormat::simple(FontId::proportional(13.0), faded(colour, opacity)),
        );
    }
    let mut quick = ui.painter().layout_job(job);
    crate::bidi::reorder(&mut quick);
    bottom -= 10.0 + quick.size().y;
    painter.galley(pos2(place(quick.size().x), bottom), quick, palette::TEXT);

    let chip_pad = vec2(8.0, 2.0);
    let chips: Vec<_> = chips(app)
        .into_iter()
        .map(|label| {
            let colour = match label.chars().next() {
                Some('✓') => palette::GOLD,
                Some('✗') => palette::RED_HI,
                _ => palette::LINE,
            };
            (
                galley(
                    ui,
                    label,
                    FontId::proportional(12.0),
                    faded(palette::SILVER, opacity),
                ),
                colour,
            )
        })
        .collect();
    let chip_h = chips.iter().map(|(g, _)| g.size().y).fold(0.0, f32::max) + 2.0 * chip_pad.y;
    let row_w: f32 = chips
        .iter()
        .map(|(g, _)| g.size().x + 2.0 * chip_pad.x)
        .sum::<f32>()
        + 8.0 * 2.0;
    bottom -= 10.0 + chip_h;
    let mut x = place(row_w);
    for (label, colour) in chips {
        let rect = Rect::from_min_size(
            pos2(x, bottom),
            vec2(label.size().x + 2.0 * chip_pad.x, chip_h),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(1.0, faded(colour, opacity)),
            StrokeKind::Inside,
        );
        painter.galley(rect.min + chip_pad, label, palette::SILVER);
        x = rect.right() + 8.0;
    }

    // PLAY (iced `button_play`): red, a lighter red on hover.
    let play = galley(
        ui,
        tr(&app.lang, "PLAY"),
        cinzel(CINZEL_BLACK, 22.0),
        faded(palette::TEXT, opacity),
    );
    let play_size = play.size() + vec2(2.0 * 54.0, 2.0 * 16.0);
    bottom -= 14.0 + play_size.y;
    let play_rect = Rect::from_min_size(pos2(place(play_size.x), bottom), play_size);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(play_rect, ui.id().with("play"), sense);
    let fill = if enabled && response.hovered() {
        palette::RED_HI
    } else {
        palette::RED
    };
    painter.rect_filled(play_rect, CornerRadius::same(4), faded(fill, opacity));
    painter.galley(play_rect.center() - play.size() / 2.0, play, palette::TEXT);
    if response.clicked() {
        action = Some(HomeAction::Play);
    }

    let logo = app.textures.logo.size_vec2();
    let logo_size = vec2(logo.x * LOGO_HEIGHT / logo.y, LOGO_HEIGHT);
    bottom -= 14.0 + LOGO_HEIGHT;
    let logo_rect = Rect::from_min_size(pos2(place(logo_size.x), bottom), logo_size);
    painter.image(
        app.textures.logo.id(),
        logo_rect,
        uv,
        Color32::WHITE.gamma_multiply(opacity),
    );
    action
}
