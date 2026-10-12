// Single responsibility: External debug panel UI (Paper-style inspector).
//
// Runs in the debug window's own engine on the session's shared font
// database. Reads plain-data snapshots over the bus; writes go back as
// commands. No reactive state is shared with the app engine, so panel bugs
// cannot corrupt the app view.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::app::debug::InspectorMode;
use crate::app::debug_bus::{DebugBus, DebugCommand, Snapshot};
use crate::app::debug_toggle::TIME_SCALE_STEPS;
use crate::dsl::svg;
use crate::foundation::Color;
use crate::prelude::*;
use crate::reactive::ReactiveRuntime;
use crate::runtime::FrameDiagnostics;
use crate::scene::VectorGraphic;

/// Panel background.
pub const PANEL_BG: u32 = 0x2C2C2C;
/// Hairline borders.
pub fn hairline() -> Color {
    Color::hex_alpha(0xFFFFFF, 0.10)
}
/// Fainter hairlines/dividers.
pub fn hairline_faint() -> Color {
    Color::hex_alpha(0xFFFFFF, 0.06)
}
/// Primary text.
pub const INK: u32 = 0xFFFFFF;
/// Secondary text.
pub const BODY: u32 = 0xB1B1B1;
/// Tertiary text.
pub const MUTED: u32 = 0x747474;
/// Faint label text.
pub const FAINT: u32 = 0x888888;
/// Accent blue.
pub const ACCENT: u32 = 0x0D99FF;
/// Badge blue.
pub const BADGE_BLUE: u32 = 0x4F9EFF;

/// Icon slot indices into [`panel_icons`].
pub const ICON_GEAR: usize = 0;
pub const ICON_POPOUT: usize = 1;
pub const ICON_CROSSHAIR: usize = 2;
pub const ICON_CHEV_UP: usize = 3;
pub const ICON_CHEV_DOWN: usize = 4;
pub const ICON_COPY: usize = 5;
pub const ICON_TRASH: usize = 6;
pub const ICON_MESSAGE: usize = 7;
pub const ICON_FRAME: usize = 8;
pub const ICON_CAMERA: usize = 9;
pub const ICON_UNDO: usize = 10;
pub const ICON_REDO: usize = 11;
pub const ICON_LAYERS: usize = 12;
pub const ICON_SLIDERS: usize = 13;
pub const ICON_SPARKLE: usize = 14;
pub const ICON_PAGE: usize = 15;
pub const ICON_CODE: usize = 16;
pub const ICON_GRID: usize = 17;
pub const ICON_EYE: usize = 18;
pub const ICON_PLAY: usize = 19;
pub const ICON_PALETTE: usize = 20;
pub const ICON_LINK: usize = 21;
pub const ICON_CHEV_RIGHT: usize = 22;

