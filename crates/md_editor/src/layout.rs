use std::{ops::Range, path::Path, sync::Arc};

use gpui::{App, LineFragment, SharedString, TextRun, Window, font, px};
use md_assets::EDITOR_FONT_FAMILY;
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection};

pub(super) use super::inline_layout::{
    atom_range_containing_display_index, atomic_wrap_boundary_index, display_inline_fragments,
    inline_style, line_fragments_for_wrapping, row_display_style_for_display_row, segment_text,
    source_display_fragments, text_runs_for_display_segments, text_runs_for_segment_lengths,
    text_runs_for_segments, text_runs_on_char_boundaries, text_segments_for_fragments,
};
use super::inline_layout::{has_inline_atoms, wrap_boundary_glyph};
use super::{
    DisplayInlineAtom, DisplayInlineFragment, DisplayTableRowLayout, InlineAtomMeasurementState,
    MarkdownEditorMode, active_source_range_for_selection,
    block::DisplayBlockLayout,
    display_model::{DisplayRow, DisplayTextStyle},
    inactive_rendered_element_source_ranges_for_selection, left_rail_width, ranges_overlap,
    visual_row::display_x_for_offset,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RowDisplayStyle {
    pub(crate) min_height: gpui::Pixels,
    pub(crate) text_size: gpui::Pixels,
    pub(crate) line_height: gpui::Pixels,
    pub(crate) caret_height: gpui::Pixels,
}

impl From<md_theme::RowMetrics> for RowDisplayStyle {
    fn from(metrics: md_theme::RowMetrics) -> Self {
        Self {
            min_height: metrics.min_height,
            text_size: metrics.text_size,
            line_height: metrics.line_height,
            caret_height: metrics.caret_height,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VisualDisplayRow {
    pub(super) display_range: Range<usize>,
    pub(super) line_start_x: gpui::Pixels,
    pub(super) top: gpui::Pixels,
    pub(super) height: gpui::Pixels,
}

#[derive(Clone, Debug)]
pub(super) struct DisplayRowTextLayout {
    pub(super) fragments: Vec<DisplayInlineFragment>,
    pub(super) visual_rows: Vec<VisualDisplayRow>,
    pub(super) shaped_line: gpui::ShapedLine,
    pub(super) text_len: usize,
    pub(super) cacheable: bool,
}

#[derive(Clone, Debug)]
pub(super) struct DisplayRowLayoutInputs {
    pub(super) fragments: Vec<DisplayInlineFragment>,
    pub(super) text_runs: Vec<TextRun>,
    pub(super) shaped_line: gpui::ShapedLine,
    pub(super) text_len: usize,
    pub(super) has_inline_atoms: bool,
}

impl DisplayRowLayoutInputs {
    pub(super) fn inline_atoms(&self) -> impl Iterator<Item = &DisplayInlineAtom> {
        self.fragments.iter().filter_map(|fragment| match fragment {
            DisplayInlineFragment::Text(_) => None,
            DisplayInlineFragment::Atom(atom) => Some(atom),
        })
    }
}

impl DisplayRowTextLayout {
    pub(super) fn height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        let height = self
            .visual_rows
            .iter()
            .fold(px(0.), |height, visual_row| height + visual_row.height);

        height.max(row_style.line_height)
    }
}

#[derive(Clone, Debug)]
pub(super) enum DisplayItemLayout {
    Text(Arc<DisplayRowTextLayout>),
    Block(Arc<DisplayBlockLayout>),
    TableRow(Arc<DisplayTableRowLayout>),
}

pub(super) type DisplayRowLayout = DisplayItemLayout;

impl DisplayItemLayout {
    pub(super) fn cacheable(&self) -> bool {
        match self {
            Self::Text(text_layout) => text_layout.cacheable,
            Self::Block(block_layout) => block_layout.cacheable(),
            Self::TableRow(table_layout) => table_layout.cacheable(),
        }
    }

    pub(super) fn row_min_height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::Text(text_layout) => row_style.min_height.max(text_layout.height(row_style)),
            Self::Block(block_layout) if block_layout.height() == px(0.) => px(0.),
            Self::Block(block_layout) => row_style.min_height.max(block_layout.height()),
            Self::TableRow(table_layout) => row_style.min_height.max(table_layout.height()),
        }
    }

    pub(super) fn content_min_height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::Text(text_layout) => text_layout.height(row_style),
            Self::Block(block_layout) if block_layout.height() == px(0.) => px(0.),
            Self::Block(block_layout) => row_style.min_height.max(block_layout.height()),
            Self::TableRow(table_layout) => row_style.min_height.max(table_layout.height()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct DisplayRowCacheKey {
    pub(super) version: md_text::Global,
    pub(super) item_id: crate::rendered_index::DisplayItemId,
    pub(super) item_index: u32,
    pub(super) source_range: Range<usize>,
    pub(super) source_row_range: Range<usize>,
    pub(super) mode: MarkdownEditorMode,
    pub(super) active_projection_source_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
pub(super) struct DisplayRowProjectionState {
    pub(super) active_source_range: Option<Range<usize>>,
    pub(super) inactive_source_ranges: Vec<Range<usize>>,
    pub(super) active_cursor: Option<Point>,
}

impl DisplayRowProjectionState {
    pub(super) fn new(
        snapshot: &BufferSnapshot,
        selection: Option<&Selection<Point>>,
        mode: MarkdownEditorMode,
    ) -> Self {
        if mode != MarkdownEditorMode::Rendered {
            return Self {
                active_source_range: None,
                inactive_source_ranges: Vec::new(),
                active_cursor: None,
            };
        }

        Self {
            active_source_range: selection
                .and_then(|selection| active_source_range_for_selection(snapshot, selection)),
            inactive_source_ranges: selection
                .map(|selection| {
                    inactive_rendered_element_source_ranges_for_selection(snapshot, selection)
                })
                .unwrap_or_default(),
            active_cursor: selection
                .filter(|selection| selection.is_empty())
                .map(|selection| selection.head()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RowLayoutCacheKey {
    pub(super) item_id: crate::rendered_index::DisplayItemId,
    pub(super) item_index: u32,
    pub(super) source_range: Range<usize>,
    pub(super) source_row_range: Range<usize>,
    pub(super) mode: MarkdownEditorMode,
    pub(super) row_style: RowDisplayStyle,
    pub(super) wrap_width: gpui::Pixels,
    pub(super) active_projection_source_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RowLayoutInputCacheKey {
    pub(super) version: md_text::Global,
    pub(super) item_id: crate::rendered_index::DisplayItemId,
    pub(super) item_index: u32,
    pub(super) source_range: Range<usize>,
    pub(super) source_row_range: Range<usize>,
    pub(super) mode: MarkdownEditorMode,
    pub(super) active_projection_source_ranges: Vec<Range<usize>>,
    pub(super) row_style: RowDisplayStyle,
}

pub(super) fn text_wrap_width(window: &Window) -> gpui::Pixels {
    text_wrap_width_for_mode(window, MarkdownEditorMode::Source)
}

pub(super) fn text_wrap_width_for_mode(window: &Window, mode: MarkdownEditorMode) -> gpui::Pixels {
    (window.bounds().size.width - left_rail_width(mode)).max(px(1.))
}

pub(super) fn effective_text_wrap_width(
    display_row: &DisplayRow,
    wrap_width: gpui::Pixels,
) -> gpui::Pixels {
    (wrap_width - display_row.rendered_indent_width()).max(px(1.))
}

pub(super) fn display_row_layout_inputs(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    document_path: Option<&Path>,
    window: &mut Window,
) -> DisplayRowLayoutInputs {
    let fragments =
        display_fragments_for_text_layout(snapshot, display_row, mode, row_style, document_path);
    display_row_layout_inputs_for_fragments(display_row, fragments, row_style, window)
}

pub(super) fn source_display_row_layout_inputs(
    display_row: &DisplayRow,
    row_style: RowDisplayStyle,
    window: &mut Window,
) -> DisplayRowLayoutInputs {
    display_row_layout_inputs_for_fragments(
        display_row,
        source_display_fragments(display_row),
        row_style,
        window,
    )
}

pub(super) fn display_row_layout_inputs_for_fragments(
    display_row: &DisplayRow,
    fragments: Vec<DisplayInlineFragment>,
    row_style: RowDisplayStyle,
    window: &mut Window,
) -> DisplayRowLayoutInputs {
    let segments = text_segments_for_fragments(&display_row.text, &fragments);
    let text_runs = text_runs_on_char_boundaries(
        &display_row.text,
        &text_runs_for_display_segments(&display_row.text, &segments),
    );
    let shaped_text = shapeable_display_text(&display_row.text);
    let shaped_line = window.text_system().shape_line(
        SharedString::from(shaped_text),
        row_style.text_size,
        &text_runs,
        None,
    );
    let has_inline_atoms = has_inline_atoms(&fragments);

    DisplayRowLayoutInputs {
        fragments,
        text_runs,
        shaped_line,
        text_len: display_row.text.len(),
        has_inline_atoms,
    }
}

fn shapeable_display_text(display_text: &str) -> String {
    display_text.replace(['\r', '\n'], " ")
}

pub(super) fn text_layout_for_display_row_inputs(
    display_text: &str,
    inputs: &DisplayRowLayoutInputs,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    inline_atom_measurements: &[InlineAtomMeasurementState],
    window: &mut Window,
    cx: &mut App,
) -> DisplayRowTextLayout {
    let mut fragments = inputs.fragments.clone();
    let cacheable =
        apply_inline_atom_measurements(&mut fragments, inputs, row_style, inline_atom_measurements);
    let visual_rows = if display_text.contains('\n') {
        forced_break_visual_rows_with_wrap(
            display_text,
            &fragments,
            &inputs.text_runs,
            &inputs.shaped_line,
            row_style,
            wrap_width,
            inputs.has_inline_atoms,
            window,
            cx,
        )
    } else if inputs.has_inline_atoms {
        visual_rows_for_fragments(
            display_text,
            &fragments,
            &inputs.shaped_line,
            row_style,
            wrap_width,
            cx,
        )
    } else if let Some(visual_rows) = unwrapped_visual_rows_if_fits(
        inputs.text_len,
        &fragments,
        row_style,
        inputs.shaped_line.width,
        wrap_width,
    ) {
        visual_rows
    } else {
        match window.text_system().shape_text(
            SharedString::from(display_text.to_string()),
            row_style.text_size,
            &inputs.text_runs,
            Some(wrap_width),
            None,
        ) {
            Ok(wrapped_lines) => wrapped_lines
                .first()
                .map(|wrapped_line| {
                    visual_rows_for_wrapped_line(wrapped_line, &fragments, row_style)
                })
                .unwrap_or_else(|| fallback_visual_rows(inputs.text_len, &fragments, row_style)),
            Err(_) => fallback_visual_rows(inputs.text_len, &fragments, row_style),
        }
    };

    DisplayRowTextLayout {
        fragments,
        visual_rows,
        shaped_line: inputs.shaped_line.clone(),
        text_len: inputs.text_len,
        cacheable,
    }
}

pub(super) fn unwrapped_visual_rows_if_fits(
    text_len: usize,
    fragments: &[DisplayInlineFragment],
    row_style: RowDisplayStyle,
    shaped_line_width: gpui::Pixels,
    wrap_width: gpui::Pixels,
) -> Option<Vec<VisualDisplayRow>> {
    (shaped_line_width <= wrap_width).then(|| fallback_visual_rows(text_len, fragments, row_style))
}

#[cfg(test)]
pub(super) fn forced_break_visual_rows(
    display_text: &str,
    fragments: &[DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
) -> Vec<VisualDisplayRow> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut top = px(0.);

    for (break_index, _) in display_text.match_indices('\n') {
        let display_range = start..break_index + 1;
        let height = visual_row_height_for_range(fragments, &display_range, row_style);
        rows.push(VisualDisplayRow {
            line_start_x: display_x_for_offset(fragments, shaped_line, start),
            display_range,
            top,
            height,
        });
        top += height;
        start = break_index + 1;
    }

    let display_range = start..display_text.len();
    rows.push(VisualDisplayRow {
        line_start_x: display_x_for_offset(fragments, shaped_line, start),
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        top,
    });

    rows
}

pub(super) fn forced_break_visual_rows_with_wrap(
    display_text: &str,
    fragments: &[DisplayInlineFragment],
    text_runs: &[TextRun],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    has_inline_atoms: bool,
    window: &mut Window,
    cx: &mut App,
) -> Vec<VisualDisplayRow> {
    let mut rows = Vec::new();
    let mut top = px(0.);
    let mut start = 0;

    for (break_index, _) in display_text.match_indices('\n') {
        push_wrapped_forced_break_segment(
            &mut rows,
            &mut top,
            display_text,
            fragments,
            text_runs,
            shaped_line,
            row_style,
            wrap_width,
            has_inline_atoms,
            window,
            cx,
            start..break_index,
            Some(break_index),
        );
        start = break_index + 1;
    }

    push_wrapped_forced_break_segment(
        &mut rows,
        &mut top,
        display_text,
        fragments,
        text_runs,
        shaped_line,
        row_style,
        wrap_width,
        has_inline_atoms,
        window,
        cx,
        start..display_text.len(),
        None,
    );

    rows
}

fn push_wrapped_forced_break_segment(
    rows: &mut Vec<VisualDisplayRow>,
    top: &mut gpui::Pixels,
    display_text: &str,
    fragments: &[DisplayInlineFragment],
    text_runs: &[TextRun],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    has_inline_atoms: bool,
    window: &mut Window,
    cx: &mut App,
    display_range: Range<usize>,
    trailing_break: Option<usize>,
) {
    let trailing_end = trailing_break.map_or(display_range.end, |break_index| break_index + 1);
    let mut segment_rows = if display_range.is_empty() {
        vec![VisualDisplayRow {
            line_start_x: display_x_for_offset(fragments, shaped_line, display_range.start),
            height: visual_row_height_for_range(
                fragments,
                &(display_range.start..trailing_end),
                row_style,
            ),
            display_range: display_range.start..trailing_end,
            top: px(0.),
        }]
    } else {
        let shaped_rows = (!has_inline_atoms)
            .then(|| {
                visual_rows_for_shaped_display_range(
                    display_text,
                    text_runs,
                    shaped_line,
                    row_style,
                    wrap_width,
                    window,
                    display_range.clone(),
                )
            })
            .flatten();
        shaped_rows.unwrap_or_else(|| {
            visual_rows_for_display_range(
                display_text,
                fragments,
                shaped_line,
                row_style,
                wrap_width,
                cx,
                display_range.clone(),
            )
        })
    };

    if let Some(last) = segment_rows.last_mut() {
        last.display_range.end = trailing_end;
        last.height = visual_row_height_for_range(fragments, &last.display_range, row_style);
    }

    for mut row in segment_rows {
        row.top = *top;
        *top += row.height;
        rows.push(row);
    }
}

fn visual_rows_for_shaped_display_range(
    display_text: &str,
    text_runs: &[TextRun],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    window: &mut Window,
    display_range: Range<usize>,
) -> Option<Vec<VisualDisplayRow>> {
    let segment_text = display_text.get(display_range.clone())?;
    let text_runs = text_runs_for_display_range(text_runs, &display_range);
    let wrapped_lines = window
        .text_system()
        .shape_text(
            SharedString::from(segment_text.to_string()),
            row_style.text_size,
            &text_runs,
            Some(wrap_width),
            None,
        )
        .ok()?;
    let wrapped_line = wrapped_lines.first()?;

    Some(visual_rows_for_wrapped_line_in_display_range(
        wrapped_line,
        shaped_line,
        row_style,
        display_range.start,
    ))
}

fn text_runs_for_display_range(
    text_runs: &[TextRun],
    display_range: &Range<usize>,
) -> Vec<TextRun> {
    let mut range_runs = Vec::new();
    let mut run_start = 0;
    for run in text_runs {
        let run_end = run_start + run.len;
        let start = run_start.max(display_range.start);
        let end = run_end.min(display_range.end);
        if start < end {
            let mut run = run.clone();
            run.len = end - start;
            range_runs.push(run);
        }
        run_start = run_end;
    }

    if range_runs.is_empty() {
        text_runs_for_segment_lengths([(0, DisplayTextStyle::default())])
    } else {
        range_runs
    }
}

fn visual_rows_for_wrapped_line_in_display_range(
    wrapped_line: &gpui::WrappedLine,
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    display_range_start: usize,
) -> Vec<VisualDisplayRow> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut top = px(0.);

    for wrap_boundary in wrapped_line.wrap_boundaries() {
        let Some(glyph) = wrap_boundary_glyph(wrapped_line, *wrap_boundary) else {
            return vec![VisualDisplayRow {
                line_start_x: display_x_for_offset(&[], shaped_line, display_range_start),
                height: row_style.line_height,
                display_range: display_range_start..display_range_start + wrapped_line.len(),
                top: px(0.),
            }];
        };
        let boundary_index = wrapped_line.text.floor_char_boundary(glyph.index);
        if boundary_index < start || boundary_index > wrapped_line.len() {
            return vec![VisualDisplayRow {
                line_start_x: display_x_for_offset(&[], shaped_line, display_range_start),
                height: row_style.line_height,
                display_range: display_range_start..display_range_start + wrapped_line.len(),
                top: px(0.),
            }];
        }
        if boundary_index == start {
            continue;
        }

        rows.push(VisualDisplayRow {
            line_start_x: display_x_for_offset(&[], shaped_line, display_range_start + start),
            height: row_style.line_height,
            display_range: display_range_start + start..display_range_start + boundary_index,
            top,
        });
        start = boundary_index;
        top += row_style.line_height;
    }

    rows.push(VisualDisplayRow {
        line_start_x: display_x_for_offset(&[], shaped_line, display_range_start + start),
        height: row_style.line_height,
        display_range: display_range_start + start..display_range_start + wrapped_line.len(),
        top,
    });
    rows
}

fn visual_rows_for_display_range(
    display_text: &str,
    fragments: &[DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    cx: &mut App,
    display_range: Range<usize>,
) -> Vec<VisualDisplayRow> {
    let Some(line_fragments) =
        line_fragments_for_wrapping_range(display_text, fragments, &display_range)
    else {
        return fallback_visual_rows_for_display_range(
            fragments,
            shaped_line,
            row_style,
            display_range,
        );
    };

    let mut rows = Vec::new();
    let mut start = display_range.start;
    let mut top = px(0.);
    let mut line_wrapper = cx
        .text_system()
        .line_wrapper(font(EDITOR_FONT_FAMILY), row_style.text_size);

    for boundary in line_wrapper.wrap_line(&line_fragments, wrap_width) {
        let Some(boundary_index) =
            display_offset_for_wrapping_index(fragments, &display_range, boundary.ix)
        else {
            return fallback_visual_rows_for_display_range(
                fragments,
                shaped_line,
                row_style,
                display_range,
            );
        };
        let boundary_index = display_text.floor_char_boundary(atomic_wrap_boundary_index(
            fragments,
            boundary_index,
            start,
        ));
        if boundary_index < start || boundary_index > display_range.end {
            return fallback_visual_rows_for_display_range(
                fragments,
                shaped_line,
                row_style,
                display_range,
            );
        }
        if boundary_index == start {
            continue;
        }

        let row_range = start..boundary_index;
        let height = visual_row_height_for_range(fragments, &row_range, row_style);
        rows.push(VisualDisplayRow {
            line_start_x: display_x_for_offset(fragments, shaped_line, start),
            display_range: row_range,
            top,
            height,
        });
        start = boundary_index;
        top += height;
    }

    let row_range = start..display_range.end;
    rows.push(VisualDisplayRow {
        line_start_x: display_x_for_offset(fragments, shaped_line, start),
        height: visual_row_height_for_range(fragments, &row_range, row_style),
        display_range: row_range,
        top,
    });
    rows
}

fn fallback_visual_rows_for_display_range(
    fragments: &[DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    display_range: Range<usize>,
) -> Vec<VisualDisplayRow> {
    vec![VisualDisplayRow {
        line_start_x: display_x_for_offset(fragments, shaped_line, display_range.start),
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        top: px(0.),
    }]
}

fn line_fragments_for_wrapping_range<'a>(
    display_text: &'a str,
    fragments: &'a [DisplayInlineFragment],
    display_range: &Range<usize>,
) -> Option<Vec<LineFragment<'a>>> {
    let mut line_fragments = Vec::new();
    for fragment in fragments {
        match fragment {
            DisplayInlineFragment::Text(segment) => {
                let start = segment.display_range.start.max(display_range.start);
                let end = segment.display_range.end.min(display_range.end);
                if start < end {
                    line_fragments.push(LineFragment::text(display_text.get(start..end)?));
                }
            }
            DisplayInlineFragment::Atom(atom)
                if ranges_overlap(&atom.display_range, display_range) =>
            {
                let text = display_text.get(atom.display_range.clone())?;
                if !text.is_empty() {
                    line_fragments.push(LineFragment::element(atom.width.max(px(1.)), text.len()));
                }
            }
            DisplayInlineFragment::Atom(_) => {}
        }
    }
    Some(line_fragments)
}

fn display_offset_for_wrapping_index(
    fragments: &[DisplayInlineFragment],
    display_range: &Range<usize>,
    wrapping_index: usize,
) -> Option<usize> {
    let mut consumed = 0;
    for fragment in fragments {
        let fragment_range = match fragment {
            DisplayInlineFragment::Text(segment) => &segment.display_range,
            DisplayInlineFragment::Atom(atom) => &atom.display_range,
        };
        let start = fragment_range.start.max(display_range.start);
        let end = fragment_range.end.min(display_range.end);
        if start >= end {
            continue;
        }

        let len = end - start;
        if wrapping_index <= consumed + len {
            return Some(start + wrapping_index.saturating_sub(consumed));
        }
        consumed += len;
    }

    (wrapping_index == consumed).then_some(display_range.end)
}

pub(super) fn display_fragments_for_text_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    document_path: Option<&Path>,
) -> Vec<DisplayInlineFragment> {
    if mode == MarkdownEditorMode::Source {
        return source_display_fragments(display_row);
    }

    display_inline_fragments(snapshot, display_row, mode, row_style, document_path)
}

pub(super) fn apply_inline_atom_measurements(
    fragments: &mut [DisplayInlineFragment],
    inputs: &DisplayRowLayoutInputs,
    row_style: RowDisplayStyle,
    inline_atom_measurements: &[InlineAtomMeasurementState],
) -> bool {
    if !inputs.has_inline_atoms {
        return true;
    }

    let mut cacheable = true;
    let mut measurements = inline_atom_measurements.iter().copied();
    for fragment in fragments {
        let DisplayInlineFragment::Atom(atom) = fragment else {
            continue;
        };
        let fallback_size = atom.fallback_size(&inputs.shaped_line, row_style);
        let measurement = measurements
            .next()
            .unwrap_or(InlineAtomMeasurementState::Pending(fallback_size));
        let size = measurement.size();
        atom.width = size.width;
        atom.height = size.height;
        cacheable &= measurement.cacheable();
    }
    cacheable
}

pub(super) fn visual_rows_for_fragments(
    display_text: &str,
    fragments: &[DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    cx: &mut App,
) -> Vec<VisualDisplayRow> {
    let Some(line_fragments) = line_fragments_for_wrapping(display_text, fragments) else {
        return fallback_visual_rows(display_text.len(), fragments, row_style);
    };

    let mut rows = Vec::new();
    let mut start = 0;
    let mut top = px(0.);
    let mut line_wrapper = cx
        .text_system()
        .line_wrapper(font(EDITOR_FONT_FAMILY), row_style.text_size);

    for boundary in line_wrapper.wrap_line(&line_fragments, wrap_width) {
        let boundary_index = display_text.floor_char_boundary(atomic_wrap_boundary_index(
            fragments,
            boundary.ix,
            start,
        ));
        if boundary_index < start || boundary_index > display_text.len() {
            return fallback_visual_rows(display_text.len(), fragments, row_style);
        }
        if boundary_index == start {
            continue;
        }

        let display_range = start..boundary_index;
        let height = visual_row_height_for_range(fragments, &display_range, row_style);
        rows.push(VisualDisplayRow {
            line_start_x: display_x_for_offset(fragments, shaped_line, start),
            display_range,
            top,
            height,
        });
        start = boundary_index;
        top += height;
    }

    let display_range = start..display_text.len();
    rows.push(VisualDisplayRow {
        line_start_x: display_x_for_offset(fragments, shaped_line, start),
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        top,
    });
    rows
}

pub(super) fn visual_rows_for_wrapped_line(
    wrapped_line: &gpui::WrappedLine,
    fragments: &[DisplayInlineFragment],
    row_style: RowDisplayStyle,
) -> Vec<VisualDisplayRow> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut start_x = px(0.);
    let mut top = px(0.);

    for wrap_boundary in wrapped_line.wrap_boundaries() {
        let Some(glyph) = wrap_boundary_glyph(wrapped_line, *wrap_boundary) else {
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        };
        let glyph_index = wrapped_line.text.floor_char_boundary(glyph.index);
        if glyph_index < start {
            if atom_range_containing_display_index(fragments, glyph_index)
                .is_some_and(|atom_range| atom_range.end <= start)
            {
                continue;
            }
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        }
        let boundary_index = wrapped_line
            .text
            .floor_char_boundary(atomic_wrap_boundary_index(fragments, glyph_index, start));
        if boundary_index < start {
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        }
        if boundary_index == start {
            continue;
        }
        let display_range = start..boundary_index;
        let height = visual_row_height_for_range(fragments, &display_range, row_style);
        rows.push(VisualDisplayRow {
            display_range,
            line_start_x: start_x,
            top,
            height,
        });
        start = boundary_index;
        start_x = wrapped_line.unwrapped_layout.x_for_index(boundary_index);
        top += height;
    }

    let display_range = start..wrapped_line.len();
    rows.push(VisualDisplayRow {
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        line_start_x: start_x,
        top,
    });
    rows
}

pub(super) fn fallback_visual_rows(
    text_len: usize,
    fragments: &[DisplayInlineFragment],
    row_style: RowDisplayStyle,
) -> Vec<VisualDisplayRow> {
    let display_range = 0..text_len;
    vec![VisualDisplayRow {
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        line_start_x: px(0.),
        top: px(0.),
    }]
}

pub(super) fn visual_row_height_for_range(
    fragments: &[DisplayInlineFragment],
    display_range: &Range<usize>,
    row_style: RowDisplayStyle,
) -> gpui::Pixels {
    fragments
        .iter()
        .filter_map(|fragment| match fragment {
            DisplayInlineFragment::Text(_) => None,
            DisplayInlineFragment::Atom(atom)
                if ranges_overlap(&atom.display_range, display_range) =>
            {
                Some(atom.height)
            }
            DisplayInlineFragment::Atom(_) => None,
        })
        .fold(row_style.line_height, |height, atom_height| {
            height.max(atom_height)
        })
}
