// The projects (folders) opened before, newest first, kept in projects.json beside the
// settings; each is marked open while its window is, and stays so when the app quits. At launch
// the app opens again those still open; with none, the welcome window lists them all.
use crate::ui::settings::{AppSettings, OnLaunch, support_dir};
use crate::ui::welcome;
use crate::ui::workspace::{MIN_WINDOW, TITLE_BAR_H, Workspace, tour};
use gpui_kit::component::TitleBar;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

actions!(
    realtime_tex,
    [NewProject, OpenProject, CloseProject, ShowTour]
);

/// A project opened before.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Recent {
    pub root: PathBuf,
    /// The file it last showed, relative to its folder.
    #[serde(default)]
    pub file: Option<String>,
    /// Its window was open when the app last quit (or is open now).
    #[serde(default)]
    pub open: bool,
}

/// The recent projects, and the windows open now.
#[derive(Default)]
pub struct Projects {
    pub list: Vec<Recent>,
    /// Each project window: its folder, the window, its workspace.
    windows: Vec<(PathBuf, AnyWindowHandle, WeakEntity<Workspace>)>,
    welcome: Option<AnyWindowHandle>,
    /// Quit was asked once (a second one does not wait for the compile).
    quitting: bool,
}

impl Global for Projects {}

fn file() -> PathBuf {
    support_dir().join("projects.json")
}

impl Projects {
    fn save(&self) {
        // tests open projects too: never the real file
        if cfg!(test) {
            return;
        }
        let _ = std::fs::create_dir_all(support_dir());
        let _ = std::fs::write(
            file(),
            serde_json::to_string_pretty(&self.list).unwrap_or_default(),
        );
    }
}