const ICON_SOURCES: [&str; 23] = [
    // gear
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><circle cx="12" cy="12" r="3" fill="none" stroke="#C2C2C2" stroke-width="2"/></svg>"##,
    // popout
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><polyline points="15 3 21 3 21 9" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><line x1="10" y1="14" x2="21" y2="3" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round"/></svg>"##,
    // crosshair
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="none" stroke="#0D99FF" stroke-width="2"/><line x1="22" y1="12" x2="18" y2="12" stroke="#0D99FF" stroke-width="2" stroke-linecap="round"/><line x1="6" y1="12" x2="2" y2="12" stroke="#0D99FF" stroke-width="2" stroke-linecap="round"/><line x1="12" y1="6" x2="12" y2="2" stroke="#0D99FF" stroke-width="2" stroke-linecap="round"/><line x1="12" y1="22" x2="12" y2="18" stroke="#0D99FF" stroke-width="2" stroke-linecap="round"/></svg>"##,
    // chevron up
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><line x1="12" y1="19" x2="12" y2="5" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round"/><polyline points="5 12 12 5 19 12" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // chevron down
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><line x1="12" y1="5" x2="12" y2="19" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round"/><polyline points="19 12 12 19 5 12" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // copy
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><rect x="9" y="9" width="13" height="13" rx="2" fill="none" stroke="#8F8F8F" stroke-width="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // trash
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><polyline points="3 6 5 6 21 6" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // message
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // frame select
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M12.034 12.681a.498.498 0 0 1 .647-.647l9 3.5a.5.5 0 0 1-.033.943l-3.444 1.068a1 1 0 0 0-.66.66l-1.067 3.443a.5.5 0 0 1-.943.033z" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M5 3a2 2 0 0 0-2 2" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round"/><path d="M19 3a2 2 0 0 1 2 2" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round"/><path d="M5 21a2 2 0 0 1-2-2" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round"/><path d="M9 3h1" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round"/></svg>"##,
    // camera
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><circle cx="12" cy="13" r="3" fill="none" stroke="#C2C2C2" stroke-width="2"/></svg>"##,
    // undo
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M3 7v6h6" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // redo
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M21 7v6h-6" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // layers
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24"><polygon points="12 2 2 7 12 12 22 7 12 2" fill="none" stroke="#CDCDCD" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><polyline points="2 17 12 22 22 17" fill="none" stroke="#CDCDCD" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><polyline points="2 12 12 17 22 12" fill="none" stroke="#CDCDCD" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // sliders
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24"><line x1="4" y1="21" x2="4" y2="14" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="4" y1="10" x2="4" y2="3" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="12" y1="21" x2="12" y2="12" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="12" y1="8" x2="12" y2="3" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="20" y1="21" x2="20" y2="16" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="20" y1="12" x2="20" y2="3" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="1" y1="14" x2="7" y2="14" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="9" y1="8" x2="15" y2="8" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/><line x1="17" y1="16" x2="23" y2="16" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"/></svg>"##,
    // sparkle
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24"><path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" fill="none" stroke="#CDCDCD" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // page
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8" viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#4F9EFF" stroke-width="2"/><line x1="15" y1="3" x2="15" y2="21" stroke="#4F9EFF" stroke-width="2"/></svg>"##,
    // code
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="11" height="11" viewBox="0 0 24 24"><polyline points="16 18 22 12 16 6" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><polyline points="8 6 2 12 8 18" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // grid
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7" fill="none" stroke="#CDCDCD" stroke-width="2"/><rect x="14" y="3" width="7" height="7" fill="none" stroke="#CDCDCD" stroke-width="2"/><rect x="14" y="14" width="7" height="7" fill="none" stroke="#CDCDCD" stroke-width="2"/><rect x="3" y="14" width="7" height="7" fill="none" stroke="#CDCDCD" stroke-width="2"/></svg>"##,
    // eye
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" fill="none" stroke="#0D99FF" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><circle cx="12" cy="12" r="3" fill="none" stroke="#0D99FF" stroke-width="2"/></svg>"##,
    // play
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><polygon points="5 3 19 12 5 21 5 3" fill="none" stroke="#CDCDCD" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // palette
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><circle cx="13.5" cy="6.5" r="1" fill="#CDCDCD"/><circle cx="17.5" cy="10.5" r="1" fill="#CDCDCD"/><circle cx="8.5" cy="7.5" r="1" fill="#CDCDCD"/><circle cx="6.5" cy="12" r="1" fill="#CDCDCD"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.5-.7 1.5-1.5 0-.4-.1-.7-.4-1-.3-.3-.4-.7-.4-1 0-.8.7-1.5 1.5-1.5H16c3.3 0 6-2.7 6-6 0-5.5-4.5-10-10-10z" fill="none" stroke="#CDCDCD" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // link
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"><path d="M18.84 12.25l1.72-1.71h-.02a5.004 5.004 0 0 0-.12-7.07 5.006 5.006 0 0 0-6.95 0l-1.72 1.71" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><path d="M5.17 11.75l-1.71 1.71a5.004 5.004 0 0 0 .12 7.07 5.006 5.006 0 0 0 6.95 0l1.71-1.71" fill="none" stroke="#C2C2C2" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
    // chevron right
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24"><polyline points="9 18 15 12 9 6" fill="none" stroke="#8F8F8F" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
];

/// Parses every panel icon once per session. Invalid artwork is skipped with
/// a warning, never a panic: a missing icon must not take down the panel.
pub fn panel_icons() -> Vec<Arc<VectorGraphic>> {
    ICON_SOURCES
        .iter()
        .enumerate()
        .filter_map(|(i, src)| match VectorGraphic::from_str(src) {
            Ok(g) => Some(Arc::new(g)),
            Err(e) => {
                tracing::warn!(icon = i, error = %e, "panel icon failed to parse");
                None
            }
        })
        .collect()
}

/// Renders an icon by slot, or an empty box when its artwork failed to parse
/// (lengths can shrink after skips above — never index directly).
fn panel_icon(icons: &[Arc<VectorGraphic>], icon: usize, size: f32) -> Element {
    match icons.get(icon) {
        Some(g) => view! {
            <Group width={size} height={size} alignment="center" distribution="center">
                {svg(Arc::clone(g)).size(size, size)}
            </Group>
        },
        None => view! {
            <Group width={size} height={size} />
        },
    }
}

/// Local UI state for the panel, on the debug runtime. Snapshot data arrives
/// over the bus; these signals are interaction-only.
#[derive(Clone)]
pub struct PanelUi {
    /// Last snapshot version rendered (drives rebuilds).
    pub version: Signal<u64>,
    /// Active tab: 0 Layers, 1 Design, 2 Changes.
    pub tab: Signal<u8>,
    /// Accordion open flags.
    pub sec_layout: Signal<bool>,
    pub sec_appearance: Signal<bool>,
    pub sec_fill: Signal<bool>,
    pub sec_motion: Signal<bool>,
    pub sec_guide: Signal<bool>,
    /// Last action feedback line.
    pub notice: Signal<String>,
    /// Parsed icon artwork, indexed by `ICON_*`.
    pub icons: Vec<Arc<VectorGraphic>>,
    /// Bridge to the app loop.
    pub bus: DebugBus,
}

