// Single responsibility: Routing pointer events through the hit chain to per-node handlers.

use super::selection::{
    current_selection, selectable_text, ActiveSelection, PendingPress,
};
use crate::foundation::Key;
use crate::foundation::Point;
use crate::input::event::{ClickInfo, CursorIcon, HoverInfo, PointerEvent, PointerPhase, WheelInfo};
use crate::input::handlers::Propagation;
use crate::input::hit::{hit_chain, HitNode, HitTest};
use crate::input::keyboard::{KeyCode, KeyEvent};
use crate::input::tracker::{PointerTracker, TrackerChange};
use crate::text::selection::{
    caret_line_col, line_byte_ranges, move_grapheme, offset_at_visual_line, snap_to_boundary,
    word_edge, TextSelection,
};
use crate::tree::TreeArena;
use crate::tree::NodeId;

#[derive(Default, Debug)]
pub struct PointerOutcome {
    pub hovered: Option<crate::foundation::Key>,
    pub captured: Option<NodeId>,
    pub dragging: bool,
    pub handled: bool,
}

pub struct PointerRouter {
    pub(crate) tracker: PointerTracker,
    pub(crate) chain: Vec<HitNode>,
    pub(crate) hovered: Option<NodeId>,
    pub(crate) press_chain: Vec<NodeId>,
    pub(crate) clock: f32,
    pub(crate) wall_now: f32,
    pub(crate) active_selection: Option<ActiveSelection>,
    pub(crate) focused: Option<NodeId>,
    pub(crate) last_press: Option<Option<Key>>,
    pub(crate) pending_press: Option<PendingPress>,
    pub(crate) key_goal_x: Option<f32>,
    pub(crate) clipboard: Option<String>,
    pub(crate) pending_collapse: Option<(NodeId, usize)>,
    pub(crate) last_extend_trace: Option<(NodeId, NodeId, usize, usize)>,
    pub(crate) root: Option<NodeId>,
}

impl PointerRouter {
    pub fn new() -> Self {
        Self {
            tracker: PointerTracker::new(),
            chain: Vec::new(),
            hovered: None,
            press_chain: Vec::new(),
            clock: 0.0,
            wall_now: 0.0,
            active_selection: None,
            focused: None,
            last_press: None,
            pending_press: None,
            key_goal_x: None,
            clipboard: None,
            pending_collapse: None,
            last_extend_trace: None,
            root: None,
        }
    }

    #[inline]
    pub fn cursor(&self) -> Option<Point> {
        self.tracker.cursor
    }

    #[inline]
    pub fn captured(&self) -> Option<NodeId> {
        self.tracker.capture()
    }

    #[inline]
    pub fn is_dragging(&self) -> bool {
        self.tracker.is_dragging()
    }

    #[inline]
    pub fn velocity(&self) -> Point {
        self.tracker.velocity()
    }

    pub fn cursor_icon(&self, arena: &TreeArena) -> CursorIcon {
        let node = if self.tracker.is_dragging() {
            self.tracker.capture()
        } else {
            self.hovered
        };
        match node {
            Some(id) if arena.is_valid(id) && selectable_text(arena, id) => CursorIcon::Text,
            _ => CursorIcon::Default,
        }
    }

    #[inline]
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    pub fn has_visible_caret(&self, arena: &TreeArena) -> bool {
        match self.focused {
            Some(id) if arena.is_valid(id) => arena
                .get(id)
                .state
                .text
                .text_selection
                .is_some_and(|s| s.is_collapsed()),
            _ => false,
        }
    }

    pub fn advance_clock(&mut self, dt: f32) { self.clock += dt; }
    pub fn advance_wall(&mut self, dt: f32) { self.wall_now += dt; }
    pub fn clear_hover(&mut self) { self.hovered = None; }

    pub fn cancel_gesture(&mut self) {
        self.clear_hover();
        self.press_chain.clear();
        self.chain.clear();
        self.tracker.cancel_gesture();
        self.pending_collapse = None;
        self.pending_press = None;
    }