/// Read the list, and handle the project commands and the app's quitting.
pub fn init(cx: &mut App) {
    let list = match std::fs::read_to_string(file()) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_else(|_| {
            // unreadable: kept aside, not written over by the next save
            let _ = std::fs::rename(file(), file().with_extension("json.bad"));
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    cx.set_global(Projects {
        list,
        ..Default::default()
    });
    cx.on_action(|_: &NewProject, cx| new_project(cx));
    cx.on_action(|_: &OpenProject, cx| choose_and_open(cx));
    cx.on_action(|_: &ShowTour, cx| start_tour(cx));
    cx.on_action(|_: &CloseProject, cx| {
        let active = cx.active_window();
        let found = cx
            .global::<Projects>()
            .windows
            .iter()
            .find(|w| Some(w.1) == active)
            .cloned();
        if let Some((root, handle, ws)) = found {
            closing(&root, &ws, cx);
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    });
    // what each window shows is remembered, and nothing typed is lost; they stay open
    cx.on_app_quit(|cx| {
        let windows = cx.global::<Projects>().windows.clone();
        // the tour is not reopened: its copy goes with it, and what its step unfolded or opened
        // (saved settings) is put back
        for (_, _, ws) in windows.iter().filter(|w| w.0 == tour::dir()) {
            let _ = ws.update(cx, |ws, cx| ws.tour_end(cx));
        }
        for (root, _, ws) in windows.into_iter().filter(|w| w.0 != tour::dir()) {
            let shown = ws
                .update(cx, |ws, cx| {
                    ws.save_closing(cx);
                    ws.shown_file(cx)
                })
                .ok()
                .flatten();
            let keep = AppSettings::get(cx).recent_kept;
            record(
                &mut cx.global_mut::<Projects>().list,
                &root,
                shown,
                true,
                keep,
            );
        }
        cx.global::<Projects>().save();
        // an install half done is removed rather than left to look finished
        crate::engine::cancel_tex_install();
        async {}
    })
    .detach();
}

/// How long quitting waits for each of the compile runs, at most.
const QUIT_RUN_LIMIT: Duration = Duration::from_secs(45);

/// Quit the app. With "Compile when quitting" on, every TeX document is first saved and compiled
/// the chosen number of times; with "Export the PDF when quitting" on, each is then exported
/// beside its file. ⌘Q again quits without waiting.
pub fn quit(cx: &mut App) {
    let s = AppSettings::get(cx);
    let (compile, export) = (s.compile_on_quit, s.export_on_quit);
    let runs = if compile {
        s.compile_runs.clamp(1, 5)
    } else {
        0
    };
    let again = std::mem::replace(&mut cx.global_mut::<Projects>().quitting, true);
    if !(compile || export) || again {
        return cx.quit();
    }
    let windows: Vec<_> = cx
        .global::<Projects>()
        .windows
        .iter()
        .filter(|w| w.0 != tour::dir())
        .map(|w| w.2.clone())
        .collect();
    let mut busy = false;
    for ws in &windows {
        busy |= ws
            .update(cx, |ws, cx| ws.prepare_quit(runs, export, cx))
            .unwrap_or(false);
    }
    if !busy {
        return cx.quit();
    }
    let ws = windows;
    cx.spawn(async move |cx| {
        let until = Instant::now() + QUIT_RUN_LIMIT * (runs + u32::from(export));
        while Instant::now() < until {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let busy = cx.update(|cx| {
                ws.iter().any(|w| {
                    w.read_with(cx, |w, cx| w.quit_compiling(cx))
                        .unwrap_or(false)
                })
            });
            if !busy {
                break;
            }
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

/// The app launched again (its Dock icon clicked) with no window open: the welcome window.
pub fn reopened(cx: &mut App) {
    if cx.windows().is_empty() {
        show_welcome(cx);
    }
}

/// At launch: `arg` (a folder or a .tex file), else the projects open when the app quit, else
/// the welcome window.
pub fn start(arg: Option<PathBuf>, cx: &mut App) {
    // a tour's copy a crash left behind
    tour::remove();
    if AppSettings::get(cx).download_rtex {
        ensure_rtex(cx);
    }
    if let Some(p) = arg {
        return open(&p, cx);
    }
    let again = to_reopen(
        &cx.global::<Projects>().list,
        AppSettings::get(cx).on_launch,
    );
    if again.is_empty() {
        // the very first launch: the tour, unless it could not typeset anything (rtex still
        // downloading starts it once there)
        if first_launch(cx) && crate::engine::find_lualatex().is_none() {
            crate::ui::tex_setup::open_tex_setup(cx);
        }
        return if first_launch(cx) && welcome::setup_ok() {
            start_tour(cx)
        } else {
            show_welcome(cx)
        };
    }
    // the newest last, so its window is in front and it stays first in the list
    for r in again.into_iter().rev() {
        open_in(r.root, r.file, cx);
    }
}

/// Open `path`: a folder, or a .tex file in its folder. A project open already comes to the
/// front.
pub fn open(path: &Path, cx: &mut App) {
    let path = path.canonicalize().unwrap_or(path.to_path_buf());
    if path.is_file() {
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
        open_in(root, name, cx)
    } else {
        open_in(path, None, cx)
    }
}

/// Ask for a folder or a .tex file, and open it.
pub fn choose_and_open(cx: &mut App) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = rx.await
            && let Some(p) = paths.first().cloned()
        {
            cx.update(|cx| open(&p, cx));
        }
    })
    .detach();
}

/// rtex being downloaded (it was nowhere the app looks), or why it could not be.
#[derive(Default, Clone, PartialEq)]
pub enum RtexDownload {
    #[default]
    Idle,
    Running,
    Failed(String),
}

impl Global for RtexDownload {}

/// rtex downloaded for the app when it is nowhere else; then the documents that could not start
/// it start it, and a first launch that waited for it starts the tour.
pub fn ensure_rtex(cx: &mut App) {
    if !crate::engine::needs_download()
        || cx.try_global::<RtexDownload>() == Some(&RtexDownload::Running)
    {
        return;
    }
    let upgrading = crate::engine::upgrading();
    cx.set_global(RtexDownload::Running);
    cx.refresh_windows();
    let task = cx
        .background_executor()
        .spawn(async { crate::engine::download_rtex() });
    cx.spawn(async move |cx| {
        let got = task.await;
        cx.update(|cx| {
            cx.set_global(match &got {
                Ok(_) => RtexDownload::Idle,
                Err(e) => RtexDownload::Failed(e.clone()),
            });
            if got.is_ok() {
                for (_, handle, ws) in cx.global::<Projects>().windows.clone() {
                    let _ = handle.update(cx, |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            // documents running the older rtex move to the new one
                            if upgrading {
                                ws.restart_engines(window, cx)
                            } else {
                                ws.start_stopped(window, cx)
                            }
                        })
                    });
                }
                if first_launch(cx) && welcome::setup_ok() {
                    start_tour(cx);
                }
            }
            cx.refresh_windows();
        })
    })
    .detach();
}

/// LuaLaTeX there now (installed, or found by Check Again): the documents' rtex started again on
/// it, and a first launch that waited for it takes the tour.
pub fn tex_installed(cx: &mut App) {
    restart_engines(cx);
    if first_launch(cx) && welcome::setup_ok() {
        start_tour(cx);
    }
}

/// rtex started again in every open document (on new engine settings, or a TeX installed).
pub fn restart_engines(cx: &mut App) {
    let Some(p) = cx.try_global::<Projects>() else {
        return;
    };
    for (_, handle, ws) in p.windows.clone() {
        let _ = handle.update(cx, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.restart_engines(window, cx))
        });
    }
}

