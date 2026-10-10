// Single responsibility: Data-only remote UI schema and its host-side interpreter.
//
// Why data, not code: a UI `cdylib` built by stock cargo statically links its own copy of
// aurora, so its thread-locals (hook scopes, ambient runtime stack) are disjoint from the
// host's — calling hooks across that boundary panics. Plain data has no such coupling: the
// dylib describes the interface (`RemoteNode`), the host owns every signal and builds the
// real elements. State therefore lives entirely host-side and trivially survives reloads;
// logic (structure, branches, events) lives dylib-side and swaps freely.
//
// The contract, all plain data by value or borrow:
// - `aurora_remote_state() -> RemoteStateDecl`: state keys and their initial values.
// - `aurora_remote_build(&RemoteSnapshot) -> RemoteNode`: the interface, branching on the
//   current state snapshot the host hands in.
// - `aurora_remote_event(&str) -> Vec<StateWrite>`: named UI actions (from `on_click`) map
//   to state writes the host applies to its signals.
//
// Requirements (dev-only): host and dylib built by the same toolchain (Rust has no stable
// ABI), values cross by move with the process-global allocator on both sides, borrowed
// `&str` arguments are read-only during the call.

use crate::dsl::element::Element;
use crate::dsl::{group, text, IntoElement};
use crate::foundation::{Color, Direction, Size};
use crate::reactive::{ReactiveRuntime, Signal};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

/// A size in the remote schema.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RemoteSize {
    /// Fixed logical pixels.
    Fixed(f32),
    /// Fill the parent.
    Fill,
    /// Fit the content.
    Fit,
}

/// Stacking direction of a remote group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteDirection {
    Horizontal,
    Vertical,
}

/// Text content in the remote schema: literals or live bindings into host state.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteText {
    /// Fixed string.
    Literal(String),
    /// A host string signal, wrapped in an optional affix.
    BoundString {
        path: String,
        prefix: String,
        suffix: String,
    },
    /// A host number signal, rendered with an optional affix.
    BoundF32 {
        path: String,
        prefix: String,
        suffix: String,
    },
    /// A host boolean signal selecting between two strings.
    BoundBool {
        path: String,
        when_true: String,
        when_false: String,
    },
}

/// Node kind in the remote schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteKind {
    Group,
    Text,
}

/// One interface node: plain data, built dylib-side, interpreted host-side.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteNode {
    /// Reconciliation key (stable across reloads to preserve layout caches).
    pub key: String,
    /// Group (container) or Text (leaf).
    pub kind: RemoteKind,
    /// Stacking direction for groups.
    pub direction: RemoteDirection,
    /// Box width.
    pub width: RemoteSize,
    /// Box height.
    pub height: RemoteSize,
    /// Fill as `0xRRGGBB`, if any.
    pub fill: Option<u32>,
    /// Corner radius in pixels (0 disables).
    pub radius: f32,
    /// Gap between children in pixels (0 disables).
    pub gap: f32,
    /// Text content (text nodes should carry exactly one).
    pub text: Option<RemoteText>,
    /// Text size in pixels.
    pub text_size: f32,
    /// Text color as `0xRRGGBB`, if any.
    pub text_color: Option<u32>,
    /// Child nodes, in order.
    pub children: Vec<RemoteNode>,
    /// Named action sent to `aurora_remote_event` on click, if any.
    pub on_click: Option<String>,
}

impl RemoteNode {
    /// An empty group with the given key.
    pub fn group(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            kind: RemoteKind::Group,
            direction: RemoteDirection::Vertical,
            width: RemoteSize::Fill,
            height: RemoteSize::Fill,
            fill: None,
            radius: 0.0,
            gap: 0.0,
            text: None,
            text_size: 14.0,
            text_color: None,
            children: Vec::new(),
            on_click: None,
        }
    }

    /// A literal text leaf with the given key.
    pub fn text(key: impl Into<String>, content: impl Into<String>) -> Self {
        let mut node = Self::group(key);
        node.kind = RemoteKind::Text;
        node.width = RemoteSize::Fit;
        node.height = RemoteSize::Fit;
        node.text = Some(RemoteText::Literal(content.into()));
        node
    }
}

/// State keys a remote UI declares, with the values fresh keys start at.
///
/// Existing host signals keep their live values across reloads; only unknown keys are
/// created from these inits — that asymmetry is what preserves state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemoteStateDecl {
    pub f32s: Vec<(String, f32)>,
    pub strings: Vec<(String, String)>,
    pub bools: Vec<(String, bool)>,
}

