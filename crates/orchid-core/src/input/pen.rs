//! Pen, palm rejection, and pen double-tap.
//!
//! Finger contacts are dropped while a pen is down when palm rejection is on,
//! including a finger that was already moving when the pen landed. Pen
//! double-tap either does nothing, toggles whether the pen drives shell
//! gestures, or holds the pen in erase (gestures off) until the next toggle.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::event::{Point, TouchEvent, TouchPhase};

const TAP_MAX_MOVEMENT_PX: f32 = 8.0;
const TAP_MAX_DURATION_MS: u32 = 250;
const DOUBLE_TAP_WINDOW_MS: u64 = 300;

/// Finger or pen, as reported by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactKind {
    /// A finger or an unclassified contact.
    Finger,
    /// A stylus.
    Pen,
}

/// What a pen double-tap should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenDoubleTap {
    /// Leave the pen mode unchanged.
    None,
    /// Toggle whether the pen drives shell gestures.
    SwitchTool,
    /// Hold the pen so it does not drive gestures until toggled again.
    Erase,
}

/// Live preferences copied from config for one contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenPrefs {
    /// Drop finger contacts while a pen is down.
    pub palm_rejection: bool,
    /// Action for a pen double-tap.
    pub double_tap: PenDoubleTap,
}

/// One platform contact.
#[derive(Debug, Clone)]
pub struct Contact {
    /// Pointer id.
    pub id: u32,
    /// Finger or pen.
    pub kind: ContactKind,
    /// Lifecycle phase.
    pub phase: TouchPhase,
    /// Position in logical pixels.
    pub position: Point,
    /// Normalised pressure.
    pub pressure: f32,
    /// Contact size in pixels.
    pub size: f32,
    /// When the platform reported it.
    pub timestamp: Instant,
}

/// Result of filtering one contact.
#[derive(Debug, Clone, PartialEq)]
pub enum PenEffect {
    /// Do not feed the gesture recognizer.
    Ignore,
    /// Feed this touch onward.
    Forward(TouchEvent),
    /// Pen double-tap changed the tool. The contact itself is consumed.
    ToolChanged {
        /// When true, later pen contacts drive shell gestures.
        gestures_enabled: bool,
        /// When true, the pen is in erase (gestures stay off).
        erase: bool,
    },
}

#[derive(Debug)]
struct ActivePen {
    began_at: Instant,
    began_pos: Point,
}

/// Tracks pens, rejected palms, and the pen tool mode.
#[derive(Debug)]
pub struct PenSession {
    pens: HashMap<u32, ActivePen>,
    rejected: HashSet<u32>,
    pen_gestures: bool,
    erase: bool,
    last_pen_tap: Option<(Instant, Point)>,
}

impl Default for PenSession {
    fn default() -> Self {
        Self {
            pens: HashMap::new(),
            rejected: HashSet::new(),
            pen_gestures: true,
            erase: false,
            last_pen_tap: None,
        }
    }
}

impl PenSession {
    /// Pen drives gestures; erase is off.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter one contact.
    #[must_use]
    pub fn handle(&mut self, contact: &Contact, prefs: PenPrefs) -> PenEffect {
        if contact.kind == ContactKind::Pen {
            return self.handle_pen(contact, prefs);
        }
        self.handle_finger(contact, prefs)
    }

    fn handle_finger(&mut self, contact: &Contact, prefs: PenPrefs) -> PenEffect {
        let palm = prefs.palm_rejection && !self.pens.is_empty();
        if palm && matches!(contact.phase, TouchPhase::Began) {
            self.rejected.insert(contact.id);
        }
        if self.rejected.contains(&contact.id) || palm {
            if matches!(contact.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.rejected.remove(&contact.id);
            } else if palm {
                self.rejected.insert(contact.id);
            }
            return PenEffect::Ignore;
        }
        PenEffect::Forward(to_touch(contact))
    }

    fn handle_pen(&mut self, contact: &Contact, prefs: PenPrefs) -> PenEffect {
        match contact.phase {
            TouchPhase::Began => {
                self.pens.insert(
                    contact.id,
                    ActivePen {
                        began_at: contact.timestamp,
                        began_pos: contact.position,
                    },
                );
            }
            TouchPhase::Moved => {
                self.pens.entry(contact.id).or_insert(ActivePen {
                    began_at: contact.timestamp,
                    began_pos: contact.position,
                });
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let started = self.pens.remove(&contact.id);
                if matches!(contact.phase, TouchPhase::Ended) {
                    if let Some(effect) = self.double_tap(contact, started.as_ref(), prefs) {
                        return effect;
                    }
                }
            }
        }
        if self.pen_gestures {
            PenEffect::Forward(to_touch(contact))
        } else {
            PenEffect::Ignore
        }
    }

    fn double_tap(
        &mut self,
        contact: &Contact,
        started: Option<&ActivePen>,
        prefs: PenPrefs,
    ) -> Option<PenEffect> {
        let started = started?;
        let duration_ms = contact
            .timestamp
            .saturating_duration_since(started.began_at)
            .as_millis() as u32;
        let distance = started.began_pos.distance_to(contact.position);
        if distance > TAP_MAX_MOVEMENT_PX || duration_ms > TAP_MAX_DURATION_MS {
            self.last_pen_tap = None;
            return None;
        }
        if let Some((last_ts, last_pos)) = self.last_pen_tap {
            let gap = contact.timestamp.saturating_duration_since(last_ts);
            if gap <= Duration::from_millis(DOUBLE_TAP_WINDOW_MS)
                && last_pos.distance_to(contact.position) <= TAP_MAX_MOVEMENT_PX * 2.0
            {
                self.last_pen_tap = None;
                return Some(self.apply_double_tap(prefs.double_tap));
            }
        }
        self.last_pen_tap = Some((contact.timestamp, contact.position));
        None
    }

