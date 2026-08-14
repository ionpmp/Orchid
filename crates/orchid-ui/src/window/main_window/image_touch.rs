//! Pinch zoom and two-finger pan for the last focused image viewer.

use std::collections::HashMap;
use std::sync::LazyLock;

use orchid_core::{TouchEvent, TouchPhase};
use parking_lot::Mutex;
use uuid::Uuid;

/// Incremental pinch / two-finger pan to apply to an image viewer.
pub(super) enum ImageTouchAction {
    Zoom(f32),
    Pan(f32, f32),
}

struct State {
    last_viewer: Option<Uuid>,
    points: HashMap<u32, (f32, f32)>,
    prev_dist: Option<f32>,
    prev_mid: Option<(f32, f32)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_viewer: None,
            points: HashMap::new(),
            prev_dist: None,
            prev_mid: None,
        }
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

/// Remember which viewer should receive pinch / two-finger pan.
pub(super) fn remember_viewer(id: Uuid) {
    STATE.lock().last_viewer = Some(id);
}

/// Last image viewer that received a zoom / pan / command.
#[must_use]
pub(super) fn last_viewer() -> Option<Uuid> {
    STATE.lock().last_viewer
}

/// Update tracked pointers and emit incremental zoom / pan when two fingers move.
#[must_use]
pub(super) fn on_touch(ev: &TouchEvent) -> Vec<ImageTouchAction> {
    let mut st = STATE.lock();
    let mut out = Vec::new();
    match ev.phase {
        TouchPhase::Began => {
            st.points
                .insert(ev.pointer_id, (ev.position.x, ev.position.y));
            if st.points.len() != 2 {
                st.prev_dist = None;
                st.prev_mid = None;
            }
        }
        TouchPhase::Moved => {
            st.points
                .insert(ev.pointer_id, (ev.position.x, ev.position.y));
            if st.points.len() == 2 {
                let pts: Vec<(f32, f32)> = st.points.values().copied().collect();
                let dist = (pts[0].0 - pts[1].0).hypot(pts[0].1 - pts[1].1).max(1.0);
                let mid = ((pts[0].0 + pts[1].0) * 0.5, (pts[0].1 + pts[1].1) * 0.5);
                if let Some(pd) = st.prev_dist {
                    let factor = dist / pd;
                    if (factor - 1.0).abs() > 0.008 {
                        out.push(ImageTouchAction::Zoom(factor));
                    }
                }
                if let Some(pm) = st.prev_mid {
                    let dx = mid.0 - pm.0;
                    let dy = mid.1 - pm.1;
                    if dx.abs() > 0.3 || dy.abs() > 0.3 {
                        out.push(ImageTouchAction::Pan(dx, dy));
                    }
                }
                st.prev_dist = Some(dist);
                st.prev_mid = Some(mid);
            }
        }
        TouchPhase::Ended | TouchPhase::Cancelled => {
            st.points.remove(&ev.pointer_id);
            st.prev_dist = None;
            st.prev_mid = None;
        }
    }
    out
}