/// A point-in-time copy of host state, handed to the dylib so `build` can branch on it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemoteSnapshot {
    pub f32s: Vec<(String, f32)>,
    pub strings: Vec<(String, String)>,
    pub bools: Vec<(String, bool)>,
}

impl RemoteSnapshot {
    /// Current value of a number signal, defaulting to `0.0` when unknown.
    pub fn get_f32(&self, path: &str) -> f32 {
        self.f32s
            .iter()
            .find(|(k, _)| k == path)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }

    /// Current value of a string signal, defaulting to empty when unknown.
    pub fn get_string(&self, path: &str) -> &str {
        self.strings
            .iter()
            .find(|(k, _)| k == path)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    /// Current value of a boolean signal, defaulting to `false` when unknown.
    pub fn get_bool(&self, path: &str) -> bool {
        self.bools
            .iter()
            .find(|(k, _)| k == path)
            .map(|(_, v)| *v)
            .unwrap_or(false)
    }
}

/// One state mutation returned by `aurora_remote_event`.
#[derive(Clone, Debug, PartialEq)]
pub enum StateWrite {
    /// Set a number signal (created when unknown).
    SetF32(String, f32),
    /// Add to a number signal (created as the delta when unknown).
    AddF32(String, f32),
    /// Set a string signal (created when unknown).
    SetString(String, String),
    /// Set a boolean signal (created when unknown).
    SetBool(String, bool),
    /// Flip a boolean signal (created as `true` when unknown).
    ToggleBool(String),
}

/// Declares the UI's state keys and initial values. Implemented dylib-side.
pub type RemoteStateFn = extern "Rust" fn() -> RemoteStateDecl;
/// Builds the interface from the current state snapshot. Implemented dylib-side.
pub type RemoteBuildFn = extern "Rust" fn(&RemoteSnapshot) -> RemoteNode;
/// Maps a named click action to state writes. Implemented dylib-side.
pub type RemoteEventFn = extern "Rust" fn(&str) -> Vec<StateWrite>;
/// Reports the UI crate's remote ABI fingerprint. Implemented dylib-side.
///
/// Separate symbol from the component path's `aurora_hot_version` (one image may serve
/// both loops): each loop checks its own contract and refuses drift loudly.
pub type RemoteVersionFn = extern "Rust" fn() -> u64;

/// Manual protocol generation, folded into the fingerprint below. Bump on semantic
/// contract changes that layouts alone cannot catch.
const REMOTE_PROTO_GEN: u64 = 1;

/// Fingerprint both sides must agree on before entry points install.
///
/// Folds the protocol generation, the size/alignment of every description type
/// crossing the boundary (`RemoteStateDecl`, `RemoteSnapshot`, `RemoteNode`,
/// `StateWrite`), and the compiling toolchain's identity — so signature drift and
/// toolchain skew fail closed with a message instead of misbehaving.
pub fn remote_abi_fingerprint() -> u64 {
    let mut h = crate::hot_reload::FNV_OFFSET;
    h = crate::hot_reload::fold_u64(h, REMOTE_PROTO_GEN);
    for t in [
        (size_of::<RemoteStateDecl>(), align_of::<RemoteStateDecl>()),
        (size_of::<RemoteSnapshot>(), align_of::<RemoteSnapshot>()),
        (size_of::<RemoteNode>(), align_of::<RemoteNode>()),
        (size_of::<StateWrite>(), align_of::<StateWrite>()),
    ] {
        h = crate::hot_reload::fold_u64(h, t.0 as u64);
        h = crate::hot_reload::fold_u64(h, t.1 as u64);
    }
    h = crate::hot_reload::fold_bytes(h, crate::hot_reload::toolchain_id().as_bytes());
    h
}

/// Host side of remote UI: owns every signal, interprets descriptions into elements.
///
/// Signals are keyed by the dylib's state paths and created once per engine session, so a
/// reload (new description, same paths) recalls live values instead of initial ones. Click
/// actions queue into a shared outbox the host drains after each frame.
///
/// Interior-mutable throughout (`RefCell`): the installed root factory holds an `Rc` to the
/// same host the extension drives, so neither side can claim unique ownership.
pub struct RemoteHost {
    runtime: RefCell<Option<Rc<RefCell<ReactiveRuntime>>>>,
    f32s: RefCell<FxHashMap<String, Signal<f32>>>,
    strings: RefCell<FxHashMap<String, Signal<String>>>,
    bools: RefCell<FxHashMap<String, Signal<bool>>>,
    /// Click actions queued by interpreted `on_click` handlers, drained per frame.
    actions: Rc<RefCell<Vec<String>>>,
    fns: RefCell<Option<(RemoteStateFn, RemoteBuildFn, RemoteEventFn)>>,
}

impl RemoteHost {
    /// Creates an unbound host: no runtime, no dylib functions yet.
    pub fn new() -> Self {
        Self {
            runtime: RefCell::new(None),
            f32s: RefCell::new(FxHashMap::default()),
            strings: RefCell::new(FxHashMap::default()),
            bools: RefCell::new(FxHashMap::default()),
            actions: Rc::new(RefCell::new(Vec::new())),
            fns: RefCell::new(None),
        }
    }