impl PanelUi {
    /// Builds panel state on the given (debug) runtime.
    pub fn new(
        runtime: Rc<RefCell<ReactiveRuntime>>,
        bus: DebugBus,
        icons: Vec<Arc<VectorGraphic>>,
    ) -> Self {
        Self {
            version: Signal::new(Rc::clone(&runtime), 0u64),
            tab: Signal::new(Rc::clone(&runtime), 1u8),
            sec_layout: Signal::new(Rc::clone(&runtime), true),
            sec_appearance: Signal::new(Rc::clone(&runtime), false),
            sec_fill: Signal::new(Rc::clone(&runtime), true),
            sec_motion: Signal::new(Rc::clone(&runtime), false),
            sec_guide: Signal::new(Rc::clone(&runtime), false),
            notice: Signal::new(Rc::clone(&runtime), String::new()),
            icons,
            bus,
        }
    }
}

/// Keeps the panel rebuilding as snapshots arrive: version bumps re-render.
pub struct PanelSync {
    bus: DebugBus,
    version: Signal<u64>,
    last: u64,
}

impl PanelSync {
    /// Watches the bus, driving the given version signal.
    pub fn new(bus: DebugBus, version: Signal<u64>) -> Self {
        Self {
            bus,
            version,
            last: 0,
        }
    }
}

impl AppExtension for PanelSync {
    fn on_frame(&mut self, _diagnostics: &FrameDiagnostics) {
        if let Some(snap) = self.bus.latest() {
            if snap.version != self.last {
                self.last = snap.version;
                self.version.set(snap.version);
            }
        }
    }
}

/// Numbered capture path under `$AURORA_CAPTURE_DIR` (default `/tmp`).
fn capture_dir() -> String {
    std::env::var("AURORA_CAPTURE_DIR").unwrap_or_else(|_| "/tmp".into())
}

static PROMPT_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Copies text to the OS clipboard. Returns false when unavailable (e.g.
/// display-less builds without the `os-clipboard` feature).
pub fn copy_text_to_clipboard(text: &str) -> bool {
    #[cfg(feature = "os-clipboard")]
    {
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "panel clipboard write failed");
                false
            }
        }
    }
    #[cfg(not(feature = "os-clipboard"))]
    {
        let _ = text;
        tracing::warn!("panel clipboard unavailable (os-clipboard off)");
        false
    }
}

/// Writes prompt text to a numbered markdown file, returning the path.
pub fn save_prompt_file(text: &str) -> String {
    let n = PROMPT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = format!(
        "{}/aurora-prompt-{}.md",
        capture_dir().trim_end_matches('/'),
        n
    );
    if let Err(e) = std::fs::write(&path, text) {
        tracing::warn!(%path, error = %e, "panel prompt save failed");
    }
    path
}

/// Renders a snapshot as an agent-ready markdown prompt. Pure function.
pub fn prompt_markdown(snap: &Snapshot) -> String {
    let mut out = String::from("# Aurora debug prompt\n\n");
    out.push_str(&format!(
        "- backend: {} · {} fps (avg5 {:.0}) · {:.1} ms (raster {:.1}, present {:.1})\n",
        snap.backend, snap.fps, snap.fps_avg5, snap.total_ms, snap.raster_ms, snap.present_ms
    ));
    out.push_str(&format!(
        "- inspector: {:?} · time: {:.2}x · frame #{}\n",
        snap.inspector, snap.time_scale, snap.frame_index
    ));
    out.push_str(&format!("- demo: {} — {}\n", snap.demo_id, snap.demo_title));
    match &snap.selected {
        Some(n) => out.push_str(&format!(
            "- selected: {} [{}] {:.0}x{:.0} @ {:.0},{:.0}{}\n",
            n.key_path,
            n.kind,
            n.w,
            n.h,
            n.x,
            n.y,
            n.fill
                .as_deref()
                .map(|f| format!(" fill {f}"))
                .unwrap_or_default()
        )),
        None => out.push_str("- selected: none\n"),
    }
    match &snap.hovered {
        Some(n) => out.push_str(&format!(
            "- hovered: {} [{}] {:.0}x{:.0}\n",
            n.key_path, n.kind, n.w, n.h
        )),
        None => out.push_str("- hovered: none\n"),
    }
    out.push_str(&format!("- focus in text: {}\n", snap.focus_present));
    out
}

/// The debug panel root: dark inspector matching the Paper design.
#[component]
pub fn DebugPanel(ui: PanelUi) -> impl IntoElement {
    let _v = ui.version.get();
    // Subscribe to tab switches too: content visibility is prop-driven, but
    // the tab strip itself re-renders on change.
    let _t = ui.tab.get();
    let snap = ui.bus.latest().unwrap_or_default();
    view! {
        <Group direction="vertical" width="fill" height="fill" fill={PANEL_BG}>
            {header(&ui)}
            {toolbar(&ui, &snap)}
            {tabs(&ui)}
            {breadcrumb(&ui, &snap)}
            {pills(&snap)}
            {tab_body(&ui, &snap)}
            {footer(&ui)}
        </Group>
    }
}

