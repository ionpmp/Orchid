//! Touch / pen gesture recognition.
//!
//! The recogniser is a small synchronous state machine driven by
//! [`GestureRecognizer::feed`] (for raw input events) and
//! [`GestureRecognizer::tick`] (to drive time-based gestures such as long
//! press without additional pointer motion).
//!
//! Gestures are intentionally conservative: false positives are more
//! annoying than false negatives in MVP, so borderline inputs are classified
//! as `Pan` rather than `Swipe` and so on.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use smallvec::{smallvec, SmallVec};

use crate::input::event::{InputEvent, PenEvent, Point, TouchEvent, TouchPhase};
use crate::input::zone::ScreenBounds;

/// A gesture identified by [`GestureRecognizer`].
#[derive(Debug, Clone)]
pub enum RecognizedGesture {
    /// Brief touch-and-release within movement and duration thresholds.
    Tap {
        /// Release position.
        position: Point,
        /// Number of simultaneous pointers that produced this tap.
        pointer_count: u8,
    },
    /// Two [`RecognizedGesture::Tap`]s within
    /// [`GestureConfig::double_tap_window_ms`].
    DoubleTap {
        /// Position of the second tap.
        position: Point,
    },
    /// Touch held without releasing for at least
    /// [`GestureConfig::long_press_ms`].
    LongPress {
        /// Press position.
        position: Point,
        /// Elapsed time when the gesture fired.
        duration_ms: u32,
    },
    /// Directional flick from `from` to `to`.
    Swipe {
        /// Start position.
        from: Point,
        /// End position.
        to: Point,
        /// Cardinal direction.
        direction: SwipeDirection,
        /// Average velocity in pixels per millisecond.
        velocity: f32,
        /// Number of pointers active during the swipe.
        pointer_count: u8,
    },
    /// Swipe starting near a screen edge.
    EdgeSwipe {
        /// Edge the swipe originated from.
        edge: Edge,
        /// Distance travelled inward.
        distance: f32,
        /// Number of pointers active during the swipe.
        pointer_count: u8,
    },
    /// Two-finger pinch (scale change) around a centroid.
    Pinch {
        /// Centroid of the two pointers at detection time.
        center: Point,
        /// Multiplicative scale vs the initial distance.
        scale: f32,
    },
    /// Two-finger rotation around a centroid.
    Rotate {
        /// Centroid of the two pointers at detection time.
        center: Point,
        /// Rotation delta in radians (positive = counter-clockwise).
        radians: f32,
    },
    /// Sustained slow movement that is not a [`RecognizedGesture::Swipe`].
    Pan {
        /// Start position.
        from: Point,
        /// Current / end position.
        to: Point,
        /// Number of pointers active during the pan.
        pointer_count: u8,
    },
}

/// Cardinal direction of a swipe.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Screen edge an edge swipe originated from.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Tunables controlling recogniser thresholds.
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    /// Minimum hold duration for [`RecognizedGesture::LongPress`].
    pub long_press_ms: u32,
    /// Maximum gap between two taps for [`RecognizedGesture::DoubleTap`].
    pub double_tap_window_ms: u32,
    /// Minimum travel distance for [`RecognizedGesture::Swipe`].
    pub swipe_min_distance_px: f32,
    /// Minimum average velocity (px/ms) for [`RecognizedGesture::Swipe`].
    pub swipe_min_velocity: f32,
    /// Distance from an edge within which [`RecognizedGesture::EdgeSwipe`]
    /// is considered.
    pub edge_threshold_px: f32,
    /// Minimum `|scale - 1|` change that fires [`RecognizedGesture::Pinch`].
    pub pinch_min_scale_delta: f32,
    /// Minimum absolute rotation in radians for
    /// [`RecognizedGesture::Rotate`].
    pub rotate_min_radians: f32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            long_press_ms: 500,
            double_tap_window_ms: 300,
            swipe_min_distance_px: 40.0,
            swipe_min_velocity: 0.5,
            edge_threshold_px: 24.0,
            pinch_min_scale_delta: 0.05,
            rotate_min_radians: 0.1,
        }
    }
}

