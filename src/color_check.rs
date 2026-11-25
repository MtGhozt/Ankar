//! Gamma check: paint known colours, read them back from the framebuffer (egui screenshot) and
//! log both. Solid fills must come back byte-exact; anything else means a colour-space bug.

use egui::{Color32, ColorImage, Pos2, Rect, Vec2};

pub const SWATCHES: [Color32; 3] = [
    Color32::from_rgb(0x0a, 0x0a, 0x0a),
    Color32::from_rgb(0x1e, 0x1e, 0x1e),
    Color32::from_rgb(0x76, 0x99, 0xFE),
];
const SIZE: f32 = 32.0;

pub fn swatch_rects(origin: Pos2) -> [Rect; 3] {
    std::array::from_fn(|i| Rect::from_min_size(origin + Vec2::new(i as f32 * SIZE, 0.0), Vec2::splat(SIZE)))
}

/// `(expected, read back)` at the centre of each swatch; the image is in physical pixels.
pub fn read_back(image: &ColorImage, ppp: f32, origin: Pos2) -> Vec<(Color32, Option<Color32>)> {
    swatch_rects(origin)
        .iter()
        .zip(SWATCHES)
        .map(|(rect, want)| {
            let c = rect.center() * ppp;
            let (x, y) = (c.x as usize, c.y as usize);
            let got = (x < image.width() && y < image.height()).then(|| image[(x, y)]);
            (want, got)
        })
        .collect()
}

pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatch_centres_are_sampled_in_physical_pixels() {
        let (ppp, origin) = (1.25, egui::pos2(100.0, 50.0));
        let mut image = ColorImage::filled([300, 150], Color32::BLACK);
        for (rect, want) in swatch_rects(origin).iter().zip(SWATCHES) {
            let c = rect.center() * ppp;
            image[(c.x as usize, c.y as usize)] = want;
        }
        let got = read_back(&image, ppp, origin);
        assert_eq!(got, SWATCHES.map(|c| (c, Some(c))).to_vec());

        let tiny = ColorImage::filled([10, 10], Color32::BLACK);
        assert!(read_back(&tiny, ppp, origin).iter().all(|(_, got)| got.is_none()), "off-image is None");
    }
}
