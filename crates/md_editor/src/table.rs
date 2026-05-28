use std::ops::Range;

use gpui::{
    Context, IntoElement, MouseButton, SharedString, TextAlign, Window, div, prelude::*, px,
};
use markdown_wysiwyg::{
    MarkdownTable, MarkdownTableAlignment, MarkdownTableCell, MarkdownTableRow,
};
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection, SelectionGoal};
use md_theme::{editor_palette, gutter_width};

use super::{
    MarkdownEditor, MarkdownEditorMode, RowDisplayStyle, VisualLineBoundary, clip_cursor,
    display_model::{DisplayRow, DisplayTextStyle, StyledDisplaySegment},
    layout::{inline_style, text_runs_for_segments},
    range_contains, render_text_piece, rendered_element_source_range_is_active,
    visual_horizontal_goal,
};

const TABLE_CELL_HORIZONTAL_PADDING: gpui::Pixels = px(8.);
const TABLE_CELL_VERTICAL_PADDING: gpui::Pixels = px(3.);
const TABLE_BORDER_WIDTH: gpui::Pixels = px(1.);
const TABLE_MIN_CELL_WIDTH: gpui::Pixels = px(32.);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct TableLayoutCacheKey {
    pub(super) version: md_text::Global,
    pub(super) table_source_range: Range<usize>,
    pub(super) wrap_width: gpui::Pixels,
    pub(super) row_style: RowDisplayStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DisplayTableLayout {
    pub(super) table_source_range: Range<usize>,
    pub(super) column_widths: Vec<gpui::Pixels>,
    pub(super) width: gpui::Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DisplayTableRowLayout {
    pub(super) table_source_range: Range<usize>,
    pub(super) row_source_range: Range<usize>,
    pub(super) row: usize,
    pub(super) cells: Vec<DisplayTableCellLayout>,
    pub(super) is_header: bool,
    pub(super) is_delimiter: bool,
    pub(super) height: gpui::Pixels,
    pub(super) width: gpui::Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DisplayTableCellLayout {
    pub(super) source_range: Range<usize>,
    pub(super) content_range: Range<usize>,
    pub(super) text: String,
    pub(super) segments: Vec<StyledDisplaySegment>,
    pub(super) visual_lines: Vec<Range<usize>>,
    pub(super) x: gpui::Pixels,
    pub(super) width: gpui::Pixels,
    pub(super) alignment: MarkdownTableAlignment,
}

impl DisplayTableRowLayout {
    pub(super) fn is_inactive_table_row(
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
    ) -> bool {
        mode == MarkdownEditorMode::Rendered
            && snapshot
                .syntax_tree()
                .table_for_source_row(display_row.row as usize)
                .is_some()
            && !rendered_element_source_range_is_active(
                snapshot,
                selection,
                &display_row.source_range,
            )
    }

    pub(super) fn new(
        snapshot: &BufferSnapshot,
        table: &MarkdownTable,
        table_row: &MarkdownTableRow,
        table_layout: &DisplayTableLayout,
        row_style: RowDisplayStyle,
        window: &mut Window,
    ) -> Self {
        let is_header = table.header.row == table_row.row;
        let is_delimiter = table.delimiter.row == table_row.row;
        let mut x = px(0.);
        let cells = table_row
            .cells
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let width = table_layout
                    .column_widths
                    .get(column)
                    .copied()
                    .unwrap_or(TABLE_MIN_CELL_WIDTH);
                let (text, segments) = table_cell_display(snapshot, cell);
                let visual_lines =
                    table_cell_visual_lines(&text, &segments, width, row_style, window);
                let layout = DisplayTableCellLayout {
                    source_range: cell.source_range.clone(),
                    content_range: cell.content_range.clone(),
                    text,
                    segments,
                    visual_lines,
                    x,
                    width,
                    alignment: table.alignments.get(column).copied().unwrap_or_default(),
                };
                x += width;
                layout
            })
            .collect::<Vec<_>>();
        let height = if is_delimiter {
            (row_style.line_height * 0.45).max(px(6.))
        } else {
            let line_count = cells
                .iter()
                .map(|cell| cell.visual_lines.len())
                .max()
                .unwrap_or(1);
            row_style.line_height * line_count as f32
                + TABLE_CELL_VERTICAL_PADDING * 2.
                + TABLE_BORDER_WIDTH * 2.
        };

        Self {
            table_source_range: table.source_range.clone(),
            row_source_range: table_row.source_range.clone(),
            row: table_row.row,
            cells,
            is_header,
            is_delimiter,
            height,
            width: table_layout.width,
        }
    }

    pub(super) fn cacheable(&self) -> bool {
        true
    }

    pub(super) fn height(&self) -> gpui::Pixels {
        self.height
    }

    pub(super) fn line_boundary_target(
        &self,
        snapshot: &BufferSnapshot,
        boundary: VisualLineBoundary,
    ) -> (Point, SelectionGoal) {
        let source_offset = match boundary {
            VisualLineBoundary::Start => self.row_source_range.start,
            VisualLineBoundary::End => self.row_source_range.end,
        };
        let x = self.visible_x_for_source_offset(source_offset);
        (
            snapshot.as_text_snapshot().offset_to_point(source_offset),
            visual_horizontal_goal(0, x),
        )
    }

    pub(super) fn visible_x_for_source_offset(&self, source_offset: usize) -> gpui::Pixels {
        let Some(cell) = self.cell_containing_source_offset(source_offset) else {
            if source_offset <= self.row_source_range.start {
                return px(0.);
            }
            return self.width;
        };
        if cell.content_range.is_empty() {
            return cell.x + TABLE_CELL_HORIZONTAL_PADDING;
        }
        let ratio = ((source_offset.saturating_sub(cell.content_range.start)) as f32
            / cell.content_range.len() as f32)
            .clamp(0., 1.);
        cell.x
            + TABLE_CELL_HORIZONTAL_PADDING
            + (cell.width - TABLE_CELL_HORIZONTAL_PADDING * 2.) * ratio
    }

    pub(super) fn point_for_x(&self, snapshot: &BufferSnapshot, x: gpui::Pixels) -> Point {
        let offset = self.source_offset_for_x(x);
        let offset = snapshot
            .as_text_snapshot()
            .as_rope()
            .floor_char_boundary(offset);
        clip_cursor(
            snapshot,
            snapshot.as_text_snapshot().offset_to_point(offset),
        )
    }

    pub(super) fn mouse_target_for_x(
        &self,
        snapshot: &BufferSnapshot,
        x: gpui::Pixels,
    ) -> (Point, SelectionGoal) {
        let point = self.point_for_x(snapshot, x - gutter_width());
        let source_offset = snapshot.as_text_snapshot().point_to_offset(point);
        (
            point,
            visual_horizontal_goal(0, self.visible_x_for_source_offset(source_offset)),
        )
    }

    pub(super) fn render(
        &self,
        row_style: RowDisplayStyle,
        cx: &mut Context<MarkdownEditor>,
    ) -> Vec<gpui::AnyElement> {
        let mouse_down_layout = self.clone();
        let mouse_move_layout = self.clone();
        let palette = editor_palette();
        let mut row = div()
            .w_full()
            .h(self.height)
            .flex()
            .relative()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.mouse_left_down_on_table_row(&mouse_down_layout, event, window, cx)
                }),
            )
            .on_mouse_move(cx.listener(move |this, event, window, cx| {
                this.mouse_move_on_table_row(&mouse_move_layout, event, window, cx)
            }));

        if self.is_delimiter {
            row = row.child(
                div().h_full().flex().items_center().child(
                    div()
                        .h(px(1.))
                        .w(self.width)
                        .bg(palette.gutter_text.opacity(0.55)),
                ),
            );
        } else {
            for cell in &self.cells {
                row = row.child(render_table_cell(
                    cell,
                    self.is_header,
                    self.height,
                    row_style,
                ));
            }
        }

        vec![row.into_any_element()]
    }

    fn source_offset_for_x(&self, x: gpui::Pixels) -> usize {
        let table_x = x.clamp(px(0.), self.width);
        let Some(cell) = self
            .cells
            .iter()
            .find(|cell| table_x >= cell.x && table_x <= cell.x + cell.width)
            .or_else(|| self.cells.last())
        else {
            return self.row_source_range.start;
        };
        if cell.content_range.is_empty() {
            return cell.content_range.start;
        }

        let content_x = (table_x - cell.x - TABLE_CELL_HORIZONTAL_PADDING)
            .clamp(px(0.), cell.width - TABLE_CELL_HORIZONTAL_PADDING * 2.);
        let ratio = if cell.width > TABLE_CELL_HORIZONTAL_PADDING * 2. {
            content_x / (cell.width - TABLE_CELL_HORIZONTAL_PADDING * 2.)
        } else {
            0.
        };
        cell.content_range.start + (cell.content_range.len() as f32 * ratio).round() as usize
    }

    fn cell_containing_source_offset(
        &self,
        source_offset: usize,
    ) -> Option<&DisplayTableCellLayout> {
        self.cells.iter().find(|cell| {
            (cell.source_range.start..=cell.source_range.end).contains(&source_offset)
                || (cell.content_range.start..=cell.content_range.end).contains(&source_offset)
        })
    }
}