    pub fn dispatch(
        &mut self,
        arena: &mut TreeArena,
        root: NodeId,
        event: &PointerEvent,
    ) -> PointerOutcome {
        self.chain = hit_chain(&*arena, root, event.position);
        self.root = Some(root);
        let captured = self.tracker.capture();
        let change = self.tracker.feed(event, self.clock, self.wall_now);

        let mut handled = false;
        match change {
            TrackerChange::Pressed => {
                if let Some(index) = self.chain.iter().position(|hit| absorbs(&*arena, hit.id)) {
                    let target = self.chain[index].id;
                    self.tracker.set_capture(Some(target));
                    let target_key = arena.get(target).key.clone();
                    if self.last_press != Some(target_key.clone()) {
                        self.tracker.reset_burst();
                    }
                    self.last_press = Some(target_key);
                    let mut press_chain: Vec<NodeId> = Vec::new();
                    let mut absorbed = false;
                    for hit in &self.chain[index..] {
                        press_chain.push(hit.id);
                        if arena.get(hit.id).style.hit_test == HitTest::Opaque {
                            absorbed = true;
                            break;
                        }
                    }
                    self.press_chain = press_chain;
                    let draggable = self.press_chain.iter().any(|id| {
                        arena.get(*id).state.input.handlers.handles_drag() || selectable_text(&*arena, *id)
                    });
                    self.tracker.set_drag_enabled(draggable);
                    if absorbs_handlers_only(&*arena, target) {
                        for &id in &self.press_chain {
                            let (fired, stopped) = fire(&*arena, id, |h| h.fire_pointer_down(event));
                            handled |= fired;
                            if stopped { break; }
                        }
                    } else {
                        handled |= absorbed;
                    }
                    self.press_selection(arena, event);
                } else {
                    self.tracker.set_drag_enabled(true);
                    self.clear_active_selection(arena);
                    self.pending_collapse = None;
                    self.pending_press = None;
                    self.set_focus(arena, None);
                    self.tracker.reset_burst();
                    self.last_press = None;
                }
            }
            TrackerChange::Moved => {
                handled |= self.update_hover(&*arena, event.position);
                for hit in &self.chain {
                    let (fired, stopped) = fire(&*arena, hit.id, |h| h.fire_pointer_move(event));
                    handled |= fired;
                    if stopped { break; }
                }
                if self.tracker.is_dragging() {
                    handled |= self.deliver_to(&*arena, captured, |h, info| h.drag_step.as_ref().map(|f| f(info)), event.position, true);
                    self.extend_active_selection(arena, event.position);
                }
            }
            TrackerChange::DragStarted => {
                handled |= self.deliver_to(&*arena, captured, |h, info| h.drag_start.as_ref().map(|f| f(info)), event.position, true);
                self.pending_collapse = None;
                self.extend_active_selection(arena, event.position);
            }
            TrackerChange::Dragged => {
                handled |= self.deliver_to(&*arena, captured, |h, info| h.drag_step.as_ref().map(|f| f(info)), event.position, true);
                self.extend_active_selection(arena, event.position);
            }
            TrackerChange::Released { click } => {
                for &id in self.press_chain.iter() {
                    let (fired_up, stopped_up) = fire(&*arena, id, |h| h.fire_pointer_up(event));
                    handled |= fired_up;
                    if stopped_up { break; }
                    if click > 0 && self.chain.iter().any(|hit| hit.id == id) {
                        let info = ClickInfo { position: event.position, click_count: click };
                        let (fired_click, stopped_click) = fire(&*arena, id, |h| h.fire_click(&info));
                        handled |= fired_click;
                        if stopped_click { break; }
                    }
                }
                self.press_chain.clear();
                self.release_selection(arena, click);
                handled |= self.update_hover(&*arena, event.position);
            }
            TrackerChange::DragEnded => {
                let position = event.position;
                handled |= self.deliver_to(&*arena, captured, |h, info| h.drag_end.as_ref().map(|f| f(info)), position, false);
                self.press_chain.clear();
                self.pending_collapse = None;
                handled |= self.update_hover(&*arena, position);
            }
        }

        PointerOutcome {
            hovered: self.chain.first().and_then(|hit| arena.get(hit.id).key.clone()),
            captured: self.tracker.capture(),
            dragging: self.tracker.is_dragging(),
            handled,
        }
    }