/// The build folders of every open document (their rtex is using them).
pub fn open_build_dirs(cx: &App) -> Vec<PathBuf> {
    let Some(p) = cx.try_global::<Projects>() else {
        return vec![];
    };
    p.windows
        .iter()
        .filter_map(|w| w.2.upgrade())
        .flat_map(|ws| ws.read(cx).build_dirs(cx))
        .collect()
}

/// Nothing opened yet, ever, and the tour not seen: the first launch.
fn first_launch(cx: &App) -> bool {
    let p = cx.global::<Projects>();
    p.list.is_empty()
        && p.windows.is_empty()
        && !crate::ui::settings::AppSettings::get(cx).tour_seen
}

/// The tour, in a fresh copy of its project (or its window to the front and the tour from its
/// first step there, when it is open). Seen once started: closing it does not bring it back.
pub fn start_tour(cx: &mut App) {
    crate::ui::settings::AppSettings::set(cx, |s| s.tour_seen = true);
    let dir = tour::dir();
    let open = cx.global::<Projects>().windows.iter().find(|w| w.0 == dir);
    if let Some((_, handle, ws)) = open.cloned() {
        let _ = handle.update(cx, |_, window, cx| {
            window.activate_window();
            ws.update(cx, |ws, cx| ws.tour_start(window, cx))
        });
        return;
    }
    match tour::create() {
        Ok(d) => open_in(d, Some("main.tex".into()), cx),
        Err(e) => welcome::report(
            format!("Couldn’t start the tour: {}", crate::ui::plain_error(&e)),
            cx,
        ),
    }
}

/// Ask for the new project's folder (the dialog can make one), put a main.tex in it, and open it.
pub fn new_project(cx: &mut App) {
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Create".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = rx.await
            && let Some(path) = paths.into_iter().next()
        {
            cx.update(|cx| match create(&path) {
                Ok(()) => open_in(path, Some("main.tex".into()), cx),
                Err(e) => welcome::report(format!("Couldn’t create the project: {e}"), cx),
            });
        }
    })
    .detach();
}

/// A project's folder at `path`, with a main.tex to start from (one there already is kept).
fn create(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    let main = path.join("main.tex");
    if !main.exists() {
        let name = path
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().to_string());
        std::fs::write(main, template(&name))?;
    }
    Ok(())
}

