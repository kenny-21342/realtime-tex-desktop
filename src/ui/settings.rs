// App settings: kept as a GPUI global, saved as JSON in
// <app data>/Realtime Tex Desktop/settings.json (~/Library/Application Support on the Mac), and edited in the Settings
// window (⌘,), built with ui/setting, our copy of gpui-kit's settings widget.
use crate::ui::Tip;
use crate::ui::keymap;
use crate::ui::palette;
use crate::ui::setting::{
    NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};
use crate::ui::themes;
use crate::ui::workspace::CloseTab;
use gpui_kit::base::Scrollbar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{CursorBlink, Input, InputEvent, InputState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme, Colorize, Disableable, Icon, Sizable, Theme, ThemeMode, ThemeRegistry, TitleBar,
    h_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    /// "system", "light" or "dark".
    pub theme: String,
    /// The accent of settings files from before each theme had its own: read, carried to the
    /// chosen themes (`migrated`), never written.
    #[serde(skip_serializing)]
    pub accent: String,
    /// The theme in light mode and in dark mode, by name.
    pub light_theme: String,
    pub dark_theme: String,
    /// Colours changed from a theme's own: theme name to colour key (as theme files write it) to
    /// "#RRGGBB" or "#RRGGBBAA". The theme's accent is among them (`themes::ACCENT`); the others
    /// win over it.
    pub colors: BTreeMap<String, BTreeMap<String, String>>,
    pub ui_font_size: f64,
    /// Pages on dark paper in the dark theme.
    pub dark_pages: bool,
    /// The caret blinks (the pages' and every text field's).
    pub cursor_blink: bool,
    /// The Source mode editor and the preamble strip, at 100% zoom.
    pub source_font_size: f64,
    /// Their font family; empty: Menlo.
    pub source_font: String,
    /// The source box on the page: a font family, or empty for the document's own font.
    pub source_box_font: String,
    /// The zoom a file opens at.
    pub default_zoom: Zoom,
    /// A .tex file opens in Visual mode (its pages) or Source mode.
    pub open_tex_in: TexView,
    /// A pinch on the trackpad zooms the pages.
    pub pinch_zoom: bool,
    /// A formula's typeset result under its source while it is edited.
    pub live_preview: bool,
    /// A `\` typed on the pages opens the paragraph's source there; off, it is typed as text.
    pub backslash_opens_source: bool,
    /// Enter in running text puts two line breaks (`\\\\`) in the same paragraph, so what
    /// follows has no first-line indent; off, it starts a paragraph (a blank line).
    pub enter_no_indent: bool,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    /// The LaTeX completion menu opens as you type.
    pub autocomplete: bool,
    /// Find starts out matching case.
    pub find_match_case: bool,
    /// At launch: the projects open when the app quit, or the welcome window.
    pub on_launch: OnLaunch,
    /// How many projects the recent list keeps.
    pub recent_kept: usize,
    /// rtex fetched at launch when it is nowhere the app looks.
    pub download_rtex: bool,
    /// The file tree shows names starting with a dot.
    pub show_hidden_files: bool,
    /// Names the file tree leaves out: comma-separated patterns (`*` any run, `?` one character).
    pub hide_files: String,
    /// The deepest heading the outline lists.
    pub outline_depth: OutlineDepth,
    /// Save a file `autosave_delay` seconds after the last edit.
    pub autosave: bool,
    pub autosave_delay: f64,
    /// Quitting saves and runs the full LaTeX compile `compile_runs` times first.
    pub compile_on_quit: bool,
    pub compile_runs: u32,
    /// Quitting also exports each document's PDF beside its .tex file (after the compile runs).
    pub export_on_quit: bool,
    /// Open an exported PDF in the default viewer.
    pub open_after_export: bool,
    /// rtex's fast-path budget per paragraph, in ms (`--fast-budget-ms`).
    pub fast_budget_ms: f64,
    /// Reuse pictures (TikZ) a full run drew before, rather than drawing each again.
    pub picture_cache: bool,
    /// Where rtex writes a bundle per engine failure (`--debug-dir`); empty: none.
    pub debug_dir: String,
    /// The rtex binary; empty finds it ($RTEX_BIN, PATH, the app's own copy).
    pub rtex_path: String,
    /// Folders with lualatex and kpsewhich, added to PATH.
    pub tex_bin: String,
    /// The ribbon folded to its tab row.
    pub ribbon_collapsed: bool,
    /// The sidebar on the right closed (it stays so until shown again).
    pub inspector_closed: bool,
    /// The tour was started once (the first launch starts it no more).
    pub tour_seen: bool,
    /// Shortcuts changed from their defaults: command to key (empty: none).
    pub shortcuts: BTreeMap<String, String>,
}

/// The zoom a file opens at: saved as a number, or null for fit width (a number is what
/// settings files from before fit width hold).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
pub enum Zoom {
    Percent(f64),
    FitWidth,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TexView {
    #[default]
    Visual,
    Source,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OnLaunch {
    #[default]
    Reopen,
    Welcome,
}

/// Each is how many heading levels the outline lists.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OutlineDepth {
    Sections = 1,
    Subsections = 2,
    #[default]
    Subsubsections = 3,
    Paragraphs = 4,
}

impl OutlineDepth {
    /// Whether the outline lists a heading of `level` (0: \section).
    pub fn shows(self, level: usize) -> bool {
        level < self as usize
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            accent: String::new(),
            light_theme: themes::DEFAULT_LIGHT.into(),
            dark_theme: themes::DEFAULT_DARK.into(),
            colors: BTreeMap::new(),
            ui_font_size: 16.0,
            dark_pages: true,
            cursor_blink: true,
            source_font_size: 13.0,
            source_font: String::new(),
            source_box_font: String::new(),
            default_zoom: Zoom::Percent(100.0),
            open_tex_in: TexView::Visual,
            pinch_zoom: true,
            live_preview: true,
            backslash_opens_source: true,
            enter_no_indent: false,
            line_numbers: true,
            soft_wrap: true,
            autocomplete: true,
            find_match_case: false,
            on_launch: OnLaunch::Reopen,
            recent_kept: 30,
            download_rtex: true,
            show_hidden_files: false,
            hide_files: String::new(),
            outline_depth: OutlineDepth::Subsubsections,
            autosave: false,
            autosave_delay: 1.0,
            compile_on_quit: false,
            compile_runs: 2,
            export_on_quit: false,
            open_after_export: false,
            fast_budget_ms: 50.0,
            picture_cache: true,
            debug_dir: String::new(),
            rtex_path: String::new(),
            tex_bin: crate::engine::default_tex_bin().into(),
            ribbon_collapsed: false,
            inspector_closed: false,
            tour_seen: false,
            shortcuts: Default::default(),
        }
    }
}

impl Global for AppSettings {}

/// The open documents' rtex restart that waits for the engine settings to settle; a newer one
/// replaces (and so cancels) it.
struct EngineRestart {
    _task: Task<()>,
}

impl Global for EngineRestart {}

/// Where the app keeps what it remembers.
pub fn support_dir() -> PathBuf {
    crate::engine::app_dir()
}

fn file() -> PathBuf {
    support_dir().join("settings.json")
}

impl AppSettings {
    /// Read the saved settings (defaults when there are none) and apply them.
    pub fn init(cx: &mut App) {
        let s = std::fs::read_to_string(file())
            .ok()
            .and_then(|t| serde_json::from_str::<AppSettings>(&t).ok())
            .unwrap_or_default()
            .migrated();
        cx.set_global::<AppSettings>(s);
        themes::init(cx);
        if let Err(e) = ThemeRegistry::watch_dir(themes::dir(), cx, |_| {}) {
            eprintln!("the themes folder: {e}");
        }
        crate::ui::workspace::place_notifications(cx);
        Self::apply(cx);
    }

    /// A settings file from before each theme had its own accent: its accent becomes the chosen
    /// light and dark themes' own where they have none, so nothing on screen changes.
    fn migrated(mut self) -> Self {
        let accent = std::mem::take(&mut self.accent);
        if palette::hex_color(&accent).is_some() {
            for theme in [self.light_theme.clone(), self.dark_theme.clone()] {
                (self.colors.entry(theme).or_default())
                    .entry(themes::ACCENT.into())
                    .or_insert_with(|| accent.clone());
            }
        }
        self
    }

    pub fn get(cx: &App) -> &AppSettings {
        cx.global::<AppSettings>()
    }

    /// Change, save and apply; every window redraws with the new values, and the open
    /// documents' rtex starts again on changed engine settings.
    pub fn set(cx: &mut App, f: impl FnOnce(&mut AppSettings)) {
        let engine_before = Self::get(cx).engine();
        f(cx.global_mut::<AppSettings>());
        if Self::get(cx).engine() != engine_before {
            // once they stop changing: a field sets them at each keystroke
            let restart = cx.spawn(async |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                cx.update(crate::ui::projects::restart_engines);
            });
            cx.set_global(EngineRestart { _task: restart });
        }
        let s = Self::get(cx);
        let path = file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // tests change settings too: never the real file
        if !cfg!(test) {
            let _ = std::fs::write(&path, serde_json::to_string_pretty(s).unwrap_or_default());
        }
        Self::apply(cx);
        cx.refresh_windows();
    }

    fn apply(cx: &mut App) {
        let s = Self::get(cx).clone();
        themes::install(cx);
        match s.theme.as_str() {
            "light" => Theme::change(ThemeMode::Light, None, cx),
            "dark" => Theme::change(ThemeMode::Dark, None, cx),
            _ => Theme::sync_system_appearance(None, cx),
        }
        Theme::update(cx, |t| {
            t.font_size = px(s.ui_font_size as f32);
            t.cursor_blink = if s.cursor_blink {
                CursorBlink::default()
            } else {
                CursorBlink::steady()
            };
        });
        crate::engine::configure(&s.rtex_path, &s.tex_bin);
        crate::engine::configure_serve(s.serve_args());
    }

    /// What `rtex serve` is started with.
    fn engine(&self) -> (String, String, Vec<String>) {
        (
            self.rtex_path.clone(),
            self.tex_bin.clone(),
            self.serve_args(),
        )
    }

    /// The family Source mode and the preamble strip show source in.
    pub fn source_family(&self) -> SharedString {
        match self.source_font.trim() {
            "" => crate::ui::editor::MONO.into(),
            f => f.to_string().into(),
        }
    }

    /// The options `rtex serve` gets from these settings.
    fn serve_args(&self) -> Vec<String> {
        let mut a = vec![
            "--fast-budget-ms".into(),
            (self.fast_budget_ms.round() as u64).to_string(),
        ];
        if !self.picture_cache {
            a.push("--no-picture-cache".into());
        }
        if !self.debug_dir.is_empty() {
            a.extend(["--debug-dir".into(), self.debug_dir.clone()]);
        }
        a
    }
}

/// The accents offered beside a colour of one's own: each named, and its setting ("" is the
/// theme's own blue, theme.json's).
pub(crate) const ACCENTS: [(&str, &str); 8] = [
    ("Blue", ""),
    ("Purple", "#7C4DDB"),
    ("Pink", "#C93A85"),
    ("Red", "#CF3A3A"),
    ("Orange", "#C2570C"),
    ("Green", "#1E7F47"),
    ("Teal", "#0E7C86"),
    ("Graphite", "#5B6472"),
];
pub(crate) const THEME_BLUE: &str = "#3557D4";

/// A ring around a swatch, in its colour when `on`.
fn ring(color: Hsla, on: bool) -> Div {
    div()
        .size(px(22.0))
        .rounded_full()
        .border_2()
        .border_color(if on { color } else { transparent_black() })
        .flex()
        .items_center()
        .justify_center()
}

