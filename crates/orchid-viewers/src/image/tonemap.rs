//! Display mapping for scene-linear float images.
//!
//! Slint presents an 8-bit RGBA framebuffer. Radiance HDR and OpenEXR often
//! store values above 1.0; a straight clamp turns those highlights white.
//! Per-channel Reinhard compression followed by an sRGB transfer keeps them
//! inside the buffer without a 10-bit swapchain.

use image::{DynamicImage, Rgb32FImage, Rgba32FImage};

/// Convert `img` to row-major RGBA8.
///
/// Float buffers are tone-mapped. Every other color type uses the `image`
/// crate's 8-bit conversion unchanged.
///
/// The boolean is `true` when tone mapping ran.
#[must_use]
pub fn to_display_rgba(img: DynamicImage) -> (Vec<u8>, bool) {
    match img {
        DynamicImage::ImageRgb32F(buf) => (tonemap_rgb32f(&buf), true),
        DynamicImage::ImageRgba32F(buf) => (tonemap_rgba32f(&buf), true),
        other => (other.into_rgba8().into_raw(), false),
    }
}

fn tonemap_rgb32f(buf: &Rgb32FImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.as_raw().len() / 3 * 4);
    for px in buf.pixels() {
        let [r, g, b] = px.0;
        out.extend_from_slice(&encoded(r, g, b, 1.0));
    }
    out
}

fn tonemap_rgba32f(buf: &Rgba32FImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.as_raw().len());
    for px in buf.pixels() {
        let [r, g, b, a] = px.0;
        out.extend_from_slice(&encoded(r, g, b, a));
    }
    out
}

fn encoded(r: f32, g: f32, b: f32, a: f32) -> [u8; 4] {
    [
        to_u8(linear_to_srgb(reinhard(r))),
        to_u8(linear_to_srgb(reinhard(g))),
        to_u8(linear_to_srgb(reinhard(b))),
        to_u8(a.clamp(0.0, 1.0)),
    ]
}

/// `v / (1 + v)` on finite non-negative light. Positive infinities land on 1.
fn reinhard(v: f32) -> f32 {
    if v.is_nan() {
        return 0.0;
    }
    if v.is_infinite() {
        return if v.is_sign_positive() { 1.0 } else { 0.0 };
    }
    let v = v.max(0.0);
    v / (1.0 + v)
}

fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_above_one_is_not_clipped_to_white() {
        let mut img = Rgb32FImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgb([4.0, 0.25, 0.05]));
        let (rgba, mapped) = to_display_rgba(DynamicImage::ImageRgb32F(img));
        assert!(mapped);
        assert_eq!(rgba.len(), 4);
        assert!(
            rgba[0] < 250,
            "a finite highlight should stay below white, got {}",
            rgba[0]
        );
        assert!(
            rgba[0] > rgba[1],
            "red highlight should stay brighter than green"
        );
        assert!(rgba[1] > rgba[2], "green should stay brighter than blue");
        assert_eq!(rgba[3], 255);
    }

    #[test]
    fn eight_bit_pixels_are_copied() {
        let mut img = image::RgbImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgb([200, 10, 30]));
        let (rgba, mapped) = to_display_rgba(DynamicImage::ImageRgb8(img));
        assert!(!mapped);
        assert_eq!(&rgba[..3], &[200, 10, 30]);
        assert_eq!(rgba[3], 255);
    }

    #[test]
    fn non_finite_light_does_not_poison_the_pixel() {
        let mut img = Rgb32FImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgb([f32::INFINITY, f32::NAN, -1.0]));
        let (rgba, _) = to_display_rgba(DynamicImage::ImageRgb32F(img));
        assert_eq!(rgba[0], 255);
        assert_eq!(rgba[1], 0);
        assert_eq!(rgba[2], 0);
    }
}