const TAP_MAX_MOVEMENT_PX: f32 = 8.0;
const TAP_MAX_DURATION_MS: u32 = 250;

#[derive(Debug, Clone)]
struct PointerState {
    began_at: Instant,
    began_position: Point,
    last_position: Point,
    long_press_emitted: bool,
}

/// Gesture state machine. See the module-level docs for design notes.
pub struct GestureRecognizer {
    config: RwLock<GestureConfig>,
    bounds: RwLock<ScreenBounds>,
    pointers: HashMap<u32, PointerState>,
    last_tap: Option<(Instant, Point)>,
    /// Tracks the initial distance/angle for two-pointer gestures.
    pair_baseline: Option<PairBaseline>,
}

#[derive(Debug, Clone, Copy)]
struct PairBaseline {
    initial_distance: f32,
    initial_angle: f32,
}

impl std::fmt::Debug for GestureRecognizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GestureRecognizer")
            .field("active_pointers", &self.pointers.len())
            .finish_non_exhaustive()
    }
}

impl GestureRecognizer {
    /// Construct a recogniser with explicit configuration and screen bounds.
    #[must_use]
    pub fn new(config: GestureConfig, bounds: ScreenBounds) -> Self {
        Self {
            config: RwLock::new(config),
            bounds: RwLock::new(bounds),
            pointers: HashMap::new(),
            last_tap: None,
            pair_baseline: None,
        }
    }

    /// Update the known screen bounds (e.g. on DPI / resolution change).
    pub fn set_bounds(&self, bounds: ScreenBounds) {
        *self.bounds.write() = bounds;
    }

    /// Replace the configuration atomically.
    pub fn set_config(&self, config: GestureConfig) {
        *self.config.write() = config;
    }

    /// Forget all in-flight state. Call when input focus is lost.
    pub fn reset(&mut self) {
        self.pointers.clear();
        self.last_tap = None;
        self.pair_baseline = None;
    }

    /// Feed a raw input event and receive any gestures that completed.
    ///
    /// Usually returns zero or one gesture. A release after a long-press in
    /// progress may yield two (e.g. the long-press itself was already
    /// emitted via [`GestureRecognizer::tick`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use orchid_core::{GestureConfig, GestureRecognizer, InputEvent, Point,
    ///                    RecognizedGesture, ScreenBounds, TouchEvent, TouchPhase};
    ///
    /// let mut rec = GestureRecognizer::new(
    ///     GestureConfig::default(),
    ///     ScreenBounds::new(1920.0, 1080.0),
    /// );
    /// let t = Instant::now();
    /// let began = TouchEvent {
    ///     pointer_id: 1, phase: TouchPhase::Began,
    ///     position: Point::new(100.0, 100.0), pressure: 1.0, size: 10.0, timestamp: t,
    /// };
    /// let ended = TouchEvent { phase: TouchPhase::Ended, timestamp: t, ..began.clone() };
    /// let _ = rec.feed(&InputEvent::Touch(began));
    /// let out = rec.feed(&InputEvent::Touch(ended));
    /// assert!(matches!(out.first(), Some(RecognizedGesture::Tap { .. })));
    /// ```
    pub fn feed(
        &mut self,
        event: &InputEvent,
    ) -> SmallVec<[RecognizedGesture; 2]> {
        match event {
            InputEvent::Touch(t) => self.feed_touch(t),
            InputEvent::Pen(p) => self.feed_pen(p),
            // Mouse and keyboard are handled by `InputMapper`, not here.
            InputEvent::Mouse(_) | InputEvent::Keyboard(_) => SmallVec::new(),
        }
    }