struct AccentField {
    picker: Entity<ColorPickerState>,
    _sub: Subscription,
}

/// The accent of the theme on screen: a round swatch for each of ACCENTS, the one in use ringed,
/// then the color picker for any other colour, which shows that colour while it is the accent.
fn accent_field(options: &RenderOptions, window: &mut Window, cx: &mut App) -> Div {
    let current = themes::page_color(themes::ACCENT, cx)
        .map(palette::hex)
        .unwrap_or_default();
    let custom = !ACCENTS.iter().any(|(_, v)| *v == current);
    let state = window.use_keyed_state(field_key("accent", options), cx, |window, cx| {
        let picker = cx.new(|cx| ColorPickerState::new(window, cx));
        let _sub = cx.subscribe(&picker, |_, _, ev: &ColorPickerEvent, cx| {
            if let ColorPickerEvent::Change(Some(c)) = ev {
                themes::set_color(
                    themes::ACCENT,
                    Some(palette::hex(Hsla { a: 1.0, ..*c })),
                    cx,
                );
            }
        });
        AccentField { picker, _sub }
    });
    // the picker opens at the accent in use
    let shown = palette::hex_color(if current.is_empty() {
        THEME_BLUE
    } else {
        &current
    })
    .unwrap_or_default();
    let picker = state.read(cx).picker.clone();
    show_in_picker(&picker, Some(shown), window, cx);
    h_flex()
        .gap_0p5()
        .children(ACCENTS.map(|(name, value)| {
            let color = palette::hex_color(if value.is_empty() { THEME_BLUE } else { value })
                .unwrap_or_default();
            Button::new(SharedString::from(format!("accent-{name}")))
                .ghost()
                .p_0()
                .size(px(26.0))
                .rounded_full()
                .disabled(options.is_disabled())
                .accessibility_label(name)
                .tooltip(name)
                .child(
                    ring(color, !custom && current == value)
                        .child(div().size(px(14.0)).rounded_full().bg(color)),
                )
                .on_click(move |_, _, cx| {
                    let v = (!value.is_empty()).then(|| value.to_string());
                    themes::set_color(themes::ACCENT, v, cx)
                })
        }))
        .child(picker_button(
            &picker,
            ring(shown, custom).id("accent-custom").child(if custom {
                div()
                    .size(px(14.0))
                    .rounded_full()
                    .bg(shown)
                    .into_any_element()
            } else {
                Icon::default()
                    .path("icons/pipette.svg")
                    .size(px(14.0))
                    .into_any_element()
            }),
            "Custom color".into(),
            if custom {
                palette::hex(shown).into()
            } else {
                "Custom color".into()
            },
            window,
            cx,
        ))
}

/// A group of settings. The page leaves 1 rem beside its list for focus rings, but the
/// scrollbar's track, opaque while it shows, takes 16 px of the list's right edge: the group
/// keeps clear of it the 3 px ring of a control on its right edge, at every interface font size.
fn group(cx: &App) -> SettingGroup {
    let clear = Scrollbar::width() + px(4.0) - cx.theme().font_size;
    SettingGroup::new().pr(clear.max(px(0.0)))
}

/// The commands' groups in the order the keymap lists them, so no group is left off the page.
fn shortcut_groups(all: &[keymap::Command]) -> Vec<&'static str> {
    let mut groups = Vec::new();
    for c in all {
        if !groups.contains(&c.group) {
            groups.push(c.group);
        }
    }
    groups
}

/// Keyboard shortcuts: each command and its key; click the key and press a new one.
fn shortcuts_page(cx: &App) -> SettingPage {
    let all = keymap::commands();
    let mut page = SettingPage::new("Keyboard shortcuts");
    for name in shortcut_groups(&all) {
        let items: Vec<SettingItem> = all
            .iter()
            .filter(|c| c.group == name)
            .map(|c| {
                let (id, label) = (c.id, c.label);
                SettingItem::render(move |_, window, cx| shortcut_row(id, label, window, cx))
                    .keywords([label, "shortcut", "key"])
            })
            .collect();
        page = page.group(group(cx).title(name).items(items));
    }
    page.group(
        group(cx).item(
            SettingItem::render(|_, _, cx| {
                let changed = !AppSettings::get(cx).shortcuts.is_empty();
                div().flex().justify_end().child(
                    Button::new("shortcuts-reset-all")
                        .label("Reset all to defaults")
                        .small()
                        .outline()
                        .disabled(!changed)
                        .on_click(|_, window, cx| {
                            keymap::reset_all(cx);
                            window.refresh();
                        }),
                )
            })
            .keywords(["reset", "shortcuts"]),
        ),
    )
}

/// A command's row: its name, its key (click to record another), and reset; or, after a
/// recorded key another command has, Replace and Cancel.
fn shortcut_row(id: &'static str, label: &'static str, _: &mut Window, cx: &mut App) -> AnyElement {
    let all = keymap::commands();
    let Some(c) = all.iter().find(|c| c.id == id) else {
        return div().into_any_element();
    };
    let key = keymap::key_of(c, cx);
    let theme = cx.theme().clone();
    let rec = cx.try_global::<keymap::Recording>();
    let recording = rec.is_some_and(|r| r.id == Some(id));
    let clash = rec
        .and_then(|r| r.clash.clone())
        .filter(|(who, ..)| *who == id);
    let row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(div().text_sm().child(label));
    if let Some((_, new, other)) = clash {
        let other_label = all
            .iter()
            .find(|c| c.id == other)
            .map_or(other, |c| c.label);
        return row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(format!("{} is {other_label}'s", keymap::display(&new))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("sc-replace-{id}")))
                            .label("Replace")
                            .xsmall()
                            .primary()
                            .on_click(move |_, window, cx| {
                                cx.set_global(keymap::Recording::default());
                                keymap::set_key(other, "", cx);
                                keymap::set_key(id, &new, cx);
                                window.refresh();
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("sc-cancel-{id}")))
                            .label("Cancel")
                            .xsmall()
                            .outline()
                            .on_click(|_, window, cx| {
                                cx.set_global(keymap::Recording::default());
                                window.refresh();
                            }),
                    ),
            )
            .into_any_element();
    }
    let changed = key != c.key;
    let default = c.key.clone();
    row.child(
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("sc-{id}")))
                    .label(if recording {
                        "Press keys…".to_string()
                    } else {
                        keymap::display(&key)
                    })
                    .small()
                    .outline()
                    .w(px(120.0))
                    .tooltip("Click, then press the new shortcut (Escape cancels)")
                    .on_click(move |_, window, cx| {
                        keymap::record(id, cx);
                        window.refresh();
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("sc-reset-{id}")))
                    .icon(Icon::default().path("icons/undo.svg"))
                    .ghost()
                    .xsmall()
                    .tip("Back to the default")
                    .disabled(!changed)
                    .on_click(move |_, window, cx| {
                        keymap::set_key(id, &default, cx);
                        window.refresh();
                    }),
            ),
    )
    .into_any_element()
}

/// A switch for a setting.
fn switch(get: fn(&AppSettings) -> bool, set: fn(&mut AppSettings, bool)) -> SettingField<bool> {
    SettingField::switch(
        move |cx| get(AppSettings::get(cx)),
        move |v, cx| AppSettings::set(cx, |s| set(s, v)),
    )
    .default_value(get(&AppSettings::default()))
}

/// A number field for a setting, from `min` to `max` by `step`.
fn number(
    (min, max, step): (f64, f64, f64),
    get: fn(&AppSettings) -> f64,
    set: fn(&mut AppSettings, f64),
) -> SettingField<f64> {
    SettingField::number_input(
        NumberFieldOptions { min, max, step },
        move |cx| get(AppSettings::get(cx)),
        move |v, cx| AppSettings::set(cx, |s| set(s, v)),
    )
    .default_value(get(&AppSettings::default()))
}

/// A dropdown of `options`, each a value and its label.
fn choice<T: PartialEq + Copy + 'static, L: Into<SharedString>>(
    options: impl IntoIterator<Item = (T, L)>,
    get: fn(&AppSettings) -> T,
    set: fn(&mut AppSettings, T),
) -> SettingField<SharedString> {
    let (values, labels): (Vec<T>, Vec<SharedString>) =
        options.into_iter().map(|(v, l)| (v, l.into())).unzip();
    let values = Rc::new(values);
    // the dropdown's keys are the options' places in the list
    let key = {
        let values = values.clone();
        move |v: T| -> SharedString {
            let at = values.iter().position(|o| *o == v).unwrap_or_default();
            at.to_string().into()
        }
    };
    let default = key(get(&AppSettings::default()));
    let keys = labels
        .into_iter()
        .enumerate()
        .map(|(i, l)| (i.to_string().into(), l))
        .collect();
    SettingField::dropdown(
        keys,
        move |cx| key(get(AppSettings::get(cx))),
        move |k, cx| {
            if let Some(&v) = k.parse::<usize>().ok().and_then(|i| values.get(i)) {
                AppSettings::set(cx, |s| set(s, v));
            }
        },
    )
    .default_value(default)
}

/// The key of a field's own state: its kind and its place in the Settings window.
fn field_key(kind: &str, options: &RenderOptions) -> SharedString {
    format!(
        "{kind}-{}-{}-{}",
        options.page_ix(),
        options.group_ix(),
        options.item_ix()
    )
    .into()
}

/// `field`, which the page's reset puts back to the setting's default.
fn resettable(
    field: SettingField<SharedString>,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    field.on_reset(
        move |cx| get(AppSettings::get(cx)) != get(&AppSettings::default()),
        move |_, cx| AppSettings::set(cx, |s| set(s, get(&AppSettings::default()).clone())),
    )
}

struct TextField {
    input: Entity<InputState>,
    _sub: Subscription,
}

/// A text field whose placeholder says what empty means; the setting follows each keystroke,
/// trimmed.
fn text_input(
    placeholder: &'static str,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
    options: &RenderOptions,
    window: &mut Window,
    cx: &mut App,
) -> Input {
    let state = window.use_keyed_state(field_key("text", options), cx, |window, cx| {
        let value = get(AppSettings::get(cx)).clone();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(value)
        });
        let _sub = cx.subscribe(&input, move |_, input, ev: &InputEvent, cx| {
            let v = input.read(cx).value().trim().to_string();
            if matches!(ev, InputEvent::Change) && *get(AppSettings::get(cx)) != v {
                AppSettings::set(cx, |s| set(s, v));
            }
        });
        TextField { input, _sub }
    });
    let input = state.read(cx).input.clone();
    // changed elsewhere (the page's reset, Choose…): shown; a space being typed is kept
    let value = get(AppSettings::get(cx)).clone();
    if input.read(cx).value().trim() != value {
        input.update(cx, |s, cx| s.set_value(value, window, cx));
    }
    Input::new(&input)
        .disabled(options.is_disabled())
        .with_size(options.size())
}

/// A text setting's field.
fn text(
    placeholder: &'static str,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    let field = SettingField::render(move |options: &RenderOptions, window, cx| {
        let field = text_input(placeholder, get, set, options, window, cx);
        if options.layout() == Axis::Horizontal {
            field.w_64()
        } else {
            field.w_full()
        }
    });
    resettable(field, get, set)
}

/// A path's text field, and Choose… beside it for the system's file (or folder) chooser.
fn path(
    placeholder: &'static str,
    folder: bool,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    let field = SettingField::render(move |options: &RenderOptions, window, cx| {
        h_flex()
            .w_full()
            .gap_2()
            .child(text_input(placeholder, get, set, options, window, cx).flex_1())
            .child(
                Button::new(field_key("choose", options))
                    .label("Choose…")
                    .outline()
                    .with_size(options.size())
                    .disabled(options.is_disabled())
                    .on_click(move |_, _, cx| choose_path(folder, set, cx)),
            )
    });
    resettable(field, get, set)
}