    fn apply_double_tap(&mut self, action: PenDoubleTap) -> PenEffect {
        match action {
            PenDoubleTap::None => PenEffect::Ignore,
            PenDoubleTap::SwitchTool => {
                if self.erase {
                    self.erase = false;
                    self.pen_gestures = true;
                } else {
                    self.pen_gestures = !self.pen_gestures;
                }
                PenEffect::ToolChanged {
                    gestures_enabled: self.pen_gestures,
                    erase: false,
                }
            }
            PenDoubleTap::Erase => {
                if self.erase {
                    self.erase = false;
                    self.pen_gestures = true;
                    PenEffect::ToolChanged {
                        gestures_enabled: true,
                        erase: false,
                    }
                } else {
                    self.erase = true;
                    self.pen_gestures = false;
                    PenEffect::ToolChanged {
                        gestures_enabled: false,
                        erase: true,
                    }
                }
            }
        }
    }
}

fn to_touch(contact: &Contact) -> TouchEvent {
    TouchEvent {
        pointer_id: contact.id,
        phase: contact.phase,
        position: contact.position,
        pressure: contact.pressure,
        size: contact.size,
        timestamp: contact.timestamp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(ms)
    }

    fn contact(id: u32, kind: ContactKind, phase: TouchPhase, ms: u64) -> Contact {
        Contact {
            id,
            kind,
            phase,
            position: Point::new(10.0, 10.0),
            pressure: 0.5,
            size: 4.0,
            timestamp: at(ms),
        }
    }

    fn prefs(tap: PenDoubleTap) -> PenPrefs {
        PenPrefs {
            palm_rejection: true,
            double_tap: tap,
        }
    }

    #[test]
    fn palm_is_ignored_while_pen_is_down() {
        let mut s = PenSession::new();
        let p = prefs(PenDoubleTap::None);
        assert!(matches!(
            s.handle(&contact(1, ContactKind::Pen, TouchPhase::Began, 0), p),
            PenEffect::Forward(_)
        ));
        assert!(matches!(
            s.handle(&contact(2, ContactKind::Finger, TouchPhase::Began, 5), p),
            PenEffect::Ignore
        ));
        assert!(matches!(
            s.handle(&contact(2, ContactKind::Finger, TouchPhase::Moved, 10), p),
            PenEffect::Ignore
        ));
        let _ = s.handle(&contact(1, ContactKind::Pen, TouchPhase::Ended, 20), p);
        assert!(matches!(
            s.handle(&contact(2, ContactKind::Finger, TouchPhase::Moved, 25), p),
            PenEffect::Ignore
        ));
        assert!(matches!(
            s.handle(&contact(2, ContactKind::Finger, TouchPhase::Ended, 30), p),
            PenEffect::Ignore
        ));
        assert!(matches!(
            s.handle(&contact(3, ContactKind::Finger, TouchPhase::Began, 40), p),
            PenEffect::Forward(_)
        ));
    }

    #[test]
    fn switch_tool_stops_forwarding_the_pen() {
        let mut s = PenSession::new();
        let p = prefs(PenDoubleTap::SwitchTool);
        let base = Instant::now();
        let tap = |phase, ms| Contact {
            id: 7,
            kind: ContactKind::Pen,
            phase,
            position: Point::new(40.0, 40.0),
            pressure: 0.4,
            size: 1.0,
            timestamp: base + Duration::from_millis(ms),
        };
        let _ = s.handle(&tap(TouchPhase::Began, 0), p);
        let _ = s.handle(&tap(TouchPhase::Ended, 40), p);
        let _ = s.handle(&tap(TouchPhase::Began, 120), p);
        let effect = s.handle(&tap(TouchPhase::Ended, 150), p);
        assert_eq!(
            effect,
            PenEffect::ToolChanged {
                gestures_enabled: false,
                erase: false,
            }
        );
        assert!(matches!(
            s.handle(&tap(TouchPhase::Began, 400), p),
            PenEffect::Ignore
        ));
    }

    #[test]
    fn erase_then_erase_again_restores_gestures() {
        let mut s = PenSession::new();
        let p = prefs(PenDoubleTap::Erase);
        let base = Instant::now();
        let tap = |phase, ms| Contact {
            id: 4,
            kind: ContactKind::Pen,
            phase,
            position: Point::new(8.0, 8.0),
            pressure: 0.2,
            size: 1.0,
            timestamp: base + Duration::from_millis(ms),
        };
        let _ = s.handle(&tap(TouchPhase::Began, 0), p);
        let _ = s.handle(&tap(TouchPhase::Ended, 30), p);
        let _ = s.handle(&tap(TouchPhase::Began, 80), p);
        assert_eq!(
            s.handle(&tap(TouchPhase::Ended, 100), p),
            PenEffect::ToolChanged {
                gestures_enabled: false,
                erase: true,
            }
        );
        let _ = s.handle(&tap(TouchPhase::Began, 500), p);
        let _ = s.handle(&tap(TouchPhase::Ended, 530), p);
        let _ = s.handle(&tap(TouchPhase::Began, 600), p);
        assert_eq!(
            s.handle(&tap(TouchPhase::Ended, 620), p),
            PenEffect::ToolChanged {
                gestures_enabled: true,
                erase: false,
            }
        );
    }
}