fn header(ui: &PanelUi) -> Element {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={40.0}
            fill={PANEL_BG}
            alignment="center"
            gap={8.0}
            margin={Margin::x(12.0)}
        >
            <Group width="fill" />
            <Group width={24.0} height={24.0} alignment="center" distribution="center">
                {panel_icon(&ui.icons, ICON_GEAR, 16.0)}
            </Group>
            <Group width={24.0} height={24.0} alignment="center" distribution="center">
                {panel_icon(&ui.icons, ICON_POPOUT, 15.0)}
            </Group>
        </Group>
    }
    .into_element()
}

fn toolbar(ui: &PanelUi, snap: &Snapshot) -> Element {
    let bus = ui.bus.clone();
    let notice = ui.notice.clone();
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={36.0}
            alignment="center"
            gap={3.0}
            margin={Margin::x(12.0)}
        >
            {icon_btn(ui, ICON_CROSSHAIR, snap.pick_armed, {
                let bus = bus.clone();
                let notice = notice.clone();
                move || {
                    bus.send(DebugCommand::ArmPick);
                    notice.set("pick armed: click the app view".into());
                }
            })}
            {icon_btn(ui, ICON_CHEV_UP, false, {
                let ui2 = ui.clone();
                move || set_all_sections(&ui2, true)
            })}
            {icon_btn(ui, ICON_CHEV_DOWN, false, {
                let ui2 = ui.clone();
                move || set_all_sections(&ui2, false)
            })}
            <Group width="fill" />
            {icon_btn(ui, ICON_COPY, false, {
                let bus = bus.clone();
                let notice = notice.clone();
                move || copy_prompt(&bus, &notice)
            })}
            {icon_btn(ui, ICON_TRASH, false, {
                let bus = bus.clone();
                move || bus.send(DebugCommand::ClearSelection)
            })}
            {icon_btn(ui, ICON_MESSAGE, false, {
                let ui2 = ui.clone();
                move || ui2.tab.set(2)
            })}
            {icon_btn(ui, ICON_FRAME, snap.inspector != InspectorMode::Off, {
                let bus = bus.clone();
                let mode = snap.inspector;
                move || bus.send(DebugCommand::SetInspector(next_inspector(mode)))
            })}
            {icon_btn(ui, ICON_CAMERA, false, {
                let bus = bus.clone();
                let notice = notice.clone();
                move || {
                    bus.send(DebugCommand::Capture);
                    notice.set("capture requested".into());
                }
            })}
            {text_btn(ui, "Tracy", {
                let bus = bus.clone();
                let notice = notice.clone();
                move || {
                    bus.send(DebugCommand::TracySnapshot);
                    notice.set("tracy snapshot requested (T)".into());
                }
            })}
            {text_btn(ui, "Reload", {
                let bus = bus.clone();
                let notice = notice.clone();
                move || {
                    bus.send(DebugCommand::ReloadAll);
                    notice.set("hot-reload queued (F6)".into());
                }
            })}
            <Group
                width={1.0}
                height={16.0}
                fill={Color::hex_alpha(0xFFFFFF, 0.10)}
            />
            {icon_btn(ui, ICON_UNDO, false, {
                let bus = bus.clone();
                let scale = snap.time_scale;
                move || bus.send(DebugCommand::SetTimeScale(slower_scale(scale)))
            })}
            {icon_btn(ui, ICON_REDO, false, {
                let bus = bus.clone();
                let scale = snap.time_scale;
                move || bus.send(DebugCommand::SetTimeScale(faster_scale(scale)))
            })}
        </Group>
    }
    .into_element()
}

fn set_all_sections(ui: &PanelUi, open: bool) {
    ui.sec_layout.set(open);
    ui.sec_appearance.set(open);
    ui.sec_fill.set(open);
    ui.sec_motion.set(open);
    ui.sec_guide.set(open);
}

fn next_inspector(mode: InspectorMode) -> InspectorMode {
    match mode {
        InspectorMode::Off => InspectorMode::DamageHeatmap,
        InspectorMode::DamageHeatmap => InspectorMode::LayoutAndBoundaries,
        InspectorMode::LayoutAndBoundaries => InspectorMode::StackingAndHierarchy,
        InspectorMode::StackingAndHierarchy => InspectorMode::Off,
    }
}

fn slower_scale(scale: f32) -> f32 {
    TIME_SCALE_STEPS
        .iter()
        .rev()
        .find(|s| **s < scale - 1e-4)
        .copied()
        .unwrap_or(TIME_SCALE_STEPS[0])
}