impl DisplayTableLayout {
    pub(super) fn new(
        snapshot: &BufferSnapshot,
        table: &MarkdownTable,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        window: &mut Window,
    ) -> Self {
        let column_widths =
            table_column_widths(snapshot, table.rows(), wrap_width, row_style, window);
        let width = column_widths
            .iter()
            .fold(px(0.), |sum, width| sum + *width)
            .max(TABLE_MIN_CELL_WIDTH);
        Self {
            table_source_range: table.source_range.clone(),
            column_widths,
            width,
        }
    }
}

fn render_table_cell(
    cell: &DisplayTableCellLayout,
    is_header: bool,
    height: gpui::Pixels,
    row_style: RowDisplayStyle,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let mut content = div()
        .h_full()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .px(TABLE_CELL_HORIZONTAL_PADDING)
        .py(TABLE_CELL_VERTICAL_PADDING)
        .line_height(row_style.line_height)
        .text_color(palette.text);
    content = match cell.alignment {
        MarkdownTableAlignment::Left => content,
        MarkdownTableAlignment::Center => content.text_center(),
        MarkdownTableAlignment::Right => content.text_align(TextAlign::Right),
    };

    div()
        .absolute()
        .left(cell.x)
        .top_0()
        .w(cell.width)
        .h(height)
        .overflow_hidden()
        .border_1()
        .border_color(palette.gutter_text.opacity(0.35))
        .bg(if is_header {
            palette.fenced_code_background
        } else {
            palette.pipe_table_background
        })
        .child(content.children(render_table_cell_lines(cell, row_style)))
        .into_any_element()
}

