use gpui::{Hsla, Pixels, px};

pub const EDITOR_FONT_FAMILY: &str = "Zed Mono";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowMetrics {
    pub min_height: Pixels,
    pub text_size: Pixels,
    pub line_height: Pixels,
    pub caret_height: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorPalette {
    pub background: Hsla,
    pub text: Hsla,
    pub current_row_background: Hsla,
    pub gutter_current_text: Hsla,
    pub gutter_text: Hsla,
    pub caret: Hsla,
    pub selection_background: Hsla,
    pub selection_text: Hsla,
    pub heading_primary: Hsla,
    pub heading_accent: Hsla,
    pub heading_muted: Hsla,
    pub inline_code_text: Hsla,
    pub inline_code_background: Hsla,
    pub link_text: Hsla,
    pub muted_text: Hsla,
    pub inline_math_text: Hsla,
    pub fenced_code_background: Hsla,
    pub pipe_table_text: Hsla,
    pub pipe_table_background: Hsla,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellPalette {
    pub window_background: Hsla,
    pub title_bar_background: Hsla,
    pub title_bar_border: Hsla,
    pub title_text: Hsla,
    pub secondary_text: Hsla,
    pub dirty_text: Hsla,
    pub error_background: Hsla,
    pub error_text: Hsla,
}

pub fn editor_palette() -> EditorPalette {
    EditorPalette {
        background: gpui::rgb(0x181818).into(),
        text: gpui::rgb(0xd6d6d6).into(),
        current_row_background: gpui::rgba(0xffffff10).into(),
        gutter_current_text: gpui::rgba(0xffffffcc).into(),
        gutter_text: gpui::rgba(0xffffff66).into(),
        caret: gpui::rgb(0xf0f0f0).into(),
        selection_background: gpui::rgb(0x264f78).into(),
        selection_text: gpui::rgb(0xf5fbff).into(),
        heading_primary: gpui::rgb(0xf0f0f0).into(),
        heading_accent: gpui::rgb(0x8fdcff).into(),
        heading_muted: gpui::rgba(0xffffffb3).into(),
        inline_code_text: gpui::rgb(0x8fdcff).into(),
        inline_code_background: gpui::rgba(0xffffff14).into(),
        link_text: gpui::rgb(0x7cc7ff).into(),
        muted_text: gpui::rgba(0xffffffb3).into(),
        inline_math_text: gpui::rgb(0xd2b6ff).into(),
        fenced_code_background: gpui::rgba(0xffffff10).into(),
        pipe_table_text: gpui::rgba(0xffffff99).into(),
        pipe_table_background: gpui::rgba(0xffffff0a).into(),
    }
}

pub fn shell_palette() -> ShellPalette {
    ShellPalette {
        window_background: editor_palette().background,
        title_bar_background: gpui::rgb(0x202020).into(),
        title_bar_border: gpui::rgb(0x303030).into(),
        title_text: gpui::rgb(0xf0f0f0).into(),
        secondary_text: gpui::rgba(0xffffff99).into(),
        dirty_text: gpui::rgb(0xffd38a).into(),
        error_background: gpui::rgb(0x3a241f).into(),
        error_text: gpui::rgb(0xffc7b8).into(),
    }
}

pub fn gutter_width() -> Pixels {
    px(48.)
}

pub fn title_bar_height() -> Pixels {
    px(36.)
}

pub fn default_row_metrics() -> RowMetrics {
    RowMetrics {
        min_height: px(22.),
        text_size: px(14.),
        line_height: px(22.),
        caret_height: px(17.),
    }
}

pub fn heading_row_metrics(level: u8) -> RowMetrics {
    match level {
        1 => RowMetrics {
            min_height: px(42.),
            text_size: px(28.),
            line_height: px(34.),
            caret_height: px(28.),
        },
        2 => RowMetrics {
            min_height: px(34.),
            text_size: px(22.),
            line_height: px(28.),
            caret_height: px(22.),
        },
        3 => RowMetrics {
            min_height: px(28.),
            text_size: px(18.),
            line_height: px(24.),
            caret_height: px(18.),
        },
        _ => RowMetrics {
            min_height: px(24.),
            text_size: px(16.),
            line_height: px(22.),
            caret_height: px(17.),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_metrics_scale_down_by_level() {
        assert!(heading_row_metrics(1).text_size > heading_row_metrics(2).text_size);
        assert!(heading_row_metrics(2).text_size > heading_row_metrics(3).text_size);
        assert!(heading_row_metrics(3).text_size > heading_row_metrics(4).text_size);
    }

    #[test]
    fn shell_and_editor_backgrounds_stay_aligned() {
        assert_eq!(
            shell_palette().window_background,
            editor_palette().background
        );
    }
}