/// A new project's main.tex, titled `name`.
fn template(name: &str) -> String {
    let title = crate::document::caret::escape(name);
    format!(
        "\\documentclass[12pt]{{article}}\n\\usepackage[margin=1in]{{geometry}}\n\n\\title{{{title}}}\n\\author{{}}\n\\date{{\\today}}\n\n\\begin{{document}}\n\\maketitle\n\n\\section{{Introduction}}\n\nStart writing here.\n\n\\end{{document}}\n"
    )
}

/// Open project `root` (showing `file`, else the file it showed last) in a window of its own,
/// or bring its window to the front.
fn open_in(root: PathBuf, file: Option<String>, cx: &mut App) {
    let open = cx
        .global::<Projects>()
        .windows
        .iter()
        .find(|w| w.0 == root)
        .cloned();
    if let Some((_, handle, ws)) = open {
        let _ = handle.update(cx, |_, window, cx| {
            window.activate_window();
            if let Some(f) = file {
                let _ = ws.update(cx, |ws, cx| ws.open(f, window, cx));
            }
        });
        return;
    }
    if !root.is_dir() {
        return welcome::report(format!("{} is not there any more", root.display()), cx);
    }
    // the file it showed last, while it is still there
    let file = file.or_else(|| {
        let list = &cx.global::<Projects>().list;
        list.iter()
            .find(|r| r.root == root)
            .and_then(|r| r.file.clone())
            .filter(|f| root.join(f).is_file())
    });
    let titlebar = TitlebarOptions {
        traffic_light_position: Some(point(px(13.0), px(TITLE_BAR_H / 2.0 - 6.0))),
        ..TitleBar::title_bar_options()
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1460.0), px(900.0)), cx)),
        titlebar: Some(titlebar),
        window_min_size: Some(size(px(MIN_WINDOW.0), px(MIN_WINDOW.1))),
        ..TitleBar::window_options()
    };
    let at = root.clone();
    let title = root
        .file_name()
        .map_or(root.to_string_lossy(), |n| n.to_string_lossy())
        .to_string();
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        // for the Window menu, Mission Control and VoiceOver
        window.set_window_title(&title);
        let ws = cx.new(|cx| Workspace::new(at.clone(), file, window, cx));
        let weak = ws.downgrade();
        // nothing is lost on close: unsaved edits are written to their files
        window.on_window_should_close(cx, move |_, cx| {
            closing(&at, &weak, cx);
            true
        });
        ws
    });
    let Ok((handle, ws)) = opened else {
        return welcome::report("Couldn’t open a window".into(), cx);
    };
    let p = cx.global_mut::<Projects>();
    p.windows.push((root.clone(), handle, ws.downgrade()));
    let shown = ws.read(cx).shown_file(cx);
    let keep = AppSettings::get(cx).recent_kept;
    let p = cx.global_mut::<Projects>();
    // the tour's scratch copy is not one of the projects
    if root != tour::dir() {
        record(&mut p.list, &root, shown, true, keep);
        p.save();
    }
    // a project open: the welcome window has done its work
    if let Some(w) = p.welcome.take() {
        let _ = w.update(cx, |_, window, _| window.remove_window());
    }
}

/// Project `root`'s window closing: its edits saved, it marked closed with the file it shows;
/// the last one gone, the welcome window.
fn closing(root: &Path, ws: &WeakEntity<Workspace>, cx: &mut App) {
    let shown = ws
        .update(cx, |ws, cx| {
            ws.tour_end(cx);
            ws.save_closing(cx);
            ws.shown_file(cx)
        })
        .ok()
        .flatten();
    if root == tour::dir() {
        // after the window (and its rtex) is gone
        cx.defer(|_| tour::remove());
    }
    let p = cx.global_mut::<Projects>();
    p.windows.retain(|w| w.0 != root);
    if let Some(r) = p.list.iter_mut().find(|r| r.root == root) {
        r.open = false;
        r.file = shown.or(r.file.take());
    }
    p.save();
    if p.windows.is_empty() {
        cx.defer(show_welcome);
    }
}