fn render_table_cell_lines(
    cell: &DisplayTableCellLayout,
    row_style: RowDisplayStyle,
) -> Vec<gpui::AnyElement> {
    cell.visual_lines
        .iter()
        .map(|line_range| {
            let mut children = render_table_cell_segments(cell, line_range);
            if children.is_empty() {
                children.push(SharedString::from(String::new()).into_any_element());
            }

            let mut line = div()
                .w_full()
                .h(row_style.line_height)
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap();
            line = match cell.alignment {
                MarkdownTableAlignment::Left => line,
                MarkdownTableAlignment::Center => line.justify_center(),
                MarkdownTableAlignment::Right => line.justify_end(),
            };
            line.children(children).into_any_element()
        })
        .collect()
}

fn render_table_cell_segments(
    cell: &DisplayTableCellLayout,
    line_range: &Range<usize>,
) -> Vec<gpui::AnyElement> {
    cell.segments
        .iter()
        .filter_map(|segment| table_cell_segment_for_range(segment, line_range))
        .map(|segment| render_text_piece(segment.text, &segment.style))
        .collect()
}

fn table_cell_segment_for_range(
    segment: &StyledDisplaySegment,
    line_range: &Range<usize>,
) -> Option<StyledDisplaySegment> {
    let start = segment.display_range.start.max(line_range.start);
    let end = segment.display_range.end.min(line_range.end);
    if start >= end {
        return None;
    }

    let local_start = start - segment.display_range.start;
    let local_end = end - segment.display_range.start;
    let text = segment.text.get(local_start..local_end)?.to_string();
    Some(StyledDisplaySegment {
        display_range: start..end,
        text,
        style: segment.style.clone(),
    })
}

fn table_column_widths<'a>(
    snapshot: &BufferSnapshot,
    rows: impl Iterator<Item = &'a MarkdownTableRow>,
    wrap_width: gpui::Pixels,
    row_style: RowDisplayStyle,
    window: &mut Window,
) -> Vec<gpui::Pixels> {
    let mut widths: Vec<gpui::Pixels> = Vec::new();
    for row in rows {
        for (column, cell) in row.cells.iter().enumerate() {
            let preferred = table_cell_preferred_width(snapshot, cell, row_style, window);
            if widths.len() <= column {
                widths.push(TABLE_MIN_CELL_WIDTH.max(preferred));
            } else {
                widths[column] = widths[column].max(TABLE_MIN_CELL_WIDTH.max(preferred));
            }
        }
    }

    let total = widths.iter().fold(px(0.), |sum, width| sum + *width);
    if total > wrap_width && total > px(0.) {
        let scale = wrap_width.max(TABLE_MIN_CELL_WIDTH) / total;
        for width in &mut widths {
            *width = (*width * scale).max(TABLE_MIN_CELL_WIDTH);
        }
    }
    widths
}

fn table_cell_preferred_width(
    snapshot: &BufferSnapshot,
    cell: &MarkdownTableCell,
    row_style: RowDisplayStyle,
    window: &mut Window,
) -> gpui::Pixels {
    let (text, segments) = table_cell_display(snapshot, cell);
    if text.is_empty() {
        return TABLE_MIN_CELL_WIDTH;
    }

    let text_runs = text_runs_for_segments(&segments);
    let shaped_line = window.text_system().shape_line(
        SharedString::from(text),
        row_style.text_size,
        &text_runs,
        None,
    );
    shaped_line.width + table_cell_horizontal_inset()
}