    /// Drive time-based gestures (currently: [`RecognizedGesture::LongPress`])
    /// without requiring additional pointer motion.
    pub fn tick(
        &mut self,
        now: Instant,
    ) -> SmallVec<[RecognizedGesture; 2]> {
        let long_press_ms = self.config.read().long_press_ms;
        let threshold = Duration::from_millis(long_press_ms as u64);
        let mut out: SmallVec<[RecognizedGesture; 2]> = SmallVec::new();
        for state in self.pointers.values_mut() {
            if state.long_press_emitted {
                continue;
            }
            if now.saturating_duration_since(state.began_at) >= threshold
                && state.began_position.distance_to(state.last_position) <= TAP_MAX_MOVEMENT_PX
            {
                state.long_press_emitted = true;
                out.push(RecognizedGesture::LongPress {
                    position: state.began_position,
                    duration_ms: long_press_ms,
                });
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // Touch feed
    // ------------------------------------------------------------------

    fn feed_touch(&mut self, t: &TouchEvent) -> SmallVec<[RecognizedGesture; 2]> {
        let mut out: SmallVec<[RecognizedGesture; 2]> = SmallVec::new();
        match t.phase {
            TouchPhase::Began => {
                self.pointers.insert(
                    t.pointer_id,
                    PointerState {
                        began_at: t.timestamp,
                        began_position: t.position,
                        last_position: t.position,
                        long_press_emitted: false,
                    },
                );
                if self.pointers.len() == 2 {
                    self.pair_baseline = Some(self.current_pair_baseline());
                }
            }
            TouchPhase::Moved => {
                if let Some(state) = self.pointers.get_mut(&t.pointer_id) {
                    state.last_position = t.position;
                }
                if self.pointers.len() == 2 {
                    if let Some(baseline) = self.pair_baseline {
                        if let Some(gest) = self.detect_pinch_rotate(baseline) {
                            out.push(gest);
                        }
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some(state) = self.pointers.remove(&t.pointer_id) {
                    if matches!(t.phase, TouchPhase::Ended) {
                        self.finish_pointer(&state, t, &mut out);
                    }
                }
                if self.pointers.len() != 2 {
                    self.pair_baseline = None;
                }
            }
        }
        out
    }

    fn feed_pen(&mut self, p: &PenEvent) -> SmallVec<[RecognizedGesture; 2]> {
        // Treat pen as a single-pointer touch with a dedicated reserved id.
        let synthetic = TouchEvent {
            pointer_id: u32::MAX,
            phase: p.phase,
            position: p.position,
            pressure: p.pressure,
            size: 1.0,
            timestamp: p.timestamp,
        };
        self.feed_touch(&synthetic)
    }

    fn finish_pointer(
        &mut self,
        state: &PointerState,
        ended: &TouchEvent,
        out: &mut SmallVec<[RecognizedGesture; 2]>,
    ) {
        let duration_ms = ended
            .timestamp
            .saturating_duration_since(state.began_at)
            .as_millis() as u32;
        let distance = state.began_position.distance_to(ended.position);
        let pointer_count = (self.pointers.len() + 1) as u8; // +1 for the pointer that just ended

        let cfg = *self.config.read();
        let bounds = *self.bounds.read();

        // Tap / double-tap detection
        if distance <= TAP_MAX_MOVEMENT_PX
            && duration_ms <= TAP_MAX_DURATION_MS
            && !state.long_press_emitted
        {
            if let Some((last_ts, last_pos)) = self.last_tap {
                let gap = ended.timestamp.saturating_duration_since(last_ts);
                if gap <= Duration::from_millis(cfg.double_tap_window_ms as u64)
                    && last_pos.distance_to(ended.position) <= TAP_MAX_MOVEMENT_PX * 2.0
                {
                    out.push(RecognizedGesture::DoubleTap {
                        position: ended.position,
                    });
                    self.last_tap = None;
                    return;
                }
            }
            out.push(RecognizedGesture::Tap {
                position: ended.position,
                pointer_count,
            });
            self.last_tap = Some((ended.timestamp, ended.position));
            return;
        }

        // Swipe / edge-swipe / pan detection
        let velocity = if duration_ms == 0 {
            0.0
        } else {
            distance / duration_ms as f32
        };

        let edge = edge_near(state.began_position, bounds, cfg.edge_threshold_px);
        if let Some(edge) = edge {
            // Edge swipe requires inward motion past the threshold.
            let inward = inward_distance(state.began_position, ended.position, edge);
            if inward >= cfg.edge_threshold_px && distance >= cfg.swipe_min_distance_px {
                out.push(RecognizedGesture::EdgeSwipe {
                    edge,
                    distance,
                    pointer_count,
                });
                return;
            }
        }

        if distance >= cfg.swipe_min_distance_px && velocity >= cfg.swipe_min_velocity {
            let direction = primary_direction(state.began_position, ended.position);
            out.push(RecognizedGesture::Swipe {
                from: state.began_position,
                to: ended.position,
                direction,
                velocity,
                pointer_count,
            });
        } else if distance > TAP_MAX_MOVEMENT_PX {
            out.push(RecognizedGesture::Pan {
                from: state.began_position,
                to: ended.position,
                pointer_count,
            });
        }
    }

    fn current_pair_baseline(&self) -> PairBaseline {
        let mut positions: SmallVec<[Point; 2]> = smallvec![];
        for s in self.pointers.values() {
            positions.push(s.last_position);
        }
        let initial_distance = positions[0].distance_to(positions[1]);
        let initial_angle = (positions[1].y - positions[0].y)
            .atan2(positions[1].x - positions[0].x);
        PairBaseline {
            initial_distance,
            initial_angle,
        }
    }

    fn detect_pinch_rotate(
        &self,
        baseline: PairBaseline,
    ) -> Option<RecognizedGesture> {
        let mut positions: SmallVec<[Point; 2]> = smallvec![];
        for s in self.pointers.values() {
            positions.push(s.last_position);
        }
        if positions.len() != 2 {
            return None;
        }
        let cfg = *self.config.read();
        let distance = positions[0].distance_to(positions[1]);
        let scale = if baseline.initial_distance <= f32::EPSILON {
            1.0
        } else {
            distance / baseline.initial_distance
        };
        if (scale - 1.0).abs() >= cfg.pinch_min_scale_delta {
            let center = Point::new(
                (positions[0].x + positions[1].x) * 0.5,
                (positions[0].y + positions[1].y) * 0.5,
            );
            return Some(RecognizedGesture::Pinch { center, scale });
        }
        let angle = (positions[1].y - positions[0].y)
            .atan2(positions[1].x - positions[0].x);
        let delta = angle - baseline.initial_angle;
        if delta.abs() >= cfg.rotate_min_radians {
            let center = Point::new(
                (positions[0].x + positions[1].x) * 0.5,
                (positions[0].y + positions[1].y) * 0.5,
            );
            return Some(RecognizedGesture::Rotate {
                center,
                radians: delta,
            });
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn edge_near(p: Point, bounds: ScreenBounds, threshold: f32) -> Option<Edge> {
    if p.x <= threshold {
        Some(Edge::Left)
    } else if p.x >= bounds.width - threshold {
        Some(Edge::Right)
    } else if p.y <= threshold {
        Some(Edge::Top)
    } else if p.y >= bounds.height - threshold {
        Some(Edge::Bottom)
    } else {
        None
    }
}

fn inward_distance(from: Point, to: Point, edge: Edge) -> f32 {
    match edge {
        Edge::Left => (to.x - from.x).max(0.0),
        Edge::Right => (from.x - to.x).max(0.0),
        Edge::Top => (to.y - from.y).max(0.0),
        Edge::Bottom => (from.y - to.y).max(0.0),
    }
}

fn primary_direction(from: Point, to: Point) -> SwipeDirection {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            SwipeDirection::Right
        } else {
            SwipeDirection::Left
        }
    } else if dy >= 0.0 {
        SwipeDirection::Down
    } else {
        SwipeDirection::Up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recognizer() -> GestureRecognizer {
        GestureRecognizer::new(
            GestureConfig::default(),
            ScreenBounds::new(1920.0, 1080.0),
        )
    }

    fn touch(id: u32, phase: TouchPhase, x: f32, y: f32, at: Instant) -> InputEvent {
        InputEvent::Touch(TouchEvent {
            pointer_id: id,
            phase,
            position: Point::new(x, y),
            pressure: 1.0,
            size: 10.0,
            timestamp: at,
        })
    }

    #[test]
    fn tap_is_detected() {
        let mut r = recognizer();
        let t = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 100.0, 100.0, t));
        let out = r.feed(&touch(
            1,
            TouchPhase::Ended,
            101.0,
            100.5,
            t + Duration::from_millis(50),
        ));
        assert!(matches!(out.first(), Some(RecognizedGesture::Tap { .. })));
    }

    #[test]
    fn double_tap_is_detected() {
        let mut r = recognizer();
        let t0 = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 100.0, 100.0, t0));
        r.feed(&touch(1, TouchPhase::Ended, 100.0, 100.0, t0 + Duration::from_millis(30)));
        let t1 = t0 + Duration::from_millis(80);
        r.feed(&touch(2, TouchPhase::Began, 101.0, 100.0, t1));
        let out = r.feed(&touch(
            2,
            TouchPhase::Ended,
            101.0,
            100.0,
            t1 + Duration::from_millis(30),
        ));
        assert!(matches!(out.first(), Some(RecognizedGesture::DoubleTap { .. })));
    }

    #[test]
    fn long_press_fires_from_tick() {
        let mut r = recognizer();
        let t0 = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 500.0, 500.0, t0));
        let out = r.tick(t0 + Duration::from_millis(550));
        assert!(matches!(
            out.first(),
            Some(RecognizedGesture::LongPress { duration_ms: 500, .. })
        ));
    }

    #[test]
    fn swipe_direction_and_velocity() {
        let mut r = recognizer();
        let t0 = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 500.0, 500.0, t0));
        let out = r.feed(&touch(
            1,
            TouchPhase::Ended,
            700.0,
            505.0,
            t0 + Duration::from_millis(80),
        ));
        match out.first() {
            Some(RecognizedGesture::Swipe {
                direction, velocity, ..
            }) => {
                assert_eq!(*direction, SwipeDirection::Right);
                assert!(*velocity > 0.5);
            }
            other => panic!("expected swipe, got {other:?}"),
        }
    }

    #[test]
    fn edge_swipe_from_left_edge_detected() {
        let mut r = recognizer();
        let t0 = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 5.0, 500.0, t0));
        let out = r.feed(&touch(
            1,
            TouchPhase::Ended,
            120.0,
            500.0,
            t0 + Duration::from_millis(150),
        ));
        assert!(matches!(
            out.first(),
            Some(RecognizedGesture::EdgeSwipe { edge: Edge::Left, .. })
        ));
    }

    #[test]
    fn pinch_detected_from_two_pointers_separating() {
        let mut r = recognizer();
        let t0 = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 400.0, 500.0, t0));
        r.feed(&touch(2, TouchPhase::Began, 500.0, 500.0, t0));
        let out = r.feed(&touch(
            2,
            TouchPhase::Moved,
            800.0,
            500.0,
            t0 + Duration::from_millis(100),
        ));
        match out.first() {
            Some(RecognizedGesture::Pinch { scale, .. }) => {
                assert!(*scale > 1.0);
            }
            other => panic!("expected pinch, got {other:?}"),
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut r = recognizer();
        let t0 = Instant::now();
        r.feed(&touch(1, TouchPhase::Began, 0.0, 0.0, t0));
        assert_eq!(r.pointers.len(), 1);
        r.reset();
        assert!(r.pointers.is_empty());
    }
}
