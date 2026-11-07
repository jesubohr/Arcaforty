use arcaforty_core::Appearance;
use gpui::{Global, Hsla, WindowAppearance, rgb};

pub const HEADING: &str = "Georgia";
pub const BODY: &str = "Segoe UI";
pub const MONO: &str = "Consolas";

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Hsla,
    pub paper: Hsla,
    pub ink: Hsla,
    /// Secondary text.
    pub subtle: Hsla,
    /// Placeholders and disabled text.
    pub muted: Hsla,
    pub line: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub danger: Hsla,
    pub hover: Hsla,
    pub selection: Hsla,
}

impl Global for Palette {}

impl Palette {
    pub fn light() -> Palette {
        Palette {
            bg: rgb(0xF7F5EF).into(),
            paper: rgb(0xFFFEFA).into(),
            ink: rgb(0x191912).into(),
            subtle: rgb(0x625E52).into(),
            muted: rgb(0x8E8A7D).into(),
            line: rgb(0xDED8CB).into(),
            accent: rgb(0x4E805F).into(),
            on_accent: rgb(0xFFFEFA).into(),
            danger: rgb(0xA3402C).into(),
            hover: rgb(0xEFEBE0).into(),
            selection: gpui::rgba(0x4E805F40).into(),
        }
    }

    pub fn dark() -> Palette {
        Palette {
            bg: rgb(0x1A1A15).into(),
            paper: rgb(0x22221C).into(),
            ink: rgb(0xEFECE2).into(),
            subtle: rgb(0xB0AC9E).into(),
            muted: rgb(0x8A8679).into(),
            line: rgb(0x38372E).into(),
            accent: rgb(0x6FA583).into(),
            on_accent: rgb(0x10150F).into(),
            danger: rgb(0xE0806B).into(),
            hover: rgb(0x2B2A22).into(),
            selection: gpui::rgba(0x6FA58350).into(),
        }
    }

    pub fn resolve(appearance: Appearance, system: WindowAppearance) -> Palette {
        match appearance {
            Appearance::Light => Palette::light(),
            Appearance::Dark => Palette::dark(),
            Appearance::System => match system {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Palette::dark(),
                _ => Palette::light(),
            },
        }
    }
}