/// The welcome window, in front (opened if it is not).
pub fn show_welcome(cx: &mut App) {
    if let Some(w) = cx.global::<Projects>().welcome
        && w.update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    if let Some(w) = welcome::open_window(cx) {
        cx.global_mut::<Projects>().welcome = Some(w);
    }
}

/// The window closing is the welcome window: forget it.
pub(crate) fn welcome_closed(cx: &mut App) {
    cx.global_mut::<Projects>().welcome = None;
}

/// Take `root` out of the list.
pub fn forget(root: &Path, cx: &mut App) {
    let p = cx.global_mut::<Projects>();
    p.list.retain(|r| r.root != root);
    p.save();
}

/// `root` first in `list`, open or not, showing `file` (else what it showed before); the list
/// cut to `keep`.
fn record(list: &mut Vec<Recent>, root: &Path, file: Option<String>, open: bool, keep: usize) {
    let before = list
        .iter()
        .position(|r| r.root == root)
        .map(|i| list.remove(i));
    list.insert(
        0,
        Recent {
            root: root.to_path_buf(),
            file: file.or(before.and_then(|r| r.file)),
            open,
        },
    );
    // a hand-edited 0 would empty the list
    list.truncate(keep.max(1));
}

/// The projects to open again at launch: those open when the app quit, still there (none when
/// launching shows the welcome window).
fn to_reopen(list: &[Recent], on_launch: OnLaunch) -> Vec<Recent> {
    list.iter()
        .filter(|r| on_launch == OnLaunch::Reopen && r.open && r.root.is_dir())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Recent, create, record, to_reopen};
    use crate::ui::settings::OnLaunch;
    use std::path::{Path, PathBuf};

    fn r(root: &str, open: bool) -> Recent {
        Recent {
            root: root.into(),
            file: None,
            open,
        }
    }

    #[test]
    fn the_newest_comes_first_once_and_keeps_its_file() {
        let mut list = vec![r("/a", false), r("/b", false)];
        list[1].file = Some("ch1.tex".into());
        record(&mut list, Path::new("/b"), None, true, 30);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].root, PathBuf::from("/b"));
        assert!(list[0].open);
        assert_eq!(list[0].file.as_deref(), Some("ch1.tex"));
        record(
            &mut list,
            Path::new("/b"),
            Some("main.tex".into()),
            true,
            30,
        );
        assert_eq!(list[0].file.as_deref(), Some("main.tex"));
        for i in 0..40 {
            record(&mut list, Path::new(&format!("/p{i}")), None, false, 30);
        }
        assert_eq!(list.len(), 30);
        // fewer kept: the oldest go at the next one opened
        record(&mut list, Path::new("/q"), None, false, 5);
        assert_eq!(list.len(), 5);
        assert_eq!(list[0].root, PathBuf::from("/q"));
    }

    #[test]
    fn launch_opens_what_was_open_and_is_still_there() {
        let here = std::env::temp_dir();
        let list = vec![
            r(here.to_str().unwrap(), true),
            r("/no/such/folder", true),
            r("/", false),
        ];
        let again = to_reopen(&list, OnLaunch::Reopen);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].root, here);
        assert!(to_reopen(&list, OnLaunch::Welcome).is_empty());
    }

    #[test]
    fn a_new_project_starts_from_a_titled_document() {
        let dir = std::env::temp_dir().join(format!("rtd-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("Essay #1");
        create(&path).unwrap();
        let tex = std::fs::read_to_string(path.join("main.tex")).unwrap();
        assert!(tex.contains("\\title{Essay \\#1}"));
        assert!(tex.contains("\\begin{document}") && tex.contains("\\end{document}"));
        // one there already is kept
        std::fs::write(path.join("main.tex"), "mine").unwrap();
        create(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("main.tex")).unwrap(),
            "mine"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
