use gpui::{Global, RenderImage};
use std::sync::Arc;

pub struct Wordmarks {
    pub light: Arc<RenderImage>,
    pub dark: Arc<RenderImage>,
}

impl Global for Wordmarks {}

impl Wordmarks {
    pub fn load() -> Self {
        let source = include_str!("../assets/logo-wordmark.svg");
        Self {
            light: render(source),
            dark: render(&source.replace("rgb(0, 0, 0)", "#EFECE2")),
        }
    }
}

fn render(source: &str) -> Arc<RenderImage> {
    let tree = resvg::usvg::Tree::from_str(source, &resvg::usvg::Options::default())
        .expect("embedded wordmark must be valid SVG");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(720, 720).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            720. / tree.size().width(),
            720. / tree.size().height(),
        ),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    // GPUI expects straight-alpha BGRA pixels.
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha != 0 {
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 / alpha).min(255) as u8;
            }
        }
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(720, 720, pixels).unwrap();
    Arc::new(RenderImage::new([image::Frame::new(image)].as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_wordmarks_have_visible_pixels() {
        let marks = Wordmarks::load();
        for mark in [&marks.light, &marks.dark] {
            assert!(mark.as_bytes(0).unwrap().chunks_exact(4).any(|p| p[3] != 0));
        }
    }
}
