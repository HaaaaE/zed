use std::ops::Range;

use gpui::{Context, IntoElement, MouseButton, SharedString, TextAlign, div, prelude::*, px};
use markdown_wysiwyg::{MarkdownTableAlignment, MarkdownTableCell, MarkdownTableRow};
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection, SelectionGoal};
use md_theme::{editor_palette, gutter_width};

use super::{
    MarkdownEditor, MarkdownEditorMode, RowDisplayStyle, VisualLineBoundary, clip_cursor,
    display_model::DisplayRow, rendered_element_source_range_is_active, visual_horizontal_goal,
};

const TABLE_CELL_HORIZONTAL_PADDING: gpui::Pixels = px(8.);
const TABLE_CELL_VERTICAL_PADDING: gpui::Pixels = px(3.);
const TABLE_BORDER_WIDTH: gpui::Pixels = px(1.);
const TABLE_MIN_CELL_WIDTH: gpui::Pixels = px(32.);

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
    pub(super) x: gpui::Pixels,
    pub(super) width: gpui::Pixels,
    pub(super) alignment: MarkdownTableAlignment,
}

impl DisplayTableRowLayout {
    pub(super) fn for_display_row(
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
    ) -> Option<Self> {
        if mode != MarkdownEditorMode::Rendered
            || rendered_element_source_range_is_active(
                snapshot,
                selection,
                &display_row.source_range,
            )
        {
            return None;
        }

        let (table, table_row) = snapshot
            .syntax_tree()
            .table_row_for_source_row(display_row.row as usize)?;
        let is_header = table.header.row == table_row.row;
        let is_delimiter = table.delimiter.row == table_row.row;
        let column_widths = table_column_widths(snapshot, table.rows(), wrap_width, row_style);
        let mut x = px(0.);
        let cells = table_row
            .cells
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let width = column_widths
                    .get(column)
                    .copied()
                    .unwrap_or(TABLE_MIN_CELL_WIDTH);
                let layout = DisplayTableCellLayout {
                    source_range: cell.source_range.clone(),
                    content_range: cell.content_range.clone(),
                    text: table_cell_text(snapshot, cell),
                    x,
                    width,
                    alignment: table.alignments.get(column).copied().unwrap_or_default(),
                };
                x += width;
                layout
            })
            .collect::<Vec<_>>();
        let width = x.max(TABLE_MIN_CELL_WIDTH);
        let height = if is_delimiter {
            (row_style.line_height * 0.45).max(px(6.))
        } else {
            row_style.line_height + TABLE_CELL_VERTICAL_PADDING * 2. + TABLE_BORDER_WIDTH
        };

        Some(Self {
            table_source_range: table.source_range.clone(),
            row_source_range: table_row.source_range.clone(),
            row: table_row.row,
            cells,
            is_header,
            is_delimiter,
            height,
            width,
        })
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

    pub(super) fn render(&self, cx: &mut Context<MarkdownEditor>) -> Vec<gpui::AnyElement> {
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
                row = row.child(render_table_cell(cell, self.is_header, self.height));
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

fn render_table_cell(
    cell: &DisplayTableCellLayout,
    is_header: bool,
    height: gpui::Pixels,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let mut content = div()
        .h_full()
        .w_full()
        .px(TABLE_CELL_HORIZONTAL_PADDING)
        .flex()
        .items_center()
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
        .border_1()
        .border_color(palette.gutter_text.opacity(0.35))
        .bg(if is_header {
            palette.fenced_code_background
        } else {
            palette.pipe_table_background
        })
        .child(content.child(SharedString::from(cell.text.clone())))
        .into_any_element()
}

fn table_column_widths<'a>(
    snapshot: &BufferSnapshot,
    rows: impl Iterator<Item = &'a MarkdownTableRow>,
    wrap_width: gpui::Pixels,
    row_style: RowDisplayStyle,
) -> Vec<gpui::Pixels> {
    let mut widths: Vec<gpui::Pixels> = Vec::new();
    for row in rows {
        for (column, cell) in row.cells.iter().enumerate() {
            let text = table_cell_text(snapshot, cell);
            let preferred = px(text.chars().count() as f32 * f32::from(row_style.text_size) * 0.55)
                + TABLE_CELL_HORIZONTAL_PADDING * 2.
                + TABLE_BORDER_WIDTH;
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

fn table_cell_text(snapshot: &BufferSnapshot, cell: &MarkdownTableCell) -> String {
    snapshot
        .as_text_snapshot()
        .text_for_range(cell.content_range.clone())
        .collect()
}