    pub fn dispatch_wheel(&mut self, arena: &mut TreeArena, root: NodeId, info: &WheelInfo) -> bool {
        let chain = hit_chain(&*arena, root, info.position);
        for hit in &chain {
            if !arena.is_valid(hit.id) { continue; }
            let slot = match arena.get(hit.id).state.input.handlers.get() {
                Some(handlers) => handlers.wheel.clone(),
                None => None,
            };
            if let Some(f) = slot {
                let _eval_node = crate::reactive::context::enter_node(hit.id, &*arena);
                f(info);
                return true;
            }
        }
        false
    }

    fn fire_key_chain(&mut self, arena: &mut TreeArena, key: &KeyEvent) -> bool {
        let Some(focused) = self.focused.filter(|n| arena.is_valid(*n)) else { return false; };
        let mut id = Some(focused);
        while let Some(n) = id {
            let slot = match arena.get(n).state.input.handlers.get() {
                Some(handlers) => handlers.key.clone(),
                None => None,
            };
            if let Some(f) = slot {
                let _eval_node = crate::reactive::context::enter_node(n, &*arena);
                f(key);
                return true;
            }
            id = arena.parent(n);
        }
        false
    }

    fn deliver_to(
        &mut self,
        arena: &TreeArena,
        target: Option<NodeId>,
        invoke: fn(&crate::input::handlers::HandlerSet, &crate::input::event::DragInfo) -> Option<Propagation>,
        position: Point,
        held: bool,
    ) -> bool {
        let Some(target) = target else { return false; };
        if !arena.is_valid(target) {
            self.tracker.set_capture(None);
            return false;
        }
        let start = self.press_chain.iter().position(|id| *id == target).unwrap_or(0);
        let info = self.tracker.drag_info(position, held);
        for &id in &self.press_chain[start..] {
            if !arena.is_valid(id) { continue; }
            let slot = match arena.get(id).state.input.handlers.get() {
                Some(handlers) => {
                    let _eval_node = crate::reactive::context::enter_node(id, arena);
                    invoke(handlers, &info)
                }
                None => None,
            };
            if slot.is_some() { return true; }
        }
        false
    }

    fn update_hover(&mut self, arena: &TreeArena, position: Point) -> bool {
        let next = self.chain.iter().find(|hit| absorbs(arena, hit.id)).map(|hit| hit.id);
        if next == self.hovered { return false; }
        let mut handled = false;
        if let Some(leaving) = self.hovered {
            if arena.is_valid(leaving) {
                let info = HoverInfo { position };
                handled |= fire(arena, leaving, |h| h.fire_pointer_leave(&info)).0;
            }
        }
        if let Some(entering) = next {
            let info = HoverInfo { position };
            handled |= fire(arena, entering, |h| h.fire_pointer_enter(&info)).0;
        }
        self.hovered = next;
        handled
    }

    pub fn dispatch_key(&mut self, arena: &mut TreeArena, key: &KeyEvent) -> bool {
        if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown | KeyCode::Char(' ')) {
            self.fire_key_chain(arena, key);
        }
        let Some(node) = self.focused.filter(|n| arena.is_valid(*n) && selectable_text(arena, *n)) else {
            return self.fire_key_chain(arena, key);
        };

        let Some(content) = arena.get(node).text_content() else { return false; };
        let Some(layout) = arena.get(node).state.text.cached_text_layout.clone() else { return false; };
        let content = content.to_string();

        if content.is_empty() || layout.lines.is_empty() { return false; }
        let len = content.len();
        let clamp = |o: usize| snap_to_boundary(&content, o.min(len));
        let current = current_selection(arena, node);
        let caret = current.map(|s| s.focus).unwrap_or(0).min(len);
        let anchor = current.map(|s| s.anchor).unwrap_or(caret).min(len);
        let collapsed = current.is_none_or(|s| s.is_collapsed());