fn faster_scale(scale: f32) -> f32 {
    TIME_SCALE_STEPS
        .iter()
        .find(|s| **s > scale + 1e-4)
        .copied()
        .unwrap_or(1.0)
}

fn copy_prompt(bus: &DebugBus, notice: &Signal<String>) {
    match bus.latest() {
        Some(snap) => {
            let md = prompt_markdown(&snap);
            if copy_text_to_clipboard(&md) {
                notice.set("prompt copied to clipboard".into());
            } else {
                notice.set("clipboard unavailable".into());
            }
        }
        None => notice.set("no snapshot yet".into()),
    }
}

/// Toolbar icon button: blue-tinted when active, dim chrome otherwise.
fn icon_btn(ui: &PanelUi, icon: usize, active: bool, on_press: impl Fn() + 'static) -> Element {
    let (fill, stroke) = if active {
        (
            Color::hex_alpha(0x0D99FF, 0.14),
            Color::hex_alpha(0x0D99FF, 0.30),
        )
    } else {
        (
            Color::hex_alpha(0xFFFFFF, 0.03),
            Color::hex_alpha(0xFFFFFF, 0.05),
        )
    };
    view! {
        <Group
            width={26.0}
            height={26.0}
            radius={5.0}
            fill={fill}
            stroke={Stroke::inside(1.0, stroke)}
            alignment="center"
            distribution="center"
            on:click={{
                move |_: &ClickInfo| on_press()
            }}
        >
            {panel_icon(&ui.icons, icon, 14.0)}
        </Group>
    }
    .into_element()
}

/// Toolbar text button in the same chrome as [`icon_btn`], for actions with no icon slot.
fn text_btn(_ui: &PanelUi, label: &'static str, on_press: impl Fn() + 'static) -> Element {
    view! {
        <Group
            height={26.0}
            radius={5.0}
            fill={Color::hex_alpha(0xFFFFFF, 0.03)}
            stroke={Stroke::inside(1.0, Color::hex_alpha(0xFFFFFF, 0.05))}
            alignment="center"
            distribution="center"
            margin={Margin::x(4.0)}
            on:click={{
                move |_: &ClickInfo| on_press()
            }}
        >
            <Text size={11.0} color={0xBFBFBF}>{label}</Text>
        </Group>
    }
    .into_element()
}

fn tabs(ui: &PanelUi) -> Element {
    let current = ui.tab.clone();
    let mk = |index: u8, icon: usize, label: &'static str| {
        let set = ui.tab.clone();
        let bg = current.clone().map(move |t| {
            if t == index {
                Color::hex_alpha(0xFFFFFF, 0.08)
            } else {
                Color::TRANSPARENT
            }
        });
        let is_sel = current.clone().map(move |t| t == index);
        let is_dim = current.clone().map(move |t| t != index);
        let icons = ui.icons.clone();
        view! {
            <Group
                direction="horizontal"
                width="fill"
                height={28.0}
                radius={5.0}
                fill={bg}
                alignment="center"
                distribution="center"
                gap={3.0}
                on:click={{
                    move |_: &ClickInfo| set.set(index)
                }}
            >
                {panel_icon(&icons, icon, 11.0)}
                <Group direction="horizontal" visible={is_dim} alignment="center" distribution="center">
                    <Text size={10.0} color={0xBFBFBF}>{label}</Text>
                </Group>
                <Group direction="horizontal" visible={is_sel} alignment="center" distribution="center">
                    <Text size={10.0} weight={700u16} color={Color::WHITE}>{label}</Text>
                </Group>
            </Group>
        }
    };
    view! {
        <Group direction="horizontal" width="fill" height={32.0} gap={2.0} margin={Margin::x(12.0)}>
            {mk(0, ICON_LAYERS, "Layers")}
            {mk(1, ICON_SLIDERS, "Design")}
            {mk(2, ICON_SPARKLE, "Changes")}
        </Group>
    }
    .into_element()
}

/// Breadcrumb: Page badge + selected key path + CSS affordance.
fn breadcrumb(_ui: &PanelUi, snap: &Snapshot) -> Element {
    let path = snap
        .selected
        .as_ref()
        .or(snap.hovered.as_ref())
        .map(|n| {
            if n.key_path.is_empty() {
                "<body>".to_string()
            } else {
                n.key_path.clone()
            }
        })
        .unwrap_or_else(|| "<body>".to_string());
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={30.0}
            alignment="center"
            gap={6.0}
            margin={Margin::x(12.0)}
        >
            <Group
                width={44.0}
                height={20.0}
                radius={3.0}
                fill={Color::hex_alpha(0x4F9EFF, 0.12)}
                alignment="center"
                distribution="center"
            >
                <Text size={9.0} weight={700u16} color={BADGE_BLUE}>"Page"</Text>
            </Group>
            <Text size={10.0} color={0x747474}>{path}</Text>
            <Group width="fill" />
            <Group
                width={52.0}
                height={24.0}
                radius={5.0}
                fill={Color::hex_alpha(0xFFFFFF, 0.06)}
                stroke={Stroke::inside(1.0, Color::hex_alpha(0xFFFFFF, 0.10))}
                alignment="center"
                distribution="center"
            >
                <Text size={10.0} color={0xB1B1B1}>"CSS"</Text>
            </Group>
        </Group>
    }
    .into_element()
}