    /// Binds the runtime signals allocate on (the engine's, for the session).
    pub fn attach_runtime(&self, runtime: Rc<RefCell<ReactiveRuntime>>) {
        *self.runtime.borrow_mut() = Some(runtime);
    }

    /// Installs (or swaps) the dylib's three entry points after a (re)load.
    pub fn set_fns(&self, state: RemoteStateFn, build: RemoteBuildFn, event: RemoteEventFn) {
        *self.fns.borrow_mut() = Some((state, build, event));
    }

    /// Whether dylib entry points are installed.
    pub fn has_fns(&self) -> bool {
        self.fns.borrow().is_some()
    }

    /// Number of live host signals (all three kinds).
    pub fn signal_count(&self) -> usize {
        self.f32s.borrow().len() + self.strings.borrow().len() + self.bools.borrow().len()
    }

    /// Creates signals for unknown paths in `decl`, keeping live values for known ones.
    ///
    /// Returns `false` when no runtime is attached yet (the caller retries next frame).
    pub fn ensure_state(&self, decl: &RemoteStateDecl) -> bool {
        let Some(runtime) = self.runtime.borrow().clone() else {
            return false;
        };
        for (path, init) in &decl.f32s {
            if !self.f32s.borrow().contains_key(path) {
                self.f32s
                    .borrow_mut()
                    .insert(path.clone(), Signal::new(Rc::clone(&runtime), *init));
            }
        }
        for (path, init) in &decl.strings {
            if !self.strings.borrow().contains_key(path) {
                self.strings.borrow_mut().insert(
                    path.clone(),
                    Signal::new(Rc::clone(&runtime), init.clone()),
                );
            }
        }
        for (path, init) in &decl.bools {
            if !self.bools.borrow().contains_key(path) {
                self.bools
                    .borrow_mut()
                    .insert(path.clone(), Signal::new(Rc::clone(&runtime), *init));
            }
        }
        true
    }

    /// Copies every live signal into a snapshot for the dylib's `build`.
    pub fn snapshot(&self) -> RemoteSnapshot {
        // Bound to locals: each borrow ends before the next begins.
        let f32s = {
            let map = self.f32s.borrow();
            let mut out: Vec<(String, f32)> =
                map.iter().map(|(k, s)| (k.clone(), s.get())).collect();
            out.sort_by(|a, b| a.0.cmp(&b.0));
            out
        };
        let strings = {
            let map = self.strings.borrow();
            let mut out: Vec<(String, String)> =
                map.iter().map(|(k, s)| (k.clone(), s.get())).collect();
            out.sort_by(|a, b| a.0.cmp(&b.0));
            out
        };
        let bools = {
            let map = self.bools.borrow();
            let mut out: Vec<(String, bool)> =
                map.iter().map(|(k, s)| (k.clone(), s.get())).collect();
            out.sort_by(|a, b| a.0.cmp(&b.0));
            out
        };
        RemoteSnapshot { f32s, strings, bools }
    }

    /// Applies event writes to host signals (creating unknown paths from the write).
    pub fn apply_writes(&self, writes: &[StateWrite]) {
        let Some(runtime) = self.runtime.borrow().clone() else {
            return;
        };
        for write in writes {
            match write {
                StateWrite::SetF32(path, v) => {
                    self.signal_f32(&runtime, path).set(*v);
                }
                StateWrite::AddF32(path, d) => {
                    let sig = self.signal_f32(&runtime, path);
                    sig.set(sig.get() + *d);
                }
                StateWrite::SetString(path, v) => {
                    self.signal_string(&runtime, path).set(v.clone());
                }
                StateWrite::SetBool(path, v) => {
                    self.signal_bool(&runtime, path).set(*v);
                }
                StateWrite::ToggleBool(path) => {
                    let sig = self.signal_bool(&runtime, path);
                    sig.set(!sig.get());
                }
            }
        }
    }

