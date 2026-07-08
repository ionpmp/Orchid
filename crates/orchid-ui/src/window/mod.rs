//! Per-window controllers.

pub mod main_window;
pub mod startup;

pub use main_window::MainWindowController;
pub use startup::StartupWindowController;

/// UI scale for the given density, with Hybrid adjusted by canvas viewport width.
#[must_use]
pub(crate) fn effective_ui_scale(density: orchid_storage::Density, canvas_width: f32) -> f32 {
    match density {
        orchid_storage::Density::Hybrid => hybrid_viewport_scale(canvas_width),
        d => d.ui_scale(),
    }
}

fn hybrid_viewport_scale(width: f32) -> f32 {
    const TOUCH: f32 = 1.2;
    const HYBRID: f32 = 1.0;
    const MOUSE: f32 = 0.8;
    if width < 1100.0 {
        let t = (width / 1100.0).clamp(0.0, 1.0);
        TOUCH + t * (HYBRID - TOUCH)
    } else if width > 1600.0 {
        let t = ((width - 1600.0) / 400.0).clamp(0.0, 1.0);
        HYBRID + t * (MOUSE - HYBRID)
    } else {
        HYBRID
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_storage::Density;

    #[test]
    fn hybrid_scale_at_breakpoints() {
        assert!((hybrid_viewport_scale(1100.0) - 1.0).abs() < f32::EPSILON);
        assert!((hybrid_viewport_scale(1600.0) - 1.0).abs() < f32::EPSILON);
        assert!(hybrid_viewport_scale(0.0) > 1.15);
        assert!(hybrid_viewport_scale(2000.0) < 0.85);
    }

    #[test]
    fn non_hybrid_uses_fixed_scale() {
        assert!((effective_ui_scale(Density::Touch, 800.0) - 1.2).abs() < f32::EPSILON);
        assert!((effective_ui_scale(Density::Mouse, 2000.0) - 0.8).abs() < f32::EPSILON);
    }
}