/// The system's chooser, for a file or a folder; the one chosen becomes the setting.
fn choose_path(folder: bool, set: fn(&mut AppSettings, String), cx: &mut App) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: !folder,
        directories: folder,
        multiple: false,
        prompt: Some("Choose".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = rx.await
            && let Some(p) = paths.first()
        {
            let p = p.to_string_lossy().into_owned();
            cx.update(|cx| AppSettings::set(cx, |s| set(s, p)));
        }
    })
    .detach();
}

/// A family in a font setting's list, drawn in its own face; `name` "" is the setting's default.
#[derive(Clone)]
struct FontChoice {
    name: SharedString,
    label: SharedString,
    face: Option<SharedString>,
}

impl FontChoice {
    fn named(n: &str) -> Self {
        let n: SharedString = n.to_string().into();
        FontChoice {
            name: n.clone(),
            label: n.clone(),
            face: Some(n),
        }
    }

    fn preview(&self) -> Div {
        div()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .when_some(self.face.clone(), |d, f| d.font_family(f))
            .child(self.label.clone())
    }
}

impl SelectItem for FontChoice {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.preview().into_any_element())
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.preview()
    }

    fn value(&self) -> &SharedString {
        &self.name
    }
}

/// A font setting's list: its default first (`default`, drawn in `face`), then the installed
/// families but that face, then `current` when it is none of them (a font since removed).
fn font_choices(
    default: &str,
    face: Option<&str>,
    names: &[String],
    current: &str,
) -> Vec<FontChoice> {
    let mut list = vec![FontChoice {
        name: "".into(),
        label: default.to_string().into(),
        face: face.map(|f| f.to_string().into()),
    }];
    list.extend(
        names
            .iter()
            .filter(|n| Some(n.as_str()) != face)
            .map(|n| FontChoice::named(n)),
    );
    if !current.is_empty() && Some(current) != face && !names.iter().any(|n| n == current) {
        list.push(FontChoice::named(current));
    }
    list
}

type FontSelect = SelectState<SearchableVec<FontChoice>>;

struct FontField {
    select: Entity<FontSelect>,
    /// The setting the list last showed.
    shown: Option<String>,
    _sub: Subscription,
}

impl FontField {
    fn new(
        default: &'static str,
        face: Option<&'static str>,
        get: fn(&AppSettings) -> &String,
        set: fn(&mut AppSettings, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let names = crate::ui::font_names(cx);
        let items = font_choices(default, face, names, get(AppSettings::get(cx)));
        let select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true)
        });
        let _sub = cx.subscribe(&select, move |_, _, ev: &SelectEvent<_>, cx| {
            if let SelectEvent::Confirm(Some(name)) = ev
                && get(AppSettings::get(cx)) != name.as_ref()
            {
                let name = name.to_string();
                AppSettings::set(cx, |s| set(s, name));
            }
        });
        FontField {
            select,
            shown: None,
            _sub,
        }
    }
}

/// A searchable list of the installed fonts, each drawn in its face; the first, `default`
/// (drawn in `face`), leaves the setting empty.
fn font(
    default: &'static str,
    face: Option<&'static str>,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
) -> SettingField<SharedString> {
    let field = SettingField::render(move |options: &RenderOptions, window, cx| {
        let state = window.use_keyed_state(field_key("font", options), cx, |window, cx| {
            FontField::new(default, face, get, set, window, cx)
        });
        // changed elsewhere (the page's reset): picked in the list
        let value = get(AppSettings::get(cx)).clone();
        if state.read(cx).shown.as_ref() != Some(&value) {
            let pick: SharedString = match face {
                Some(f) if f == value => "".into(),
                _ => value.clone().into(),
            };
            let select = state.read(cx).select.clone();
            select.update(cx, |s, cx| s.set_selected_value(&pick, window, cx));
            state.update(cx, |f, _| f.shown = Some(value));
        }
        let select = Select::new(&state.read(cx).select)
            .disabled(options.is_disabled())
            .with_size(options.size())
            .menu_max_h(px(320.0));
        if options.layout() == Axis::Horizontal {
            select.w_64()
        } else {
            select.w_full()
        }
    });
    resettable(field, get, set)
}

/// A theme list for `mode`: the themes of that mode, the one in use picked (the app's own when
/// the chosen one is gone).
fn theme_list(
    mode: ThemeMode,
    get: fn(&AppSettings) -> &String,
    set: fn(&mut AppSettings, String),
    cx: &App,
) -> SettingField<SharedString> {
    let names = themes::names(mode, cx);
    SettingField::dropdown(
        names.into_iter().map(|n| (n.clone(), n)).collect(),
        move |cx| {
            let s = AppSettings::get(cx);
            themes::chosen(s, mode, cx).map_or_else(|| get(s).clone().into(), |t| t.name.clone())
        },
        move |v, cx| AppSettings::set(cx, |s| set(s, v.to_string())),
    )
    .default_value(get(&AppSettings::default()).clone())
}

/// `page` with the theme on screen: a group named after it with its accent, Export and Reset all,
/// and nested in it every colour of the theme, by group, each changeable. The page's reset puts
/// back this theme's accent and colours, as Reset all does, and no other theme's.
fn with_theme_colors(page: SettingPage, cx: &App) -> SettingPage {
    let name = themes::active(cx).unwrap_or_default();
    let shown = Rc::new(themes::shown(cx));
    let changed = AppSettings::get(cx).colors.contains_key(name.as_ref());
    // by the name this page shows: the page's reset may change the theme on screen first
    let has = move |name: &str, key: Option<&str>, cx: &App| {
        let colors = AppSettings::get(cx).colors.get(name);
        colors.is_some_and(|c| key.is_none_or(|k| c.contains_key(k)))
    };
    // `key` of theme `name` back to the theme's own; None: all of them
    let clear = move |name: &str, key: Option<&str>, cx: &mut App| {
        if has(name, key, cx) {
            AppSettings::set(cx, |s| {
                let c = s.colors.entry(name.to_string()).or_default();
                if let Some(k) = key {
                    c.remove(k);
                }
                if key.is_none() || c.is_empty() {
                    s.colors.remove(name);
                }
            });
        }
    };
    let reset = |key: Option<&'static str>| {
        let (n1, n2) = (name.clone(), name.clone());
        (
            move |cx: &App| has(&n1, key, cx),
            move |_: &mut Window, cx: &mut App| clear(&n2, key, cx),
        )
    };
    let (dirty, back) = reset(Some(themes::ACCENT));
    let accent = SettingItem::new(
        "Accent color",
        SettingField::render(accent_field).on_reset(dirty, back),
    )
    .keywords(["colour", "highlight"]);
    let (dirty, back) = reset(None);
    let shows = name.clone();
    let actions = SettingItem::render(move |_, _, _| {
        let shows = shows.clone();
        h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("colors-export")
                    .label("Export theme")
                    .small()
                    .outline()
                    .on_click(|_, _, cx| match themes::export(cx) {
                        Ok(path) => cx.reveal_path(&path),
                        Err(e) => eprintln!("export theme: {e}"),
                    }),
            )
            .child(
                Button::new("colors-reset-all")
                    .label("Reset all colors")
                    .small()
                    .outline()
                    .disabled(!changed)
                    .on_click(move |_, _, cx| clear(&shows, None, cx)),
            )
    })
    .keywords(["export", "reset", "theme"])
    .on_reset(dirty, back);
    let colors = themes::groups().iter().map(|g| {
        let items = g.keys.iter().map(|k| {
            let (key, label) = (k.key.clone(), SharedString::from(k.label.clone()));
            let color = shown.get(&k.key).copied().flatten();
            SettingItem::new(
                k.label.clone(),
                SettingField::render(move |_: &RenderOptions, window, cx| {
                    color_field(&key, &label, color, window, cx)
                }),
            )
            .keywords([k.key.clone()])
        });
        group(cx).title(g.title).items(items.collect::<Vec<_>>())
    });
    page.group(
        group(cx)
            .title(name)
            .items([accent, actions])
            .groups(colors),
    )
}

struct ColorRow {
    picker: Entity<ColorPickerState>,
    _sub: Subscription,
}

/// A colour's field: its value, a swatch that opens the colour picker (transparency shows over a
/// checkerboard), and, while it is changed, reset to the theme's own. "Auto": a source colour the
/// theme leaves to the editor.
fn color_field(
    key: &str,
    label: &SharedString,
    color: Option<Hsla>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    // keyed by the colour, not the row's place: a search moves rows
    let state = window.use_keyed_state(SharedString::from(format!("color-{key}")), cx, {
        let key = key.to_string();
        move |window, cx| {
            let picker = cx.new(|cx| ColorPickerState::new(window, cx));
            let _sub = cx.subscribe(&picker, move |_, _, ev: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    themes::set_color(&key, Some(palette::hex(*c)), cx);
                }
            });
            ColorRow { picker, _sub }
        }
    });
    let picker = state.read(cx).picker.clone();
    #[cfg(test)]
    tests::COLOR_ROWS.with(|r| r.borrow_mut().insert(key.to_string(), picker.clone()));
    show_in_picker(&picker, color, window, cx);
    let changed = themes::active(cx).is_some_and(|name| {
        let colors = &AppSettings::get(cx).colors;
        colors
            .get(name.as_ref())
            .is_some_and(|c| c.contains_key(key))
    });
    let theme = cx.theme();
    let name: SharedString = color.map_or("Auto".into(), palette::hex).into();
    h_flex()
        .gap_2()
        .when(changed, |row| {
            let key = key.to_string();
            row.child(
                Button::new(SharedString::from(format!("color-reset-{key}")))
                    .icon(Icon::default().path("icons/undo.svg"))
                    .ghost()
                    .xsmall()
                    .tip("Back to the theme's color")
                    .on_click(move |_, _, cx| themes::set_color(&key, None, cx)),
            )
        })
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(name.clone()),
        )
        .child(
            picker_button(
                &picker,
                div()
                    .id(SharedString::from(format!("swatch-{key}")))
                    .size_8()
                    .rounded(theme.radius)
                    .bg(checkerboard(theme.muted_foreground.opacity(0.4), 4.0))
                    .child(
                        div()
                            .size_full()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(color.map_or(theme.input, |c| c.darken(0.3)))
                            .bg(color.unwrap_or(theme.background)),
                    ),
                label.clone(),
                name,
                window,
                cx,
            )
            .test_support(),
        )
}

/// `face`, a colour's button, opening `picker` on its left. The kit's own button opens it below,
/// or, short of room there, pushes it back up over the button (the anchor asked of it is not
/// passed on), the pointer that opened it already on a colour, which a second click picks. Here
/// the kit's button is a bare line reaching the picker's width (18 rem) left of `face`, and the
/// picker opens below that line, beside `face`.
fn picker_button(
    picker: &Entity<ColorPickerState>,
    face: Stateful<Div>,
    name: SharedString,
    tip: SharedString,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    // as this frame drew it: a click on `face` that closed it, being outside it, leaves it so
    let open = picker.read(cx).is_open();
    let toggle = picker.clone();
    face.relative()
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            if !open {
                toggle.update(cx, |p, cx| p.set_open(true, cx));
            }
        })
        .child(
            ColorPicker::new(picker)
                .accessibility_label(name)
                .icon(
                    Icon::default()
                        .path("icons/pipette.svg")
                        .text_color(transparent_black())
                        .w(window.rem_size() * 18.0 + px(4.0))
                        .h(px(1.0)),
                )
                .absolute()
                .top_0()
                .right_full(),
        )
}

