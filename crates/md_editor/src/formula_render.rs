use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

use gpui::{Hsla, RenderImage, Rgba, px};
use image::Frame;
use ratex_layout::{LayoutOptions, layout, to_display_list};
use ratex_parser::parse;
use ratex_render::{RenderOptions, render_to_png};
use ratex_types::color::Color;
use smallvec::SmallVec;

static FORMULA_RENDER_CACHE: LazyLock<Mutex<HashMap<FormulaRenderKey, FormulaRenderState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FormulaRenderMode {
    Inline,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct FormulaRenderKey {
    pub(super) tex: String,
    pub(super) mode: FormulaRenderMode,
    pub(super) text_size: gpui::Pixels,
    pub(super) line_height: gpui::Pixels,
    pub(super) color: Hsla,
    pub(super) padding: gpui::Pixels,
    pub(super) scale_factor_bits: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FormulaRenderAsset {
    pub(super) image: Arc<RenderImage>,
    pub(super) logical_size: gpui::Size<gpui::Pixels>,
}

#[derive(Clone, Debug)]
pub(super) enum FormulaRenderState {
    Ready(FormulaRenderAsset),
    Invalid(gpui::Size<gpui::Pixels>),
}

pub(super) fn formula_render_key(
    tex: impl Into<String>,
    mode: FormulaRenderMode,
    text_size: gpui::Pixels,
    line_height: gpui::Pixels,
    color: Hsla,
    padding: gpui::Pixels,
    scale_factor: f32,
) -> FormulaRenderKey {
    FormulaRenderKey {
        tex: tex.into(),
        mode,
        text_size,
        line_height,
        color,
        padding,
        scale_factor_bits: scale_factor.to_bits(),
    }
}

pub(super) fn render_formula(
    key: &FormulaRenderKey,
    fallback_size: gpui::Size<gpui::Pixels>,
) -> FormulaRenderState {
    if let Some(state) = formula_render_cache().get(key).cloned() {
        return state;
    }

    let state = match render_formula_image(key) {
        Ok(asset) => FormulaRenderState::Ready(asset),
        Err(_) => FormulaRenderState::Invalid(fallback_size),
    };

    formula_render_cache()
        .entry(key.clone())
        .or_insert(state)
        .clone()
}

fn formula_render_cache() -> MutexGuard<'static, HashMap<FormulaRenderKey, FormulaRenderState>> {
    FORMULA_RENDER_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn render_formula_image(key: &FormulaRenderKey) -> Result<FormulaRenderAsset, String> {
    let ast = parse(&key.tex).map_err(|error| error.to_string())?;
    let layout = layout(&ast, &LayoutOptions::default());
    let display_list = to_display_list(&layout);
    let rgba: Rgba = key.color.into();
    let png = render_to_png(
        &display_list,
        &RenderOptions {
            font_size: f32::from(key.text_size).max(1.),
            padding: f32::from(key.padding).max(0.),
            background_color: Color::new(0., 0., 0., 0.),
            font_dir: String::new(),
            device_pixel_ratio: f32::from_bits(key.scale_factor_bits).clamp(0.5, 4.),
        },
    )?;
    let mut image = image::load_from_memory(&png)
        .map_err(|error| error.to_string())?
        .to_rgba8();

    for pixel in image.pixels_mut() {
        let alpha = f32::from(pixel[3]) / 255.;
        pixel[0] = (rgba.r.clamp(0., 1.) * alpha * 255.).round() as u8;
        pixel[1] = (rgba.g.clamp(0., 1.) * alpha * 255.).round() as u8;
        pixel[2] = (rgba.b.clamp(0., 1.) * alpha * 255.).round() as u8;
    }

    let width = image.width() as f32 / f32::from_bits(key.scale_factor_bits).max(1.);
    let height = image.height() as f32 / f32::from_bits(key.scale_factor_bits).max(1.);
    let image = Arc::new(RenderImage::new(SmallVec::from_elem(Frame::new(image), 1)));

    Ok(FormulaRenderAsset {
        image,
        logical_size: gpui::size(px(width.max(1.)), px(height.max(1.))),
    })
}

#[cfg(test)]
mod tests {
    use md_theme::editor_palette;

    use super::*;

    #[test]
    fn formula_key_changes_for_tex_mode_style_padding_and_scale() {
        let palette = editor_palette();
        let base = formula_render_key(
            "x",
            FormulaRenderMode::Inline,
            px(16.),
            px(20.),
            palette.inline_math_text,
            px(2.),
            1.,
        );

        assert_ne!(
            base,
            formula_render_key(
                "y",
                FormulaRenderMode::Inline,
                px(16.),
                px(20.),
                palette.inline_math_text,
                px(2.),
                1.
            )
        );
        assert_ne!(
            base,
            formula_render_key(
                "x",
                FormulaRenderMode::Block,
                px(16.),
                px(20.),
                palette.inline_math_text,
                px(2.),
                1.
            )
        );
        assert_ne!(
            base,
            formula_render_key(
                "x",
                FormulaRenderMode::Inline,
                px(18.),
                px(20.),
                palette.inline_math_text,
                px(2.),
                1.
            )
        );
        assert_ne!(
            base,
            formula_render_key(
                "x",
                FormulaRenderMode::Inline,
                px(16.),
                px(24.),
                palette.inline_math_text,
                px(2.),
                1.
            )
        );
        assert_ne!(
            base,
            formula_render_key(
                "x",
                FormulaRenderMode::Inline,
                px(16.),
                px(20.),
                palette.text,
                px(2.),
                1.
            )
        );
        assert_ne!(
            base,
            formula_render_key(
                "x",
                FormulaRenderMode::Inline,
                px(16.),
                px(20.),
                palette.inline_math_text,
                px(4.),
                1.
            )
        );
        assert_ne!(
            base,
            formula_render_key(
                "x",
                FormulaRenderMode::Inline,
                px(16.),
                px(20.),
                palette.inline_math_text,
                px(2.),
                2.
            )
        );
    }

    #[test]
    fn formula_render_produces_non_empty_image() {
        let palette = editor_palette();
        let key = formula_render_key(
            "x + y",
            FormulaRenderMode::Inline,
            px(16.),
            px(20.),
            palette.inline_math_text,
            px(2.),
            1.,
        );

        let FormulaRenderState::Ready(asset) = render_formula(&key, gpui::size(px(10.), px(10.)))
        else {
            panic!("formula should render");
        };

        assert!(asset.logical_size.width > px(1.));
        assert!(asset.logical_size.height > px(1.));
        assert!(!asset.image.as_bytes(0).unwrap().is_empty());
    }

    #[test]
    fn repeated_formula_style_pairs_reuse_cached_asset() {
        let palette = editor_palette();
        let key = formula_render_key(
            "x + y",
            FormulaRenderMode::Inline,
            px(16.),
            px(20.),
            palette.inline_math_text,
            px(2.),
            1.,
        );

        let FormulaRenderState::Ready(first) = render_formula(&key, gpui::size(px(10.), px(10.)))
        else {
            panic!("formula should render");
        };
        let FormulaRenderState::Ready(second) = render_formula(&key, gpui::size(px(20.), px(20.)))
        else {
            panic!("formula should render");
        };

        assert!(Arc::ptr_eq(&first.image, &second.image));
    }
}