    /// Takes queued click actions, leaving the outbox empty.
    pub fn drain_actions(&self) -> Vec<String> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }

    /// Runs the dylib's `build` over the current snapshot. `None` without functions.
    pub fn describe(&self) -> Option<RemoteNode> {
        let fns = self.fns.borrow();
        let (_, build, _) = fns.as_ref()?;
        Some(build(&self.snapshot()))
    }

    /// Interprets one description node into a mountable element.
    pub fn interpret(&self, node: &RemoteNode) -> Element {
        match node.kind {
            RemoteKind::Text => self.interpret_text(node),
            RemoteKind::Group => self.interpret_group(node),
        }
    }

    fn interpret_group(&self, node: &RemoteNode) -> Element {
        let mut out = group()
            .key(node.key.clone())
            .direction(match node.direction {
                RemoteDirection::Horizontal => Direction::Horizontal,
                RemoteDirection::Vertical => Direction::Vertical,
            })
            .width(remote_size(node.width))
            .height(remote_size(node.height));
        if let Some(fill) = node.fill {
            out = out.fill(Color::hex(fill));
        }
        if node.radius > 0.0 {
            out = out.radius(node.radius);
        }
        if node.gap > 0.0 {
            out = out.gap(node.gap);
        }
        if let Some(action) = node.on_click.clone() {
            let actions = Rc::clone(&self.actions);
            out = out.on_click(move |_| {
                actions.borrow_mut().push(action.clone());
            });
        }
        let children: Vec<Element> = node.children.iter().map(|c| self.interpret(c)).collect();
        out.children(children).into_element()
    }

    fn interpret_text(&self, node: &RemoteNode) -> Element {
        let content = match node.text.clone().unwrap_or(RemoteText::Literal(String::new())) {
            RemoteText::Literal(s) => text(s),
            RemoteText::BoundString { path, prefix, suffix } => {
                let sig = self.string_or_default(&path);
                if prefix.is_empty() && suffix.is_empty() {
                    text(sig)
                } else {
                    text(sig.map(move |s| format!("{prefix}{s}{suffix}")))
                }
            }
            RemoteText::BoundF32 { path, prefix, suffix } => {
                let sig = self.f32_or_default(&path);
                text(sig.map(move |v| format!("{prefix}{v}{suffix}")))
            }
            RemoteText::BoundBool { path, when_true, when_false } => {
                let sig = self.bool_or_default(&path);
                text(sig.map(move |b| {
                    if b {
                        when_true.clone()
                    } else {
                        when_false.clone()
                    }
                }))
            }
        };
        let mut out = content.size(node.text_size);
        if let Some(color) = node.text_color {
            out = out.color(Color::hex(color));
        }
        out.key(node.key.clone()).into_element()
    }

    fn signal_f32(&self, runtime: &Rc<RefCell<ReactiveRuntime>>, path: &str) -> Signal<f32> {
        if let Some(sig) = self.f32s.borrow().get(path) {
            return *sig;
        }
        let sig = Signal::new(Rc::clone(runtime), 0.0f32);
        self.f32s.borrow_mut().insert(path.to_string(), sig);
        sig
    }

    fn signal_string(
        &self,
        runtime: &Rc<RefCell<ReactiveRuntime>>,
        path: &str,
    ) -> Signal<String> {
        if let Some(sig) = self.strings.borrow().get(path) {
            return *sig;
        }
        let sig = Signal::new(Rc::clone(runtime), String::new());
        self.strings.borrow_mut().insert(path.to_string(), sig);
        sig
    }

    fn signal_bool(&self, runtime: &Rc<RefCell<ReactiveRuntime>>, path: &str) -> Signal<bool> {
        if let Some(sig) = self.bools.borrow().get(path) {
            return *sig;
        }
        let sig = Signal::new(Rc::clone(runtime), false);
        self.bools.borrow_mut().insert(path.to_string(), sig);
        sig
    }

    /// Existing number signal, or a `0.0` placeholder when the path is undeclared.
    ///
    /// Needs the attached runtime; falls back to a throwaway graph only when unattached,
    /// which binds nothing (tests may attach first; the extension always does).
    fn f32_or_default(&self, path: &str) -> Signal<f32> {
        if let Some(sig) = self.f32s.borrow().get(path) {
            return *sig;
        }
        match self.runtime.borrow().clone() {
            Some(runtime) => self.signal_f32(&runtime, path),
            None => Signal::new(Rc::new(RefCell::new(ReactiveRuntime::new())), 0.0f32),
        }
    }