/// State pills reflecting app hover/focus/selection/motion.
fn pills(snap: &Snapshot) -> Element {
    view! {
        <Group direction="horizontal" width="fill" height={24.0} gap={4.0} margin={Margin::x(12.0)}>
            {pill("Hover", snap.hovered.is_some())}
            {pill("Focus", snap.focus_present)}
            {pill("Active", snap.motion_active)}
        </Group>
    }
    .into_element()
}

fn pill(label: &'static str, lit: bool) -> Element {
    let (fill, ink) = if lit {
        (Color::hex_alpha(0xFFFFFF, 0.08), Color::WHITE)
    } else {
        (Color::TRANSPARENT, Color::hex(0x747474))
    };
    view! {
        <Group
            width={64.0}
            height={22.0}
            radius="max"
            fill={fill}
            stroke={Stroke::inside(1.0, Color::hex_alpha(0xFFFFFF, 0.10))}
            alignment="center"
            distribution="center"
        >
            <Text size={10.0} color={ink}>{label}</Text>
        </Group>
    }
    .into_element()
}

fn tab_body(ui: &PanelUi, snap: &Snapshot) -> Element {
    // Visibility toggles, not rebuilds: tab clicks must swap content even
    // when the app is idle (no version bumps, no rebuilds). Live Derived
    // props re-evaluate per frame; bodies run once.
    let show_layers = ui.tab.clone().map(|t| t == 0);
    let show_design = ui.tab.clone().map(|t| t == 1);
    let show_changes = ui.tab.clone().map(|t| t == 2);
    view! {
        <Group direction="vertical" width="fill" height="fill">
            <Group direction="vertical" width="fill" height="fill" visible={show_layers}>
                {layers_tab(ui, snap)}
            </Group>
            <Group direction="vertical" width="fill" height="fill" visible={show_design}>
                {design_tab(ui, snap)}
            </Group>
            <Group direction="vertical" width="fill" height="fill" visible={show_changes}>
                {changes_tab(snap)}
            </Group>
        </Group>
    }
    .into_element()
}

/// Layers tab: selected-node ancestor chain (full tree walk is a follow-up).
fn layers_tab(_ui: &PanelUi, snap: &Snapshot) -> Element {
    let rows: Vec<Element> = match &snap.selected {
        Some(n) => {
            let mut out = vec![ancestor_row("self", &n.key_path, &n.kind)];
            for (i, a) in n.ancestors.iter().enumerate() {
                out.push(ancestor_row(
                    if i == 0 { "parent" } else { "ancestor" },
                    a,
                    "",
                ));
            }
            out
        }
        None => {
            let hovering = snap
                .hovered
                .as_ref()
                .map(|n| format!("hovering: {} [{}]", n.key_path, n.kind))
                .unwrap_or_else(|| "hover the app view, or arm pick (+)".to_string());
            vec![hint_row(&hovering)]
        }
    };
    view! {
        <Group direction="vertical" width="fill" height="fill" gap={2.0} margin={Margin::x(12.0)}>
            {rows}
        </Group>
    }
    .into_element()
}

fn ancestor_row(role: &'static str, path: &str, kind: &str) -> Element {
    let line = if kind.is_empty() {
        format!("{role}: {path}")
    } else {
        format!("{role}: {path} [{kind}]")
    };
    view! {
        <Group direction="horizontal" width="fill" height={22.0} alignment="center">
            <Text size={10.0} color={0xBFBFBF}>{line}</Text>
        </Group>
    }
    .into_element()
}

fn hint_row(text: &str) -> Element {
    view! {
        <Group direction="horizontal" width="fill" height={22.0} alignment="center">
            <Text size={10.0} color={0x747474}>{text.to_string()}</Text>
        </Group>
    }
    .into_element()
}

/// Design tab: inspector sections.
fn design_tab(ui: &PanelUi, snap: &Snapshot) -> Element {
    view! {
        <Group direction="vertical" width="fill" height="fill" gap={2.0}>
            {section(ui, ICON_GRID, "Layout", ui.sec_layout.clone(), layout_body(ui, snap))}
            {section(ui, ICON_EYE, "Appearance", ui.sec_appearance.clone(), collapsed_note("opacity · blend (follow-up)"))}
            {section(ui, ICON_PALETTE, "Fill", ui.sec_fill.clone(), fill_body(snap))}
            {section(ui, ICON_PLAY, "Motion", ui.sec_motion.clone(), collapsed_note("transitions list (follow-up)"))}
            {section(ui, ICON_LINK, "Layout guide", ui.sec_guide.clone(), collapsed_note("guides (follow-up)"))}
        </Group>
    }
    .into_element()
}

