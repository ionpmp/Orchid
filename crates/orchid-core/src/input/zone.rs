//! Ergonomic screen zones.
//!
//! Orchid's design philosophy splits the screen into four zones that map
//! roughly to how comfortable each region is for a one-handed user holding
//! a tablet:
//!
//! | zone    | location                                                 | comfort |
//! |---------|----------------------------------------------------------|---------|
//! | Hot     | bottom-center band (30 % tall × 40 % wide)               | easiest |
//! | Warm    | outer bottom band (flanks of `Hot`)                      | easy    |
//! | Neutral | middle of screen, plus top-center band                   | ok      |
//! | Cold    | upper corners (top 20 % height × outermost 20 % width)   | hardest |

use crate::input::event::Point;

/// Ergonomic classification of a screen location.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenZone {
    Hot,
    Warm,
    Neutral,
    Cold,
}

/// Physical screen dimensions in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct ScreenBounds {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
}

impl ScreenBounds {
    /// Construct a [`ScreenBounds`].
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl ScreenZone {
    /// Classify `point` against a screen of size `bounds`.
    ///
    /// Points outside the bounds are classified as [`ScreenZone::Neutral`].
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{Point, ScreenBounds, ScreenZone};
    /// let b = ScreenBounds::new(1920.0, 1080.0);
    /// assert_eq!(ScreenZone::classify(Point::new(0.0, 0.0), b), ScreenZone::Cold);
    /// assert_eq!(
    ///     ScreenZone::classify(Point::new(960.0, 972.0), b),
    ///     ScreenZone::Hot
    /// );
    /// ```
    #[must_use]
    pub fn classify(point: Point, bounds: ScreenBounds) -> Self {
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return Self::Neutral;
        }
        if point.x < 0.0 || point.x > bounds.width || point.y < 0.0 || point.y > bounds.height {
            return Self::Neutral;
        }

        let nx = point.x / bounds.width;
        let ny = point.y / bounds.height;

        // Upper 20% of height...
        if ny <= 0.20 {
            // ...within the outermost 20% horizontally → Cold corner.
            if nx <= 0.20 || nx >= 0.80 {
                return Self::Cold;
            }
            // ...otherwise (top-center band) → Neutral.
            return Self::Neutral;
        }

        // Lower 30% of height...
        if ny >= 0.70 {
            // ...within the central 40% horizontally → Hot.
            if (0.30..=0.70).contains(&nx) {
                return Self::Hot;
            }
            // ...outer bands → Warm.
            return Self::Warm;
        }

        // Middle of the screen → Neutral.
        Self::Neutral
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b() -> ScreenBounds {
        ScreenBounds::new(1920.0, 1080.0)
    }

    #[test]
    fn corners_classify_as_cold() {
        assert_eq!(ScreenZone::classify(Point::new(0.0, 0.0), b()), ScreenZone::Cold);
        assert_eq!(
            ScreenZone::classify(Point::new(1920.0, 0.0), b()),
            ScreenZone::Cold
        );
    }

    #[test]
    fn top_center_is_neutral() {
        assert_eq!(
            ScreenZone::classify(Point::new(960.0, 50.0), b()),
            ScreenZone::Neutral
        );
    }

    #[test]
    fn bottom_center_is_hot() {
        assert_eq!(
            ScreenZone::classify(Point::new(960.0, 1000.0), b()),
            ScreenZone::Hot
        );
    }

    #[test]
    fn bottom_corners_are_warm() {
        assert_eq!(
            ScreenZone::classify(Point::new(20.0, 1060.0), b()),
            ScreenZone::Warm
        );
        assert_eq!(
            ScreenZone::classify(Point::new(1900.0, 1060.0), b()),
            ScreenZone::Warm
        );
    }

    #[test]
    fn middle_is_neutral() {
        assert_eq!(
            ScreenZone::classify(Point::new(960.0, 540.0), b()),
            ScreenZone::Neutral
        );
    }

    #[test]
    fn out_of_bounds_is_neutral() {
        assert_eq!(
            ScreenZone::classify(Point::new(-1.0, 0.0), b()),
            ScreenZone::Neutral
        );
        assert_eq!(
            ScreenZone::classify(Point::new(5000.0, 540.0), b()),
            ScreenZone::Neutral
        );
    }
}