fn table_cell_visual_lines(
    text: &str,
    segments: &[StyledDisplaySegment],
    width: gpui::Pixels,
    row_style: RowDisplayStyle,
    window: &mut Window,
) -> Vec<Range<usize>> {
    if text.is_empty() {
        return vec![0..0];
    }

    let content_width = table_cell_content_width(width);
    let text_runs = text_runs_for_segments(segments);
    let shaped_line = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        row_style.text_size,
        &text_runs,
        None,
    );
    if shaped_line.width <= content_width {
        return vec![0..text.len()];
    }

    let Some(wrapped_line) = window
        .text_system()
        .shape_text(
            SharedString::from(text.to_string()),
            row_style.text_size,
            &text_runs,
            Some(content_width),
            None,
        )
        .ok()
        .and_then(|wrapped_lines| wrapped_lines.into_iter().next())
    else {
        return vec![0..text.len()];
    };

    let mut ranges = Vec::new();
    let mut start = 0;
    for wrap_boundary in wrapped_line.wrap_boundaries() {
        let Some(glyph) = wrapped_line
            .unwrapped_layout
            .runs
            .get(wrap_boundary.run_ix)
            .and_then(|run| run.glyphs.get(wrap_boundary.glyph_ix))
        else {
            return vec![0..text.len()];
        };
        if glyph.index > start && glyph.index <= text.len() {
            ranges.push(start..glyph.index);
            start = glyph.index;
        }
    }
    ranges.push(start..text.len());
    ranges
}

fn table_cell_content_width(width: gpui::Pixels) -> gpui::Pixels {
    (width - table_cell_horizontal_inset()).max(px(1.))
}

fn table_cell_horizontal_inset() -> gpui::Pixels {
    TABLE_CELL_HORIZONTAL_PADDING * 2. + TABLE_BORDER_WIDTH * 2.
}

fn table_cell_display(
    snapshot: &BufferSnapshot,
    cell: &MarkdownTableCell,
) -> (String, Vec<StyledDisplaySegment>) {
    if cell.content_range.is_empty() {
        return (String::new(), Vec::new());
    }

    let syntax_tree = snapshot.syntax_tree();
    let inline_spans = syntax_tree
        .inline_spans_in_source_range(cell.content_range.clone())
        .collect::<Vec<_>>();
    let mut hidden_ranges = Vec::new();
    let mut style_ranges = Vec::new();
    let mut breakpoints = vec![cell.content_range.start, cell.content_range.end];

    for span in inline_spans {
        for marker_range in &span.marker_ranges {
            let clipped = clipped_range(marker_range.clone(), cell.content_range.clone());
            if !clipped.is_empty() {
                breakpoints.push(clipped.start);
                breakpoints.push(clipped.end);
                hidden_ranges.push(clipped);
            }
        }

        let style = inline_style(span.kind);
        for content_range in &span.content_ranges {
            let clipped = clipped_range(content_range.clone(), cell.content_range.clone());
            if !clipped.is_empty() {
                breakpoints.push(clipped.start);
                breakpoints.push(clipped.end);
                style_ranges.push((clipped, style.clone()));
            }
        }
    }

    breakpoints.sort_unstable();
    breakpoints.dedup();

    let mut display_text = String::new();
    let mut segments = Vec::new();
    for window in breakpoints.windows(2) {
        let source_range = window[0]..window[1];
        if source_range.is_empty()
            || hidden_ranges
                .iter()
                .any(|hidden_range| range_contains(hidden_range, &source_range))
        {
            continue;
        }

        let text = snapshot
            .as_text_snapshot()
            .text_for_range(source_range.clone())
            .collect::<String>();
        if text.is_empty() {
            continue;
        }

        let display_start = display_text.len();
        display_text.push_str(&text);
        let display_end = display_text.len();
        let style = table_cell_style_for_range(&style_ranges, &source_range);
        segments.push(StyledDisplaySegment {
            display_range: display_start..display_end,
            text,
            style,
        });
    }

    (display_text, segments)
}

fn table_cell_style_for_range(
    style_ranges: &[(Range<usize>, DisplayTextStyle)],
    source_range: &Range<usize>,
) -> DisplayTextStyle {
    let mut style = DisplayTextStyle::default();
    for (style_range, range_style) in style_ranges {
        if range_contains(style_range, source_range) {
            style = style.merge(range_style);
        }
    }
    style
}

fn clipped_range(range: Range<usize>, bounds: Range<usize>) -> Range<usize> {
    range.start.max(bounds.start)..range.end.min(bounds.end)
}
