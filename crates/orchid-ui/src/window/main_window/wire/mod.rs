//! Slint callback wiring for [`MainWindowController`].

use std::sync::Arc;

use slint::ComponentHandle;

use super::MainWindowController;
use crate::error::Result;

mod apps;
mod clock_search;
mod fm;
mod jyotish;
mod password;
mod processes;
mod shell;
mod terminal;
mod viewer;
mod weather;
mod widgets;

impl MainWindowController {
    pub(super) fn wire_callbacks(self: &Arc<Self>) -> Result<()> {
        let t = Arc::downgrade(self);
        self.wire_shell(&t);
        self.wire_widgets(&t);
        self.wire_terminal(&t);
        self.wire_weather(&t);
        self.wire_jyotish(&t);
        self.wire_clock_search(&t);
        self.wire_apps(&t);
        self.wire_processes(&t);
        self.wire_password(&t);
        self.wire_viewer(&t);
        self.wire_fm(&t);
        Ok(())
    }

    /// OS-level fullscreen / kiosk / next-monitor for the image viewer.
    fn apply_viewer_window_command(&self, cmd: &str) {
        use slint::winit_030::winit::dpi::PhysicalPosition;
        use slint::winit_030::winit::window::Fullscreen;
        use slint::winit_030::WinitWindowAccessor;

        self.window.window().with_winit_window(|win| match cmd {
            "fullscreen" => {
                if win.fullscreen().is_some() {
                    win.set_fullscreen(None);
                } else {
                    win.set_fullscreen(Some(Fullscreen::Borderless(None)));
                }
            }
            "kiosk" => {
                if win.is_decorated() {
                    win.set_decorations(false);
                    win.set_fullscreen(Some(Fullscreen::Borderless(None)));
                } else {
                    win.set_fullscreen(None);
                    win.set_decorations(true);
                }
            }
            "exit-immersive" => {
                win.set_fullscreen(None);
                win.set_decorations(true);
            }
            "next-monitor" => {
                let monitors: Vec<_> = win.available_monitors().collect();
                if monitors.len() < 2 {
                    return;
                }
                let current = win.current_monitor();
                let idx = current
                    .as_ref()
                    .and_then(|cur| monitors.iter().position(|m| m.name() == cur.name()))
                    .unwrap_or(0);
                let next = monitors[(idx + 1) % monitors.len()].clone();
                if win.fullscreen().is_some() {
                    win.set_fullscreen(Some(Fullscreen::Borderless(Some(next))));
                } else {
                    let pos = next.position();
                    win.set_outer_position(PhysicalPosition::new(pos.x, pos.y));
                }
            }
            _ => {}
        });
    }
}