fn section(
    ui: &PanelUi,
    icon: usize,
    title: &'static str,
    open: Signal<bool>,
    body: Element,
) -> Element {
    let is_open = open.clone();
    let toggle = open.clone();
    let show_down = open.clone().map(|o| o);
    let show_right = open.clone().map(|o| !o);
    let icons = ui.icons.clone();
    view! {
        <Group direction="vertical" width="fill">
            <Group
                direction="horizontal"
                width="fill"
                height={34.0}
                alignment="center"
                gap={8.0}
                margin={Margin::x(14.0)}
                on:click={{
                    move |_: &ClickInfo| toggle.set(!toggle.get())
                }}
            >
                {panel_icon(&icons, icon, 14.0)}
                <Text size={11.0} weight={700u16} color={0xB1B1B1}>{title}</Text>
                <Group width="fill" />
                <Group direction="horizontal" visible={show_down} alignment="center" distribution="center">
                    {panel_icon(&icons, ICON_CHEV_DOWN, 10.0)}
                </Group>
                <Group direction="horizontal" visible={show_right} alignment="center" distribution="center">
                    {panel_icon(&icons, ICON_CHEV_RIGHT, 10.0)}
                </Group>
            </Group>
            <Group direction="vertical" width="fill" visible={is_open}>
                {body}
            </Group>
        </Group>
    }
    .into_element()
}

fn collapsed_note(text: &'static str) -> Element {
    view! {
        <Group direction="horizontal" width="fill" height={22.0} margin={Margin::x(14.0)}>
            <Text size={10.0} color={0x747474}>{text}</Text>
        </Group>
    }
    .into_element()
}

/// Layout section body: W/H rows with computed values for the selected (or
/// hovered) node.
fn layout_body(_ui: &PanelUi, snap: &Snapshot) -> Element {
    let node = snap.selected.as_ref().or(snap.hovered.as_ref());
    let (w, h) = node
        .map(|n| (n.computed_w.clone(), n.computed_h.clone()))
        .unwrap_or_else(|| ("—".into(), "—".into()));
    let rect = node
        .map(|n| format!("{:.0},{:.0} {:.0}x{:.0}", n.x, n.y, n.w, n.h))
        .unwrap_or_else(|| "nothing selected".into());
    view! {
        <Group direction="vertical" width="fill" gap={6.0} margin={Margin::x(14.0)}>
            {field_row("W", "auto", "Auto", &w)}
            {field_row("H", "auto", "Auto", &h)}
            <Text size={9.0} color={0xBFBFBF}>{"rect ".to_string() + &rect}</Text>
        </Group>
    }
    .into_element()
}

fn field_row(label: &'static str, value: &str, tag: &'static str, computed: &str) -> Element {
    view! {
        <Group direction="vertical" width="fill" gap={2.0}>
            <Group direction="horizontal" width="fill" height={14.0} alignment="center">
                <Text size={10.0} color={0xBFBFBF}>{label}</Text>
            </Group>
            <Group
                direction="horizontal"
                width="fill"
                height={28.0}
                radius={5.0}
                fill={Color::hex_alpha(0xFFFFFF, 0.04)}
                stroke={Stroke::inside(1.0, Color::hex_alpha(0xFFFFFF, 0.10))}
                alignment="center"
                gap={6.0}
                margin={Margin::x(0.0)}
            >
                <Text size={13.0} color={0xBFBFBF}>{value.to_string()}</Text>
                <Group width="fill" />
                <Group
                    height={20.0}
                    radius={3.0}
                    fill={Color::hex_alpha(0xFFFFFF, 0.04)}
                    alignment="center"
                    distribution="center"
                    margin={Margin::x(6.0)}
                >
                    <Text size={9.0} weight={700u16} color={0xBFBFBF}>{tag.to_string()}</Text>
                </Group>
            </Group>
            <Text size={9.0} color={0xBFBFBF}>{"Computed: ".to_string() + computed}</Text>
        </Group>
    }
    .into_element()
}

/// Fill section body: swatch + hex when the node has a solid fill.
fn fill_body(snap: &Snapshot) -> Element {
    let node = snap.selected.as_ref().or(snap.hovered.as_ref());
    match node.and_then(|n| n.fill.clone()) {
        Some(hex) => {
            let (r, g, b) = parse_hex(&hex);
            view! {
                <Group direction="horizontal" width="fill" height={30.0} gap={8.0} margin={Margin::x(14.0)} alignment="center">
                    <Group width={22.0} height={22.0} radius={4.0} fill={Color::hex((r as u32) << 16 | (g as u32) << 8 | b as u32)} />
                    <Text size={11.0} color={Color::WHITE}>{hex}</Text>
                    <Text size={11.0} color={0xBFBFBF}>"100"</Text>
                </Group>
            }
            .into_element()
        }
        None => collapsed_note("no solid fill"),
    }
}

fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let v = u32::from_str_radix(h.get(0..6).unwrap_or("000000"), 16).unwrap_or(0);
    (
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    )
}

/// Changes tab: debugger event log, newest last.
fn changes_tab(snap: &Snapshot) -> Element {
    let rows: Vec<Element> = if snap.events.is_empty() {
        vec![hint_row("no events yet — switch demos, freeze, capture")]
    } else {
        snap.events
            .iter()
            .rev()
            .take(24)
            .map(|e| hint_row(e))
            .collect()
    };
    let cap = match &snap.capture_last {
        Some(p) => format!("last capture: {p}"),
        None => "no captures yet".to_string(),
    };
    let mut all = vec![hint_row(&cap)];
    all.extend(rows);
    view! {
        <Group direction="vertical" width="fill" height="fill" gap={2.0} margin={Margin::x(12.0)}>
            {all}
        </Group>
    }
    .into_element()
}

/// Footer: prompt actions with local feedback.
fn footer(ui: &PanelUi) -> Element {
    let bus = ui.bus.clone();
    let notice = ui.notice.clone();
    let bus2 = ui.bus.clone();
    let notice2 = ui.notice.clone();
    let note = ui.notice.clone().map(|s| s.clone());
    view! {
        <Group direction="vertical" width="fill" gap={4.0} margin={Margin::sides(12.0, 12.0, 12.0, 12.0)}>
            <Group direction="horizontal" width="fill" height={40.0} gap={8.0}>
                <Group
                    width="fill"
                    height="fill"
                    radius={8.0}
                    fill={Color::hex_alpha(0xFFFFFF, 0.03)}
                    stroke={Stroke::inside(1.0, Color::hex_alpha(0xFFFFFF, 0.05))}
                    alignment="center"
                    distribution="center"
                    on:click={{
                        move |_: &ClickInfo| copy_prompt(&bus, &notice)
                    }}
                >
                    <Text size={11.0} weight={700u16} color={0x747474}>"Copy as Prompt"</Text>
                </Group>
                <Group
                    width="fill"
                    height="fill"
                    radius={8.0}
                    fill={Color::hex_alpha(0xFFFFFF, 0.03)}
                    stroke={Stroke::inside(1.0, Color::hex_alpha(0xFFFFFF, 0.05))}
                    alignment="center"
                    distribution="center"
                    on:click={{
                        move |_: &ClickInfo| {
                            match bus2.latest() {
                                Some(snap) => {
                                    let path = save_prompt_file(&prompt_markdown(&snap));
                                    notice2.set(format!("saved {path}"));
                                }
                                None => notice2.set("no snapshot yet".into()),
                            }
                        }
                    }}
                >
                    <Text size={11.0} weight={700u16} color={0x747474}>"Send to your AI agent"</Text>
                </Group>
            </Group>
            <Group direction="horizontal" width="fill" height={14.0} alignment="center" distribution="center">
                <Text size={9.0} color={0x747474}>{note}</Text>
            </Group>
        </Group>
    }
    .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::debug_bus::{NodeBrief, Snapshot};

    fn sample() -> Snapshot {
        Snapshot {
            version: 3,
            fps: 60,
            fps_avg5: 59.0,
            total_ms: 4.2,
            raster_ms: 1.1,
            present_ms: 0.4,
            backend: "tiny-skia".into(),
            inspector: InspectorMode::DamageHeatmap,
            time_scale: 1.0,
            frame_index: 120,
            demo_id: "text-smooth".into(),
            demo_title: "Smooth drift".into(),
            hovered: None,
            selected: Some(NodeBrief {
                key_path: "cta".into(),
                kind: "Group".into(),
                x: 10.0,
                y: 20.0,
                w: 200.0,
                h: 48.0,
                computed_w: "200px".into(),
                computed_h: "48px".into(),
                fill: Some("#2563EB".into()),
                ancestors: vec![],
            }),
            focus_present: false,
            motion_active: true,
            pick_armed: false,
            events: vec!["selected cta".into()],
            capture_last: None,
        }
    }

    #[test]
    fn prompt_contains_snapshot_facts() {
        let md = prompt_markdown(&sample());
        for needle in [
            "tiny-skia",
            "60 fps",
            "DamageHeatmap",
            "text-smooth",
            "cta",
            "200x48",
            "#2563EB",
        ] {
            assert!(md.contains(needle), "prompt missing {needle}:\n{md}");
        }
    }

    #[test]
    fn prompt_without_selection_states_none() {
        let mut snap = sample();
        snap.selected = None;
        let md = prompt_markdown(&snap);
        assert!(md.contains("selected: none"), "{md}");
    }

    #[test]
    fn time_scale_steps_walk_both_ways() {
        assert!((slower_scale(1.0) - 0.5).abs() < 1e-6);
        assert!((faster_scale(0.05) - 0.1).abs() < 1e-6);
        assert!((faster_scale(1.0) - 1.0).abs() < 1e-6);
    }
}