        if matches!(key.code, KeyCode::Char(_)) && key.command_modifier() {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'a' => {
                        return self.apply_key_selection(arena, node, TextSelection { anchor: 0, focus: len });
                    }
                    'c' => {
                        if let Some(text) = self.clipboard_text_for(arena) {
                            if !text.is_empty() { self.clipboard = Some(text); }
                        }
                        return false;
                    }
                    _ => return false,
                }
            }
        }

        let shift = key.shift;
        let extend = |a: usize, f: usize| if shift { (a, f) } else { (f, f) };
        let mut keep_goal = false;
        let next: Option<(usize, usize)> = match key.code {
            KeyCode::Escape => Some((caret, caret)),
            KeyCode::Left => {
                if key.word_modifier() {
                    let f = word_edge(&content, caret, -1);
                    Some(extend(anchor, f))
                } else if !shift && !collapsed {
                    let c = anchor.min(caret);
                    Some((c, c))
                } else {
                    let f = move_grapheme(&content, caret, -1);
                    Some(extend(anchor, f))
                }
            }
            KeyCode::Right => {
                if key.word_modifier() {
                    let f = word_edge(&content, caret, 1);
                    Some(extend(anchor, f))
                } else if !shift && !collapsed {
                    let c = anchor.max(caret);
                    Some((c, c))
                } else {
                    let f = move_grapheme(&content, caret, 1);
                    Some(extend(anchor, f))
                }
            }
            KeyCode::Up | KeyCode::Down => {
                let dir = if matches!(key.code, KeyCode::Up) { -1 } else { 1 };
                let Some(v) = caret_line_col(&layout, &content, caret) else { return false; };
                let (line, x) = v;
                let gx = self.key_goal_x.unwrap_or(x);
                let target = (line as i32 + dir).clamp(0, layout.lines.len() as i32 - 1) as usize;
                let Some(f) = offset_at_visual_line(&layout, &content, target, gx) else { return false; };
                self.key_goal_x = Some(gx);
                keep_goal = true;
                Some(extend(anchor, f))
            }
            KeyCode::Home | KeyCode::End => {
                let f = if key.word_modifier() {
                    if matches!(key.code, KeyCode::Home) { 0 } else { len }
                } else {
                    let Some((line, _)) = caret_line_col(&layout, &content, caret) else { return false; };
                    let ranges = line_byte_ranges(&layout, &content);
                    let (ls, le) = ranges[line];
                    if matches!(key.code, KeyCode::Home) { ls } else { le }
                };
                Some(extend(anchor, f))
            }
            KeyCode::PageUp | KeyCode::PageDown => return false,
            KeyCode::Char(_) => return false,
        };
        if !keep_goal { self.key_goal_x = None; }
        let Some((a, f)) = next else { return false };
        let sel = TextSelection { anchor: clamp(a), focus: clamp(f) };
        if current == Some(sel) { return false; }
        self.apply_key_selection(arena, node, sel)
    }
}

#[inline]
fn absorbs(arena: &TreeArena, id: NodeId) -> bool {
    !arena.get(id).state.input.handlers.is_empty()
        || arena.get(id).style.hit_test == HitTest::Opaque
        || selectable_text(arena, id)
}

#[inline]
fn absorbs_handlers_only(arena: &TreeArena, id: NodeId) -> bool {
    !arena.get(id).state.input.handlers.is_empty()
}

#[inline]
fn fire(
    arena: &TreeArena,
    id: NodeId,
    invoke: impl FnOnce(&crate::input::handlers::HandlerSet) -> Propagation,
) -> (bool, bool) {
    if !arena.is_valid(id) { return (false, false); }
    match arena.get(id).state.input.handlers.get() {
        Some(handlers) => {
            let _eval_node = crate::reactive::context::enter_node(id, arena);
            let propagation = invoke(handlers);
            (true, propagation == Propagation::Stop)
        }
        None => (false, false),
    }
}

impl Default for PointerRouter {
    fn default() -> Self { Self::new() }
}

#[inline]
pub fn is_press(event: &PointerEvent) -> bool {
    event.phase == PointerPhase::Down
}