    fn string_or_default(&self, path: &str) -> Signal<String> {
        if let Some(sig) = self.strings.borrow().get(path) {
            return *sig;
        }
        match self.runtime.borrow().clone() {
            Some(runtime) => self.signal_string(&runtime, path),
            None => Signal::new(Rc::new(RefCell::new(ReactiveRuntime::new())), String::new()),
        }
    }

    fn bool_or_default(&self, path: &str) -> Signal<bool> {
        if let Some(sig) = self.bools.borrow().get(path) {
            return *sig;
        }
        match self.runtime.borrow().clone() {
            Some(runtime) => self.signal_bool(&runtime, path),
            None => Signal::new(Rc::new(RefCell::new(ReactiveRuntime::new())), false),
        }
    }
}

impl Default for RemoteHost {
    fn default() -> Self {
        Self::new()
    }
}

fn remote_size(size: RemoteSize) -> Size {
    match size {
        RemoteSize::Fixed(v) => Size::Fixed(v),
        RemoteSize::Fill => Size::Fill,
        RemoteSize::Fit => Size::Fit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "Rust" fn stub_state() -> RemoteStateDecl {
        RemoteStateDecl {
            f32s: vec![("taps".to_string(), 0.0)],
            strings: vec![],
            bools: vec![("open".to_string(), false)],
        }
    }

    extern "Rust" fn stub_build(snap: &RemoteSnapshot) -> RemoteNode {
        let mut root = RemoteNode::group("root");
        root.width = RemoteSize::Fixed(400.0);
        root.height = RemoteSize::Fixed(300.0);
        root.fill = Some(0x4563EB);
        let mut label = RemoteNode::text("label", "");
        label.text = Some(RemoteText::BoundF32 {
            path: "taps".to_string(),
            prefix: "taps=".to_string(),
            suffix: String::new(),
        });
        label.text_size = 16.0;
        root.children.push(label);
        if snap.get_bool("open") {
            let mut extra = RemoteNode::group("extra");
            extra.width = RemoteSize::Fixed(50.0);
            extra.height = RemoteSize::Fixed(50.0);
            root.children.push(extra);
        }
        root
    }

    extern "Rust" fn stub_event(action: &str) -> Vec<StateWrite> {
        match action {
            "tap" => vec![StateWrite::AddF32("taps".to_string(), 1.0)],
            "toggle" => vec![StateWrite::ToggleBool("open".to_string())],
            _ => Vec::new(),
        }
    }

    fn host() -> RemoteHost {
        let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let mut host = RemoteHost::new();
        host.attach_runtime(runtime);
        host.set_fns(stub_state, stub_build, stub_event);
        host
    }

    #[test]
    fn ensure_state_creates_once_and_preserves_live_values() {
        let mut host = host();
        assert!(host.ensure_state(&stub_state()));
        assert_eq!(host.signal_count(), 2);
        host.apply_writes(&[StateWrite::SetF32("taps".to_string(), 7.0)]);
        // A reload re-declares the same keys: live values win over inits.
        assert!(host.ensure_state(&stub_state()));
        assert_eq!(host.signal_count(), 2);
        let snap = host.snapshot();
        assert_eq!(snap.get_f32("taps"), 7.0);
    }

    #[test]
    fn snapshot_defaults_for_unknown_paths() {
        let snap = RemoteSnapshot::default();
        assert_eq!(snap.get_f32("missing"), 0.0);
        assert_eq!(snap.get_string("missing"), "");
        assert!(!snap.get_bool("missing"));
    }

    #[test]
    fn event_writes_toggle_and_accumulate() {
        let mut host = host();
        assert!(host.ensure_state(&stub_state()));
        host.apply_writes(&stub_event("tap"));
        host.apply_writes(&stub_event("tap"));
        host.apply_writes(&stub_event("toggle"));
        let snap = host.snapshot();
        assert_eq!(snap.get_f32("taps"), 2.0);
        assert!(snap.get_bool("open"));
    }

    #[test]
    fn build_branches_on_snapshot() {
        let host = host();
        let closed = host.interpret(&stub_build(&RemoteSnapshot::default()));
        assert_eq!(closed.key(), Some(crate::foundation::Key::new("root")));
        let mut snap = RemoteSnapshot::default();
        snap.bools.push(("open".to_string(), true));
        let open = host.interpret(&stub_build(&snap));
        assert_eq!(open.key(), Some(crate::foundation::Key::new("root")));
    }
}