/// `picker` at `color`, so it opens at it: the kit keeps the colour hovered last as its preview
/// once closed. While it is open only a change from elsewhere (a reset) moves it, not its own
/// slider being dragged.
fn show_in_picker(
    picker: &Entity<ColorPickerState>,
    color: Option<Hsla>,
    window: &mut Window,
    cx: &mut App,
) {
    let p = picker.read(cx);
    let stale = p.value().map(palette::hex) != color.map(palette::hex)
        || !p.is_open() && p.preview() != p.value();
    if stale {
        picker.update(cx, |p, cx| match color {
            Some(c) => p.set_value(c, window, cx),
            None => p.clear_value(window, cx),
        });
    }
}

/// Clear: the build files of the documents not open now removed, in the background (an open
/// document's rtex is using its own).
fn clear_cache_button(options: &RenderOptions, window: &mut Window, cx: &mut App) -> Button {
    let cleared = window.use_keyed_state("cache-cleared", cx, |_, _| false);
    let done = *cleared.read(cx);
    Button::new("clear-cache")
        .label(if done { "Cleared" } else { "Clear" })
        .outline()
        .with_size(options.size())
        .disabled(done)
        .on_click(move |_, window, cx| {
            let keep = crate::ui::projects::open_build_dirs(cx);
            cx.background_executor()
                .spawn(async move { crate::engine::clear_build_cache(&keep) })
                .detach();
            cleared.update(cx, |c, _| *c = true);
            window.refresh();
        })
}

/// The Settings window's pages, the most used first on each.
fn pages(cx: &App) -> Vec<SettingPage> {
    let s = AppSettings::get(cx);
    let icon = |name: &str| Icon::default().path(format!("icons/{name}.svg"));
    let general = SettingPage::new("General")
        .icon(icon("sliders-horizontal"))
        .group(group(cx).title("Startup").items([
            SettingItem::new(
                "On launch",
                choice(
                    [
                        (OnLaunch::Reopen, "Reopen last projects"),
                        (OnLaunch::Welcome, "Show the Welcome window"),
                    ],
                    |s| s.on_launch,
                    |s, v| s.on_launch = v,
                ),
            ),
            SettingItem::new(
                "Recent projects kept",
                number(
                    (5.0, 100.0, 1.0),
                    |s| s.recent_kept as f64,
                    |s, v| s.recent_kept = v.round() as usize,
                ),
            ),
        ]))
        .group(
            group(cx).title("Saving").items([
                SettingItem::new("Auto-save", switch(|s| s.autosave, |s, v| s.autosave = v)),
                SettingItem::new(
                    "Auto-save delay (seconds)",
                    number(
                        (0.5, 60.0, 0.5),
                        |s| s.autosave_delay,
                        |s, v| s.autosave_delay = v,
                    ),
                )
                .disabled(!s.autosave),
                SettingItem::new(
                    "Compile when quitting",
                    switch(|s| s.compile_on_quit, |s, v| s.compile_on_quit = v),
                ),
                SettingItem::new(
                    "Compile runs when quitting",
                    number(
                        (1.0, 5.0, 1.0),
                        |s| s.compile_runs as f64,
                        |s, v| s.compile_runs = (v.round() as u32).clamp(1, 5),
                    ),
                )
                .disabled(!s.compile_on_quit),
            ]),
        )
        .group(group(cx).title("Export").items([
            SettingItem::new(
                "Open the PDF after export",
                switch(|s| s.open_after_export, |s, v| s.open_after_export = v),
            ),
            SettingItem::new(
                "Export the PDF when quitting",
                switch(|s| s.export_on_quit, |s, v| s.export_on_quit = v),
            ),
        ]));
    let appearance = SettingPage::new("Appearance")
        .icon(icon("palette"))
        .group(
            group(cx).title("Window").items([
                SettingItem::new(
                    "Theme",
                    SettingField::dropdown(
                        vec![
                            ("system".into(), "System".into()),
                            ("light".into(), "Light".into()),
                            ("dark".into(), "Dark".into()),
                        ],
                        |cx| AppSettings::get(cx).theme.clone().into(),
                        |v, cx| AppSettings::set(cx, |s| s.theme = v.to_string()),
                    )
                    .default_value(AppSettings::default().theme),
                ),
                SettingItem::new(
                    "Light theme",
                    theme_list(
                        ThemeMode::Light,
                        |s| &s.light_theme,
                        |s, v| s.light_theme = v,
                        cx,
                    ),
                ),
                SettingItem::new(
                    "Dark theme",
                    theme_list(
                        ThemeMode::Dark,
                        |s| &s.dark_theme,
                        |s, v| s.dark_theme = v,
                        cx,
                    ),
                ),
                SettingItem::new(
                    "Themes folder",
                    SettingField::render(|options: &RenderOptions, _, _| {
                        Button::new("themes-folder")
                            .label("Open")
                            .outline()
                            .with_size(options.size())
                            .on_click(|_, _, cx| {
                                let dir = themes::dir();
                                let _ = std::fs::create_dir_all(&dir);
                                cx.open_with_system(&dir);
                            })
                    }),
                ),
                SettingItem::new(
                    "Interface font size",
                    number(
                        (10.0, 24.0, 1.0),
                        |s| s.ui_font_size,
                        |s, v| s.ui_font_size = v,
                    ),
                ),
                SettingItem::new(
                    "Dark pages",
                    switch(|s| s.dark_pages, |s, v| s.dark_pages = v),
                ),
                SettingItem::new(
                    "Blinking caret",
                    switch(|s| s.cursor_blink, |s, v| s.cursor_blink = v),
                )
                .keywords(["cursor"]),
            ]),
        )
        .group(
            group(cx).title("Layout").items([
                SettingItem::new(
                    "Show ribbon tools",
                    switch(|s| !s.ribbon_collapsed, |s, v| s.ribbon_collapsed = !v),
                )
                .keywords(["toolbar"]),
                SettingItem::new(
                    "Show sidebar",
                    switch(|s| !s.inspector_closed, |s, v| s.inspector_closed = !v),
                )
                .keywords(["inspector"]),
            ]),
        );
    let appearance = with_theme_colors(appearance, cx);
    let mut zooms: Vec<(Zoom, SharedString)> = [50, 75, 100, 125, 150, 200]
        .map(|p| (Zoom::Percent(p.into()), format!("{p}%").into()))
        .into();
    zooms.insert(0, (Zoom::FitWidth, "Fit width".into()));
    // a zoom typed in before there was this list
    if let Zoom::Percent(p) = s.default_zoom
        && !zooms.iter().any(|z| z.0 == s.default_zoom)
    {
        zooms.push((s.default_zoom, format!("{p}%").into()));
    }
    let editor = SettingPage::new("Editor")
        .icon(icon("pencil-line"))
        .group(
            group(cx).title("Pages").items([
                SettingItem::new(
                    "Open files at",
                    choice(zooms, |s| s.default_zoom, |s, v| s.default_zoom = v),
                )
                .keywords(["zoom"]),
                SettingItem::new(
                    "Open .tex files in",
                    choice(
                        [(TexView::Visual, "Visual"), (TexView::Source, "Source")],
                        |s| s.open_tex_in,
                        |s, v| s.open_tex_in = v,
                    ),
                ),
                SettingItem::new(
                    "Live formula preview",
                    switch(|s| s.live_preview, |s, v| s.live_preview = v),
                ),
                SettingItem::new(
                    "Enter makes a gap with \\\\\\\\ (no first-line indent), not a paragraph",
                    switch(|s| s.enter_no_indent, |s, v| s.enter_no_indent = v),
                ),
                SettingItem::new(
                    "Typing \\ opens the source",
                    switch(
                        |s| s.backslash_opens_source,
                        |s, v| s.backslash_opens_source = v,
                    ),
                )
                .keywords(["backslash"]),
                SettingItem::new(
                    "Pinch to zoom",
                    switch(|s| s.pinch_zoom, |s, v| s.pinch_zoom = v),
                ),
            ]),
        )
        .group(
            group(cx).title("Source").items([
                SettingItem::new(
                    "Source font",
                    font(
                        crate::ui::editor::MONO,
                        Some(crate::ui::editor::MONO),
                        |s| &s.source_font,
                        |s, v| s.source_font = v,
                    ),
                ),
                SettingItem::new(
                    "Source font size",
                    number(
                        (9.0, 28.0, 1.0),
                        |s| s.source_font_size,
                        |s, v| s.source_font_size = v,
                    ),
                ),
                SettingItem::new(
                    "Source box font",
                    font(
                        "Document font",
                        None,
                        |s| &s.source_box_font,
                        |s, v| s.source_box_font = v,
                    ),
                ),
                SettingItem::new(
                    "Line numbers",
                    switch(|s| s.line_numbers, |s, v| s.line_numbers = v),
                ),
                SettingItem::new("Soft wrap", switch(|s| s.soft_wrap, |s, v| s.soft_wrap = v)),
                SettingItem::new(
                    "Autocomplete",
                    switch(|s| s.autocomplete, |s, v| s.autocomplete = v),
                )
                .keywords(["completion"]),
            ]),
        )
        .group(group(cx).title("Find").item(SettingItem::new(
            "Match case by default",
            switch(|s| s.find_match_case, |s, v| s.find_match_case = v),
        )));
    let files = SettingPage::new("Files").icon(icon("folder")).group(
        group(cx).title("Project panel").items([
            SettingItem::new(
                "Show hidden files",
                switch(|s| s.show_hidden_files, |s, v| s.show_hidden_files = v),
            ),
            SettingItem::new(
                "Hide files matching",
                text("*.aux, *.log", |s| &s.hide_files, |s, v| s.hide_files = v),
            ),
            SettingItem::new(
                "Outline depth",
                choice(
                    [
                        (OutlineDepth::Sections, "Sections"),
                        (OutlineDepth::Subsections, "Subsections"),
                        (OutlineDepth::Subsubsections, "Subsubsections"),
                        (OutlineDepth::Paragraphs, "Paragraphs"),
                    ],
                    |s| s.outline_depth,
                    |s, v| s.outline_depth = v,
                ),
            ),
        ]),
    );
    let typesetting = SettingPage::new("Typesetting")
        .icon(icon("type"))
        .group(
            group(cx).title("Engine").items([
                SettingItem::new(
                    "rtex binary",
                    path("Automatic", false, |s| &s.rtex_path, |s, v| s.rtex_path = v),
                )
                .layout(Axis::Vertical)
                .keywords(["engine", "path"]),
                SettingItem::new(
                    "TeX folder",
                    path("Automatic", true, |s| &s.tex_bin, |s, v| s.tex_bin = v),
                )
                .layout(Axis::Vertical)
                .keywords(["lualatex", "path"]),
                SettingItem::new(
                    "Download rtex automatically",
                    switch(|s| s.download_rtex, |s, v| s.download_rtex = v),
                ),
            ]),
        )
        .group(
            group(cx).title("Performance").items([
                SettingItem::new(
                    "Paragraph time limit (ms)",
                    number(
                        (10.0, 1000.0, 10.0),
                        |s| s.fast_budget_ms,
                        |s, v| s.fast_budget_ms = v,
                    ),
                )
                .keywords(["budget", "live"]),
                SettingItem::new(
                    "Reuse drawn pictures",
                    switch(|s| s.picture_cache, |s, v| s.picture_cache = v),
                )
                .keywords(["tikz", "cache"]),
            ]),
        )
        .group(
            group(cx).title("Troubleshooting").items([
                SettingItem::new(
                    "Diagnostics folder",
                    path("None", true, |s| &s.debug_dir, |s, v| s.debug_dir = v),
                )
                .layout(Axis::Vertical)
                .keywords(["debug"]),
                SettingItem::new("Build cache", SettingField::render(clear_cache_button)),
            ]),
        );
    vec![
        general,
        appearance,
        editor,
        files,
        typesetting,
        shortcuts_page(cx).icon(icon("keyboard")),
    ]
}

/// The Settings window; focused as it opens, so its keys (⌘W) reach it before anything is
/// clicked.
pub struct SettingsView(FocusHandle);

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self(focus)
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // a drawn title bar, so it follows the theme like the main window's
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .track_focus(&self.0)
            // ⌘W closes this window as it closes any
            .on_action(|_: &CloseTab, window, _| window.remove_window())
            .child(
                TitleBar::new()
                    .child(
                        // moves the window, like the main window's title bar
                        div()
                            .id("title-drag")
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .on_mouse_down(MouseButton::Left, |ev, window, cx| {
                                cx.stop_propagation();
                                if ev.click_count == 2 {
                                    window.titlebar_double_click();
                                } else {
                                    window.start_window_move();
                                }
                            })
                            .child("Settings"),
                    )
                    .bg(cx.theme().title_bar)
                    .border_color(cx.theme().title_bar_border),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Settings::new("settings").pages(pages(cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettings, FontField, OnLaunch, OutlineDepth, SettingsView, TexView, Zoom, choice, font,
        font_choices, group, pages, shortcut_groups,
    };
    use crate::ui::editor::MONO;
    use crate::ui::palette;
    use crate::ui::setting::{
        AnySettingField, SelectIndex, SettingField, SettingGroup, SettingItem, SettingPage,
        Settings,
    };
    use crate::ui::themes;
    use gpui_kit::base::Scrollbar;
    use gpui_kit::component::color_picker::ColorPickerState;
    use gpui_kit::component::select::SelectEvent;
    use gpui_kit::component::{
        ActiveTheme, Theme, ThemeConfig, ThemeMode, ThemeRegistry, ThemeSet,
    };
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{
        App, AppContext, Axis, Context, ElementId, Entity, IntoElement, Modifiers, Pixels, Point,
        Render, ScrollDelta, ScrollEvent, SharedString, Styled, TestAppContext, VisualTestContext,
        Window, canvas, div, point, px, size,
    };
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::rc::Rc;

    thread_local! {
        /// The colour rows drawn, by key.
        pub(super) static COLOR_ROWS: RefCell<HashMap<String, Entity<ColorPickerState>>> =
            RefCell::default();
    }

    #[test]
    fn rtex_gets_the_engine_settings() {
        let d = AppSettings::default();
        assert_eq!(d.serve_args(), ["--fast-budget-ms", "50"]);
        let s = AppSettings {
            fast_budget_ms: 120.4,
            picture_cache: false,
            debug_dir: "/tmp/rtex debug".into(),
            ..d
        };
        assert_eq!(
            s.serve_args(),
            [
                "--fast-budget-ms",
                "120",
                "--no-picture-cache",
                "--debug-dir",
                "/tmp/rtex debug"
            ]
        );
        // a settings file from before these settings reads with their defaults
        let old: AppSettings = serde_json::from_str(r#"{"autosave":true}"#).unwrap();
        assert_eq!(old.autosave_delay, 1.0);
        assert!(old.picture_cache);
        assert!(!old.compile_on_quit);
        assert!(!old.export_on_quit);
        assert_eq!(old.compile_runs, 2);
    }

    /// The last version's file: its zoom a number; the settings added since at their defaults.
    #[test]
    fn an_old_settings_file_still_reads() {
        let old = r#"{
            "theme": "dark", "ui_font_size": 15.0, "dark_pages": true,
            "source_font_size": 13.0, "source_box_font": "", "default_zoom": 90.0,
            "live_preview": true, "line_numbers": true, "soft_wrap": true, "autosave": true,
            "autosave_delay": 2.0, "open_after_export": false, "fast_budget_ms": 50.0,
            "picture_cache": true, "debug_dir": "", "rtex_path": "",
            "tex_bin": "/Library/TeX/texbin", "ribbon_collapsed": false,
            "inspector_closed": false, "tour_seen": true, "shortcuts": {}
        }"#;
        let s: AppSettings = serde_json::from_str(old).unwrap();
        assert_eq!(s.default_zoom, Zoom::Percent(90.0));
        assert_eq!((s.theme.as_str(), s.autosave_delay), ("dark", 2.0));
        assert_eq!(
            (s.on_launch, s.recent_kept, s.outline_depth, s.open_tex_in),
            (
                OnLaunch::Reopen,
                30,
                OutlineDepth::Subsubsections,
                TexView::Visual
            )
        );
        assert!(s.cursor_blink && s.download_rtex && s.autocomplete && s.pinch_zoom);
        assert!(!s.show_hidden_files && s.hide_files.is_empty() && s.source_font.is_empty());
        assert!(s.accent.is_empty() && s.colors.is_empty());
        assert_eq!(
            (s.light_theme.as_str(), s.dark_theme.as_str()),
            (themes::DEFAULT_LIGHT, themes::DEFAULT_DARK)
        );
        // and what this version writes reads back the same
        let new = AppSettings {
            default_zoom: Zoom::FitWidth,
            on_launch: OnLaunch::Welcome,
            outline_depth: OutlineDepth::Paragraphs,
            dark_theme: "Nord".into(),
            colors: BTreeMap::from([(
                "Nord".to_string(),
                BTreeMap::from([
                    ("syntax.keyword".to_string(), "#FF000080".to_string()),
                    (themes::ACCENT.to_string(), "#7C4DDB".to_string()),
                ]),
            )]),
            ..s.clone()
        };
        let json = serde_json::to_string(&new).unwrap();
        assert!(json.contains(r#""default_zoom":null"#), "{json}");
        assert!(json.contains(r#""on_launch":"welcome""#), "{json}");
        assert!(
            serde_json::from_str::<serde_json::Value>(&json)
                .unwrap()
                .get("accent")
                .is_none()
        );
        assert_eq!(serde_json::from_str::<AppSettings>(&json).unwrap(), new);
        // with no accent of its own, reading it changes nothing
        assert_eq!(s.clone().migrated(), s);
    }

    /// A settings file from when the accent was one for all themes: the chosen light and dark
    /// themes each get it, unless they have their own; nothing else changes, and it is not
    /// written again. An empty or unreadable one changes nothing.
    #[test]
    fn an_old_accent_goes_to_the_chosen_themes() {
        let old = r##"{
            "accent": "#C93A85", "light_theme": "Solarized Light", "dark_theme": "Nord",
            "colors": {
                "Nord": {"accent": "#1E7F47", "background": "#000000"},
                "Gruvbox Dark": {"background": "#111111"}
            }
        }"##;
        let s = serde_json::from_str::<AppSettings>(old).unwrap().migrated();
        let map = |pairs: &[(&str, &str)]| {
            (pairs.iter())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            s.colors,
            BTreeMap::from([
                ("Solarized Light".to_string(), map(&[("accent", "#C93A85")])),
                (
                    "Nord".to_string(),
                    map(&[("accent", "#1E7F47"), ("background", "#000000")])
                ),
                (
                    "Gruvbox Dark".to_string(),
                    map(&[("background", "#111111")])
                ),
            ])
        );
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&json)
                .unwrap()
                .get("accent")
                .is_none()
        );
        // the themes chosen by default when the file names none
        let s = serde_json::from_str::<AppSettings>(r##"{"accent": "#7C4DDB"}"##)
            .unwrap()
            .migrated();
        assert_eq!(
            s.colors.keys().collect::<Vec<_>>(),
            [themes::DEFAULT_DARK, themes::DEFAULT_LIGHT]
        );
        for accent in ["", "blue"] {
            let file = format!(r#"{{"accent": "{accent}", "theme": "dark"}}"#);
            let s = serde_json::from_str::<AppSettings>(&file)
                .unwrap()
                .migrated();
            assert_eq!(
                s,
                AppSettings {
                    theme: "dark".into(),
                    ..AppSettings::default()
                }
            );
        }
    }

    /// Each theme has its own accent: it colours the theme at launch (an old settings file's,
    /// carried to both chosen themes) and on each change, a switch of mode (the system's too)
    /// keeps it, and changing the dark theme's leaves the light one's. None: the theme's own.
    #[gpui_kit::test]
    fn the_accent_belongs_to_its_theme(cx: &mut TestAppContext) {
        let hex = |c| palette::hex(c);
        let old = r##"{"theme": "light", "accent": "#C93A85"}"##;
        started(
            serde_json::from_str::<AppSettings>(old).unwrap().migrated(),
            cx,
        );
        let accent = |cx: &App| themes::page_color(themes::ACCENT, cx).map(palette::hex);
        cx.update(|cx| {
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            AppSettings::set(cx, |s| s.theme = "dark".into());
            assert!(cx.theme().mode.is_dark());
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            assert_ne!(hex(cx.theme().ring), "#86A1FF");
            // the dark theme's accent changed: the light one keeps its own
            set(themes::ACCENT, "#1E7F47", cx);
            assert_eq!(hex(cx.theme().primary), "#1E7F47");
            assert_eq!(accent(cx).as_deref(), Some("#1E7F47"));
            Theme::change(ThemeMode::Light, None, cx);
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            Theme::change(ThemeMode::Dark, None, cx);
            assert_eq!(hex(cx.theme().primary), "#1E7F47");
            AppSettings::set(cx, |s| s.theme = "light".into());
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            assert_eq!(accent(cx).as_deref(), Some("#C93A85"));
            // the theme's own: theme.json's light and dark blues
            themes::set_color(themes::ACCENT, None, cx);
            assert_eq!(hex(cx.theme().primary), "#3557D4");
            assert_eq!(hex(cx.theme().ring), "#3557D4");
            assert_eq!(accent(cx), None);
            AppSettings::set(cx, |s| s.theme = "dark".into());
            assert_eq!(hex(cx.theme().primary), "#1E7F47");
            themes::set_color(themes::ACCENT, None, cx);
            assert_eq!(hex(cx.theme().primary), "#4A68E8");
            assert_eq!(hex(cx.theme().ring), "#86A1FF");
            assert_eq!(hex(cx.theme().link), "#86A1FF");
            assert!(AppSettings::get(cx).colors.is_empty());
            // a colour no one can read is the theme's own
            set(themes::ACCENT, "blue", cx);
            assert_eq!(hex(cx.theme().primary), "#4A68E8");
        });
        cx.run_until_parked();
    }

    /// The app as AppSettings::init leaves it, with `s`.
    fn started(s: AppSettings, cx: &mut TestAppContext) {
        // the theme registry's observer in place, which puts back a theme it holds by name
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.set_global(s);
            themes::init(cx);
            AppSettings::apply(cx);
        });
        cx.run_until_parked();
    }

    /// theme.json's theme `name`, as the registry reads it.
    fn theme_json(name: &str) -> ThemeConfig {
        let set: ThemeSet = serde_json::from_str(include_str!("theme.json")).unwrap();
        set.themes.into_iter().find(|t| t.name == name).unwrap()
    }

    /// With nothing set, the colours on screen are theme.json's, in either mode.
    #[gpui_kit::test]
    fn no_colour_settings_is_theme_json(cx: &mut TestAppContext) {
        started(AppSettings::default(), cx);
        cx.update(|cx| {
            for (mode, name) in [
                (ThemeMode::Light, themes::DEFAULT_LIGHT),
                (ThemeMode::Dark, themes::DEFAULT_DARK),
            ] {
                Theme::change(mode, None, cx);
                let mut want = Theme::default();
                want.apply_config(&Rc::new(theme_json(name)));
                assert_eq!(cx.theme().colors, want.colors, "{name}");
            }
        });
    }

    fn set(key: &str, value: &str, cx: &mut App) {
        themes::set_color(key, Some(value.into()), cx);
    }

    /// A colour changed in the dark theme is on screen at once, and stays through every switch of
    /// mode and every change to the registry (a theme file added): the registry's observer puts
    /// back a theme it holds by name, so a colour set on the theme on screen alone would be lost.
    #[gpui_kit::test]
    fn a_changed_colour_survives_a_switch_of_mode(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        let hex = |c| palette::hex(c);
        cx.update(|cx| {
            set("tab.active.background", "#123456", cx);
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            // the accent is under it
            set(themes::ACCENT, "#C93A85", cx);
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            assert_eq!(hex(cx.theme().primary), "#C93A85");
            set("primary.background", "#00FF0080", cx);
            assert_eq!(hex(cx.theme().primary), "#00FF0080");
            Theme::change(ThemeMode::Light, None, cx);
            Theme::change(ThemeMode::Dark, None, cx);
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            ThemeRegistry::global_mut(cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(hex(cx.theme().tab_active), "#123456");
            assert_eq!(hex(cx.theme().primary), "#00FF0080");
            themes::set_color("tab.active.background", None, cx);
            assert_eq!(hex(cx.theme().tab_active), "#1D1E22");
        });
    }

    /// Colours are changed per theme: the dark theme's leave the light one as it is; another dark
    /// theme shows its own, and the first one's come back with it.
    #[gpui_kit::test]
    fn changed_colours_belong_to_their_theme(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        let hex = |c| palette::hex(c);
        cx.update(|cx| {
            set("background", "#FF0000", cx);
            assert_eq!(hex(cx.theme().background), "#FF0000");
            assert!(AppSettings::get(cx).colors[themes::DEFAULT_DARK].contains_key("background"));
            AppSettings::set(cx, |s| s.theme = "light".into());
            assert_eq!(hex(cx.theme().background), "#FFFFFF");
            AppSettings::set(cx, |s| {
                s.theme = "dark".into();
                s.dark_theme = "Nord".into();
            });
            assert_eq!(hex(cx.theme().background), "#2E3440");
            assert_eq!(themes::active(cx).as_deref(), Some("Nord"));
            assert!(
                AppSettings::get(cx)
                    .colors
                    .contains_key(themes::DEFAULT_DARK)
            );
            AppSettings::set(cx, |s| s.dark_theme = themes::DEFAULT_DARK.into());
            assert_eq!(hex(cx.theme().background), "#FF0000");
            // a theme no longer there: the app's own
            AppSettings::set(cx, |s| s.dark_theme = "Gone".into());
            assert_eq!(themes::active(cx).as_deref(), Some(themes::DEFAULT_DARK));
        });
    }

    /// A syntax colour changed is the one the source editor's highlighter gives its tokens, the
    /// theme's weight and style kept.
    #[gpui_kit::test]
    fn a_changed_syntax_colour_reaches_the_highlighter(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                dark_theme: "Gruvbox Dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        cx.update(|cx| {
            let style = |name: &str, cx: &App| cx.theme().highlight_theme.style(name).unwrap();
            assert_eq!(palette::hex(style("keyword", cx).color.unwrap()), "#FB4934");
            set("syntax.keyword", "#00FF00", cx);
            set("syntax.function", "#0000FF", cx);
            assert_eq!(palette::hex(style("keyword", cx).color.unwrap()), "#00FF00");
            let function = style("function", cx);
            assert_eq!(palette::hex(function.color.unwrap()), "#0000FF");
            assert_eq!(function.font_weight, Some(gpui_kit::FontWeight::BOLD));
            // and what Settings shows
            assert_eq!(
                themes::shown(cx)["syntax.keyword"]
                    .map(palette::hex)
                    .as_deref(),
                Some("#00FF00")
            );
        });
    }

    /// A theme file in the themes folder is offered in its mode's list, after the app's own,
    /// which the registry drops when it reads the folder (its own reading starts a file watcher
    /// thread the test scheduler refuses, so the test reads the file as it does: a registry of
    /// the folder's themes alone).
    #[gpui_kit::test]
    fn a_theme_in_the_themes_folder_is_offered(cx: &mut TestAppContext) {
        started(AppSettings::default(), cx);
        let dir = std::env::temp_dir().join(format!("rtex-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = r##"{"name": "Mine", "themes": [
            {"name": "Midnight Mine", "mode": "dark", "colors": {"background": "#010203"}}
        ]}"##;
        std::fs::write(dir.join("mine.json"), file).unwrap();
        cx.update(|cx| {
            let mut read = ThemeRegistry::default();
            let json = std::fs::read_to_string(dir.join("mine.json")).unwrap();
            read.load_themes_from_str(&json).unwrap();
            cx.set_global(read);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let dark = themes::names(ThemeMode::Dark, cx);
            assert_eq!(dark[0], themes::DEFAULT_DARK, "{dark:?}");
            assert_eq!(
                dark.last().map(|n| n.as_ref()),
                Some("Midnight Mine"),
                "{dark:?}"
            );
            assert!(!dark.iter().any(|n| n == "Default Dark"), "{dark:?}");
            let light = themes::names(ThemeMode::Light, cx);
            assert_eq!(
                light,
                [
                    themes::DEFAULT_LIGHT,
                    "Catppuccin Latte",
                    "GitHub Light",
                    "Gruvbox Light",
                    "High Contrast Light",
                    "Light Owl",
                    "Solarized Light",
                    "Tokyo Night Light"
                ]
            );
            AppSettings::set(cx, |s| {
                s.theme = "dark".into();
                s.dark_theme = "Midnight Mine".into();
            });
            assert_eq!(palette::hex(cx.theme().background), "#010203");
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The pages' colours, each changed in Settings, are what the editor draws, for the theme on
    /// screen alone; unchanged, they are what it drew before they could be changed, and the
    /// Appearance shows what is drawn.
    #[gpui_kit::test]
    fn page_colours_reach_the_pages(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        let hex = |c| palette::hex(c);
        let drawn = |cx: &App| {
            let dark = palette::dark_pages(cx);
            let m = palette::marks(dark, cx);
            let (bg, fg) = palette::source_box(dark, cx);
            let d = palette::dark_paper(cx);
            [
                hex(m.text_selection),
                hex(m.match_current),
                hex(m.match_other),
                hex(m.accent),
                hex(bg),
                hex(fg),
                format!("{:?}", d.paper),
                format!("{:?}", d.ink),
                hex(m.on_accent),
            ]
        };
        // what Settings shows is what is drawn, key by key in themes::PAGES' order
        let shown = |cx: &App| {
            let s = themes::shown(cx);
            (themes::PAGES.iter())
                .map(|(k, _)| s[*k].map(palette::hex).unwrap())
                .collect::<Vec<_>>()
        };
        let as_hex = |d: [String; 9], cx: &App| {
            let mut v = d[..6].to_vec();
            v.push(hex(palette::paper(true, cx)));
            v.push(hex(palette::ink(true, cx)));
            v
        };
        let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // as the editor drew them before: the theme's selection blue (opaque for the block and
        // insert line, 60% under selected text, 40% on white) and warning amber, the source box a
        // step off the paper, black paper with light gray ink
        let dark_before = strings(&[
            "#4A68E899",
            "#FACC1599",
            "#FACC1547",
            "#4A68E8",
            "#262626",
            "#E6E6E6",
            "[0, 0, 0]",
            "[224, 224, 224]",
            "#FFFFFF",
        ]);
        let light_before = strings(&[
            "#3557D466",
            "#CA8A0499",
            "#CA8A0447",
            "#3557D4",
            "#EBEBEB",
            "#1F1F1F",
            "[0, 0, 0]",
            "[224, 224, 224]",
            "#FFFFFF",
        ]);
        cx.update(|cx| {
            assert_eq!(drawn(cx).to_vec(), dark_before);
            assert_eq!(shown(cx), as_hex(drawn(cx), cx));
            // the selection follows a changed accent (a pale one, less of it, so the light ink
            // over it still reads), and the icon on it reads
            set("pages.accent", "#FFE066", cx);
            let d = drawn(cx);
            assert_eq!(
                (&*d[0], &*d[3], &*d[8]),
                ("#FFE06666", "#FFE066", "#000000")
            );
            let mine = [
                ("pages.selection", "#00FF0040"),
                ("pages.find.current", "#FF000080"),
                ("pages.find.other", "#FF000030"),
                ("pages.accent", "#123456"),
                ("pages.source_box.background", "#202040"),
                ("pages.source_box.foreground", "#FFEEDD"),
                ("pages.paper", "#101830"),
                ("pages.ink", "#FAF0C8"),
            ];
            for (k, v) in mine {
                set(k, v, cx);
            }
            let want = strings(&[
                "#00FF0040",
                "#FF000080",
                "#FF000030",
                "#123456",
                "#202040",
                "#FFEEDD",
                "[16, 24, 48]",
                "[250, 240, 200]",
                "#FFFFFF",
            ]);
            assert_eq!(drawn(cx).to_vec(), want);
            assert_eq!(shown(cx), as_hex(drawn(cx), cx));
            assert_eq!(shown(cx), mine.map(|(_, v)| v.to_string()));
            assert_eq!(
                palette::paper(true, cx),
                palette::hex_color("#101830").unwrap()
            );
            // the light theme's are its own; back in the dark one, the changed ones are back
            AppSettings::set(cx, |s| s.theme = "light".into());
            assert_eq!(drawn(cx).to_vec(), light_before);
            assert_eq!(shown(cx), as_hex(drawn(cx), cx));
            AppSettings::set(cx, |s| s.theme = "dark".into());
            Theme::change(ThemeMode::Light, None, cx);
            Theme::change(ThemeMode::Dark, None, cx);
            assert_eq!(drawn(cx).to_vec(), want);
            // another dark theme has none of them
            AppSettings::set(cx, |s| s.dark_theme = "Nord".into());
            assert_eq!(drawn(cx)[4..8], dark_before[4..8]);
            AppSettings::set(cx, |s| s.dark_theme = themes::DEFAULT_DARK.into());
            assert_eq!(drawn(cx).to_vec(), want);
            // back to the theme's
            for (k, _) in mine {
                themes::set_color(k, None, cx);
            }
            assert_eq!(drawn(cx).to_vec(), dark_before);
            assert!(AppSettings::get(cx).colors.is_empty());
        });
    }

    /// Appearance draws a row for every colour of the theme schema; there is no page of colours
    /// apart from it (the pages are those every_page_draws counts).
    #[gpui_kit::test]
    fn appearance_has_a_row_per_colour(cx: &mut TestAppContext) {
        started(AppSettings::default(), cx);
        COLOR_ROWS.with(|r| r.borrow_mut().clear());
        let t = std::time::Instant::now();
        let (_, vcx) = cx.add_window_view(|_, _| OnPage(1));
        // tall enough that the page's list draws every group
        vcx.simulate_resize(size(px(1000.0), px(200_000.0)));
        vcx.run_until_parked();
        eprintln!("Appearance, every colour row, drew in {:?}", t.elapsed());
        let t = std::time::Instant::now();
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
        eprintln!("and again in {:?}", t.elapsed());
        let keys: HashSet<String> = (themes::groups().iter())
            .flat_map(|g| g.keys.iter().map(|k| k.key.clone()))
            .collect();
        COLOR_ROWS.with(|r| assert_eq!(r.borrow().keys().cloned().collect::<HashSet<_>>(), keys));
        // and the theme's actions with them
        let export = vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find("colors-export").is_some()
        });
        assert!(export);
    }

    /// Appearance's reset puts back its settings and the accent and colours of the theme it
    /// shows, as Reset all colors does, and leaves the other themes' alone.
    #[gpui_kit::test]
    fn appearance_reset_leaves_other_themes_colours(cx: &mut TestAppContext) {
        started(
            AppSettings {
                theme: "dark".into(),
                ui_font_size: 18.0,
                ..AppSettings::default()
            },
            cx,
        );
        cx.update(|cx| {
            set(themes::ACCENT, "#1E7F47", cx);
            set("tab.active.background", "#123456", cx);
            AppSettings::set(cx, |s| s.theme = "light".into());
            set(themes::ACCENT, "#C93A85", cx);
            AppSettings::set(cx, |s| s.theme = "dark".into());
        });
        let (_, vcx) = cx.add_window_view(|_, _| OnPage(1));
        vcx.simulate_resize(size(px(860.0), px(586.0)));
        vcx.run_until_parked();
        let reset = vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find("reset").map(|b| b.bounds().center())
        });
        vcx.simulate_click(reset.expect("the page's reset"), Modifiers::default());
        vcx.run_until_parked();
        vcx.update(|_, cx| {
            let s = AppSettings::get(cx);
            assert_eq!((s.theme.as_str(), s.ui_font_size), ("system", 16.0));
            assert!(
                !s.colors.contains_key(themes::DEFAULT_DARK),
                "{:?}",
                s.colors
            );
            assert_eq!(
                (s.colors[themes::DEFAULT_LIGHT].get(themes::ACCENT)).map(String::as_str),
                Some("#C93A85")
            );
        });
    }

    #[test]
    fn the_outline_lists_headings_down_to_its_depth() {
        let o = crate::document::outline(
            "\\section{A}\n\\subsection{B}\n\\subsubsection{C}\n\\paragraph{D}\n",
        );
        let titles = |d: OutlineDepth| {
            o.iter()
                .filter(|h| d.shows(h.level))
                .map(|h| h.title.as_str())
                .collect::<String>()
        };
        assert_eq!(titles(OutlineDepth::Sections), "A");
        assert_eq!(titles(OutlineDepth::Subsections), "AB");
        assert_eq!(titles(OutlineDepth::Subsubsections), "ABC");
        assert_eq!(titles(OutlineDepth::Paragraphs), "ABCD");
    }

    /// The Settings window opened on page `0`.
    struct OnPage(usize);

    impl Render for OnPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            Settings::new("settings")
                .pages(pages(cx))
                .default_selected_index(SelectIndex {
                    page_ix: self.0,
                    group_ix: None,
                })
        }
    }

    /// The window, and each of its pages, draws (the test text system has no fonts: nothing
    /// about how it looks is checked).
    #[gpui_kit::test]
    fn every_page_draws(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        cx.add_window_view(SettingsView::new).1.run_until_parked();
        let n = cx.update(|cx| pages(cx).len());
        assert_eq!(n, 6);
        for i in 0..n {
            cx.add_window_view(|_, _| OnPage(i)).1.run_until_parked();
        }
    }

    /// A page's reset puts a dropdown's and a text field's settings back to their defaults.
    #[gpui_kit::test]
    fn reset_brings_back_the_defaults(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        let zoom = choice(
            [
                (Zoom::FitWidth, "Fit width"),
                (Zoom::Percent(100.0), "100%"),
            ],
            |s| s.default_zoom,
            |s, v| s.default_zoom = v,
        );
        let font = font(
            MONO,
            Some(MONO),
            |s| &s.source_font,
            |s, v| s.source_font = v,
        );
        let (_, cx) = cx.add_window_view(SettingsView::new);
        cx.update(|window, cx| {
            assert!(!zoom.is_resettable(cx) && !font.is_resettable(cx));
            AppSettings::set(cx, |s| {
                s.default_zoom = Zoom::FitWidth;
                s.source_font = "Fira Code".into();
            });
            assert!(zoom.is_resettable(cx) && font.is_resettable(cx));
            zoom.reset(window, cx);
            font.reset(window, cx);
            let s = AppSettings::get(cx);
            assert_eq!(s.default_zoom, Zoom::Percent(100.0));
            assert_eq!(s.source_font, "");
        });
    }

    /// A font list: the default first (meaning empty), the installed families without it, and
    /// a font no longer installed kept last.
    #[test]
    fn a_font_list_starts_with_its_default() {
        let names = ["Fira Code", "Menlo", "Monaco"].map(String::from);
        let list = font_choices("Menlo", Some("Menlo"), &names, "Gone Mono");
        let got: Vec<(&str, &str)> = list
            .iter()
            .map(|c| (c.name.as_ref(), c.label.as_ref()))
            .collect();
        assert_eq!(
            got,
            [
                ("", "Menlo"),
                ("Fira Code", "Fira Code"),
                ("Monaco", "Monaco"),
                ("Gone Mono", "Gone Mono"),
            ]
        );
        assert_eq!(list[0].face.as_deref(), Some("Menlo"));
        assert_eq!(list[1].face.as_deref(), Some("Fira Code"));
        let doc = font_choices("Document font", None, &names, "Monaco");
        assert_eq!(
            (doc[0].name.as_ref(), doc[0].label.as_ref()),
            ("", "Document font")
        );
        assert!(doc[0].face.is_none());
        assert_eq!(doc.len(), 4);
    }

    /// A font picked in the list is the setting; the first item empties it.
    #[gpui_kit::test]
    fn picking_a_font_sets_it(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        let (_, cx) = cx.add_window_view(SettingsView::new);
        let field = cx.update(|window, cx| {
            cx.new(|cx| {
                FontField::new(
                    MONO,
                    Some(MONO),
                    |s| &s.source_font,
                    |s, v| s.source_font = v,
                    window,
                    cx,
                )
            })
        });
        let select = cx.update(|_, cx| field.read(cx).select.clone());
        for name in ["Fira Code", ""] {
            select.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some(name.into()))));
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(AppSettings::get(cx).source_font, name));
        }
    }

    /// A page with one full-width field, which notes where its right edge is drawn, at an
    /// interface font size.
    struct Probe(Rc<Cell<Pixels>>, f32);

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // the app's Root sets it from the theme
            window.set_rem_size(px(self.1));
            let edge = self.0.clone();
            let field = SettingField::render(move |_, _, _| {
                let edge = edge.clone();
                canvas(move |b, _, _| edge.set(b.right()), |_, _, _, _| {})
                    .w_full()
                    .h(px(8.0))
            });
            let item = SettingItem::new("Field", field).layout(Axis::Vertical);
            Settings::new("settings")
                .pages(vec![SettingPage::new("Page").group(group(cx).item(item))])
        }
    }

    /// The focus ring (3 px) of a control on a group's right edge stays clear of the
    /// scrollbar's track, which covers the list's right edge while it shows.
    #[gpui_kit::test]
    fn focus_rings_clear_the_scrollbar(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
        });
        for size in [10.0, 16.0, 24.0] {
            cx.update(|cx| AppSettings::set(cx, |s| s.ui_font_size = size));
            let edge = Rc::new(Cell::new(px(0.0)));
            let (_, vcx) = cx.add_window_view(|_, _| Probe(edge.clone(), size as f32));
            vcx.run_until_parked();
            let track = vcx.update(|window, _| window.viewport_size().width) - Scrollbar::width();
            assert!(edge.get() > px(0.0), "the field was drawn");
            assert!(
                edge.get() + px(3.0) < track,
                "at {size} px the ring reaches {:?}, the track starts at {track:?}",
                edge.get() + px(3.0)
            );
        }
    }

    fn on_pages(cx: &mut TestAppContext) -> &mut VisualTestContext {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        // the Settings window's size, less its title bar
        let (_, vcx) = cx.add_window_view(|_, _| OnPage(1));
        vcx.simulate_resize(size(px(860.0), px(586.0)));
        vcx.run_until_parked();
        // the Pages group at the top, as its entry in the sidebar shows it
        while swatch("pages.selection", vcx).is_none_or(|at| at.y > px(140.0)) {
            vcx.simulate_event(ScrollEvent {
                position: point(px(600.0), px(300.0)),
                delta: ScrollDelta::Pixels(point(px(0.0), px(-20.0))),
                ..Default::default()
            });
            vcx.run_until_parked();
        }
        vcx
    }

    fn picker(key: &str) -> Entity<ColorPickerState> {
        COLOR_ROWS.with(|r| r.borrow()[key].clone())
    }

    /// The middle of `key`'s swatch, when it is on screen.
    fn swatch(key: &str, vcx: &mut VisualTestContext) -> Option<Point<Pixels>> {
        let id = SharedString::from(format!("swatch-{key}"));
        vcx.update(|window, cx| {
            window.render_frame(cx);
            (window.try_find(id))
                .filter(|s| s.visible())
                .map(|s| s.bounds().center())
        })
    }

    /// Where each palette colour of `key`'s open picker is, found by hovering, as a user would.
    fn cells(key: &str, vcx: &mut VisualTestContext) -> Vec<(String, Point<Pixels>)> {
        let p = picker(key);
        let size = vcx.update(|window, _| window.viewport_size());
        // what it shows before any hover
        let at_first = vcx.update(|_, cx| p.read(cx).preview().map(palette::hex));
        let mut found: Vec<(String, Point<Pixels>)> = at_first
            .into_iter()
            .map(|h| (h, point(px(-1.0), px(-1.0))))
            .collect();
        let (mut x, mut y) = (px(0.0), px(0.0));
        while y < size.height {
            while x < size.width {
                vcx.simulate_mouse_move(point(x, y), None, Modifiers::default());
                let shown = vcx.update(|_, cx| p.read(cx).preview().map(palette::hex));
                if let Some(h) = shown.filter(|h| !found.iter().any(|(f, _)| f == h)) {
                    // the middle of the cell: 20 px, 4 px apart
                    found.push((h, point(x + px(8.0), y + px(8.0))));
                }
                x += px(4.0);
            }
            (x, y) = (px(0.0), y + px(4.0));
        }
        found.retain(|(_, at)| at.x >= px(0.0));
        found
    }

    fn stored(key: &str, cx: &App) -> Option<String> {
        let name = themes::active(cx)?;
        AppSettings::get(cx)
            .colors
            .get(name.as_ref())?
            .get(key)
            .cloned()
    }

    /// A click on any colour of a row's palette, in any column, picks it for that row.
    #[gpui_kit::test]
    fn a_click_on_any_palette_cell_picks_it(cx: &mut TestAppContext) {
        let vcx = on_pages(cx);
        let key = "pages.selection";
        let at = swatch(key, vcx).expect("the Selected text row is on screen");
        vcx.simulate_click(at, Modifiers::default());
        vcx.run_until_parked();
        let cells = cells(key, vcx);
        // the 12 featured colours and the palette's 9 rows of 11
        assert!(cells.len() >= 100, "{} cells found", cells.len());
        for (hex, cell) in &cells {
            if !vcx.update(|_, cx| picker(key).read(cx).is_open()) {
                vcx.simulate_click(at, Modifiers::default());
                vcx.run_until_parked();
            }
            vcx.simulate_click(*cell, Modifiers::default());
            vcx.run_until_parked();
            let got = vcx.update(|_, cx| stored(key, cx));
            assert_eq!(got.as_deref(), Some(hex.as_str()), "the cell at {cell:?}");
        }
    }

    /// A row's picker opens at the row's colour: not at a colour under the pointer that opened
    /// it, nor at one hovered the last time it was open.
    #[gpui_kit::test]
    fn a_picker_opens_at_its_rows_colour(cx: &mut TestAppContext) {
        let vcx = on_pages(cx);
        let keys: Vec<&str> = (themes::groups().iter())
            .flat_map(|g| g.keys.iter().map(|k| k.key.as_str()))
            .collect();
        let shown = vcx.update(|_, cx| themes::shown(cx));
        let opened = |key: &str, vcx: &mut VisualTestContext| {
            vcx.update(|_, cx| picker(key).read(cx).preview().map(palette::hex))
        };
        let mut tried = 0;
        for key in keys {
            let Some(at) = swatch(key, vcx) else { continue };
            vcx.simulate_click(at, Modifiers::default());
            // the pointer that opened it, still over the swatch
            vcx.simulate_mouse_move(at, None, Modifiers::default());
            vcx.run_until_parked();
            assert!(vcx.update(|_, cx| picker(key).read(cx).is_open()), "{key}");
            assert_eq!(opened(key, vcx), shown[key].map(palette::hex), "{key}");
            vcx.simulate_keystrokes("escape");
            vcx.run_until_parked();
            tried += 1;
        }
        assert!(tried >= 8, "{tried} rows on screen");
        // a colour hovered, the picker closed without a pick, and opened again
        let key = "pages.selection";
        let at = swatch(key, vcx).unwrap();
        vcx.simulate_click(at, Modifiers::default());
        vcx.run_until_parked();
        let (hex, cell) = cells(key, vcx).pop().unwrap();
        vcx.simulate_mouse_move(cell, None, Modifiers::default());
        assert_eq!(opened(key, vcx), Some(hex));
        vcx.simulate_keystrokes("escape");
        vcx.run_until_parked();
        assert!(!vcx.update(|_, cx| picker(key).read(cx).is_open()));
        vcx.simulate_click(at, Modifiers::default());
        vcx.run_until_parked();
        assert_eq!(opened(key, vcx), shown[key].map(palette::hex));
        assert_eq!(vcx.update(|_, cx| stored(key, cx)), None);
    }

    /// A mark over text chosen opaque is drawn at the mark's own strength, so the text under it
    /// still reads, and the row shows that; one chosen with an alpha is drawn as chosen. The
    /// selected block's outline and insert line, over no text, are drawn as chosen.
    #[gpui_kit::test]
    fn an_opaque_mark_over_text_keeps_it_readable(cx: &mut TestAppContext) {
        let drawn = |cx: &App| {
            let m = palette::marks(palette::dark_pages(cx), cx);
            [m.text_selection, m.match_current, m.match_other, m.accent].map(palette::hex)
        };
        let shown = |cx: &App| {
            let s = themes::shown(cx);
            [
                "pages.selection",
                "pages.find.current",
                "pages.find.other",
                "pages.accent",
            ]
            .map(|k| palette::hex(s[k].unwrap()))
        };
        for (theme, selection) in [("dark", "#EA580C99"), ("light", "#EA580C66")] {
            started(
                AppSettings {
                    theme: theme.into(),
                    ..AppSettings::default()
                },
                cx,
            );
            cx.update(|cx| {
                for k in ["pages.selection", "pages.find.current", "pages.find.other"] {
                    set(k, "#EA580C", cx);
                }
                set("pages.accent", "#123456", cx);
                let want = [selection, "#EA580C99", "#EA580C47", "#123456"];
                assert_eq!(drawn(cx), want, "{theme}");
                assert_eq!(shown(cx), want, "{theme}");
                set("pages.selection", "#EA580C40", cx);
                assert_eq!(drawn(cx)[0], "#EA580C40", "{theme}");
                for k in [
                    "pages.selection",
                    "pages.find.current",
                    "pages.find.other",
                    "pages.accent",
                ] {
                    themes::set_color(k, None, cx);
                }
            });
        }
    }

    /// ⌘W closes the Settings window, as it closes any window, and no other.
    #[gpui_kit::test]
    fn cmd_w_closes_the_settings_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
            crate::ui::keymap::bind_all(cx);
        });
        let other = cx.add_window(|_, _| OnPage(0));
        let (_, vcx) = cx.add_window_view(SettingsView::new);
        vcx.run_until_parked();
        assert_eq!(vcx.update(|_, cx| cx.windows().len()), 2);
        vcx.update(|window, cx| window.press(&crate::ui::keymap::platform_key("cmd-w"), cx));
        vcx.run_until_parked();
        let left = cx.update(|cx| cx.windows());
        assert_eq!(left, [*other]);
    }

    /// The Settings window on Appearance, tall enough for its whole sidebar, Appearance's entry
    /// clicked open.
    fn on_appearance(cx: &mut TestAppContext) -> &mut VisualTestContext {
        started(
            AppSettings {
                theme: "dark".into(),
                ..AppSettings::default()
            },
            cx,
        );
        let (_, vcx) = cx.add_window_view(|_, _| OnPage(1));
        vcx.simulate_resize(size(px(860.0), px(1100.0)));
        vcx.run_until_parked();
        click_entry("0-1", vcx);
        vcx
    }

    /// The sidebar's entries, top to bottom, each by its place in the tree, as the sidebar
    /// numbers its elements: "0-1-2" is the third entry under the second page. Their names are
    /// drawn text, which the test text system leaves out.
    fn entries(vcx: &mut VisualTestContext) -> Vec<String> {
        let sidebar = ElementId::Name("settings-sidebar".into());
        vcx.update(|window, cx| {
            window.render_frame(cx);
            (window.elements().into_iter())
                .filter(|e| e.path().contains(&sidebar))
                .filter_map(|e| match e.path().last() {
                    Some(ElementId::Name(id))
                        if id.split('-').all(|n| n.parse::<usize>().is_ok()) =>
                    {
                        Some(id.to_string())
                    }
                    _ => None,
                })
                .collect()
        })
    }

    /// Clicks the row of the sidebar entry `id`, left of its caret.
    fn click_entry(id: &str, vcx: &mut VisualTestContext) {
        let row = vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(id.to_string()).map(|e| e.bounds().origin)
        });
        let at = row.expect(id) + point(px(20.0), px(14.0));
        vcx.simulate_click(at, Modifiers::default());
        vcx.run_until_parked();
    }

    /// Where the top of element `id` is, while it is on screen.
    fn top(id: &str, vcx: &mut VisualTestContext) -> Option<Pixels> {
        vcx.update(|window, cx| {
            window.render_frame(cx);
            (window.try_find(id.to_string()))
                .filter(|e| e.visible())
                .map(|e| e.bounds().top())
        })
    }

    /// Where the colour group `title` is among the theme's, and its first colour's swatch.
    fn colour_group(title: &str) -> (usize, String) {
        let ix = (themes::groups().iter().position(|g| g.title == title)).unwrap();
        (ix, format!("swatch-{}", themes::groups()[ix].keys[0].key))
    }

    /// Under Appearance the sidebar lists its three groups (Window, Layout, the theme on screen),
    /// and under the third, the theme's, one entry per colour group; the other pages stay closed.
    #[gpui_kit::test]
    fn the_sidebar_nests_the_colour_groups_under_the_theme(cx: &mut TestAppContext) {
        let vcx = on_appearance(cx);
        let mut want: Vec<String> = ["0-0", "0-1", "0-1-0", "0-1-1", "0-1-2"]
            .map(String::from)
            .into();
        want.extend((0..themes::groups().len()).map(|k| format!("0-1-2-{k}")));
        want.extend((2..6).map(|p| format!("0-{p}")));
        assert_eq!(entries(vcx), want);
    }

    /// A click on a colour group's entry scrolls the page to that group, the last one as far as
    /// the page goes; a click on the theme's entry scrolls back to the theme's own settings.
    #[gpui_kit::test]
    fn a_nested_entry_scrolls_to_its_group(cx: &mut TestAppContext) {
        let vcx = on_appearance(cx);
        let (tabs, swatch) = colour_group("Tabs");
        assert!(top(&swatch, vcx).is_none_or(|y| y > px(200.0)));
        click_entry(&format!("0-1-2-{tabs}"), vcx);
        let y = top(&swatch, vcx).expect("the Tabs group on screen");
        // under the page's header and the group's title
        assert!(y > px(60.0) && y < px(200.0), "{y:?}");
        let (syntax, swatch) = colour_group("Source syntax");
        assert!(top(&swatch, vcx).is_none());
        click_entry(&format!("0-1-2-{syntax}"), vcx);
        assert!(
            top(&swatch, vcx).is_some(),
            "the Source syntax group on screen"
        );
        click_entry("0-1-2", vcx);
        let y = top("accent-Blue", vcx).expect("the accent on screen");
        assert!(y > px(60.0) && y < px(200.0), "{y:?}");
    }

    /// Searching a colour's key shows its row alone on Appearance, and in the sidebar the theme
    /// under Appearance and that colour's group under the theme, nothing else.
    #[gpui_kit::test]
    fn searching_a_colour_shows_it_under_the_theme(cx: &mut TestAppContext) {
        let vcx = on_appearance(cx);
        let search = vcx.update(|window, cx| {
            window.render_frame(cx);
            (window.elements().into_iter())
                .find(|e| e.placeholder() == Some("Search..."))
                .map(|e| e.bounds().center())
        });
        vcx.simulate_click(search.expect("the search box"), Modifiers::default());
        vcx.simulate_input("tab.active.background");
        vcx.run_until_parked();
        assert!(swatch("tab.active.background", vcx).is_some());
        assert!(swatch("pages.selection", vcx).is_none());
        // Appearance alone, now first, its entry closed as the first one was
        assert_eq!(entries(vcx), ["0-0"]);
        click_entry("0-0", vcx);
        assert_eq!(entries(vcx), ["0-0", "0-0-0", "0-0-0-0"]);
        click_entry("0-0-0-0", vcx);
        assert!(swatch("tab.active.background", vcx).is_some());
    }

    /// A page with a setting in a group nested in another, which notes whether it was reset.
    struct Nested(Rc<Cell<bool>>);

    impl Render for Nested {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let (dirty, reset) = (self.0.clone(), self.0.clone());
            let item = SettingItem::render(|_, _, _| div().h(px(20.0)))
                .on_reset(move |_| dirty.get(), move |_, _| reset.set(false));
            let parent = SettingGroup::new()
                .title("Parent")
                .groups([SettingGroup::new().title("Child").item(item)]);
            Settings::new("settings").pages(vec![SettingPage::new("Page").group(parent)])
        }
    }

    /// A page's reset reaches the settings of the groups nested in its groups.
    #[gpui_kit::test]
    fn a_page_reset_reaches_nested_groups(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let dirty = Rc::new(Cell::new(true));
        let (_, vcx) = cx.add_window_view(|_, _| Nested(dirty.clone()));
        vcx.run_until_parked();
        let reset = vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find("reset").map(|b| b.bounds().center())
        });
        vcx.simulate_click(reset.expect("the page's reset"), Modifiers::default());
        vcx.run_until_parked();
        assert!(!dirty.get());
    }

    #[test]
    fn every_command_group_has_a_shortcuts_group() {
        assert_eq!(
            shortcut_groups(&crate::ui::keymap::commands()),
            ["File", "Edit", "Insert", "Format", "View"],
            "Insert's commands (Link, Picture, Page Break…) can be rebound too"
        );
    }
}
