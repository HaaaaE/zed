use std::ops::Range;

use gpui::{
    App, FontStyle, FontWeight, LineFragment, SharedString, StrikethroughStyle, TextRun,
    UnderlineStyle, Window, font, px,
};
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind};
use md_assets::EDITOR_FONT_FAMILY;
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection};
use md_theme::{default_row_metrics, editor_palette, gutter_width, heading_row_metrics};

use super::{
    DisplayInlineAtom, DisplayInlineFragment, DisplayInlineRowInputs, MarkdownEditorMode,
    RowDisplayStyle, active_source_range_for_selection,
    block::DisplayBlockLayout,
    display_model::{DisplayRow, DisplayTextStyle, StyledDisplaySegment},
    inactive_rendered_element_source_ranges_for_selection, ranges_overlap,
    rendered_element_descriptor_for_inline_span_in_row,
    visual_row::display_x_for_offset,
};

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
pub(super) enum DisplayRowLayout {
    Text(DisplayRowTextLayout),
    Block(DisplayBlockLayout),
}

impl DisplayRowLayout {
    pub(super) fn cacheable(&self) -> bool {
        match self {
            Self::Text(text_layout) => text_layout.cacheable,
            Self::Block(block_layout) => block_layout.cacheable(),
        }
    }

    pub(super) fn row_min_height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::Text(text_layout) => row_style.min_height.max(text_layout.height(row_style)),
            Self::Block(block_layout) => row_style.min_height.max(block_layout.height()),
        }
    }

    pub(super) fn content_min_height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::Text(text_layout) => text_layout.height(row_style),
            Self::Block(block_layout) => row_style.min_height.max(block_layout.height()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct DisplayRowCacheKey {
    pub(super) version: md_text::Global,
    pub(super) row: u32,
    pub(super) mode: MarkdownEditorMode,
    pub(super) active_projection_source_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
pub(super) struct DisplayRowProjectionState {
    pub(super) active_source_range: Option<Range<usize>>,
    pub(super) inactive_source_ranges: Vec<Range<usize>>,
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RowLayoutCacheKey {
    pub(super) row: u32,
    pub(super) mode: MarkdownEditorMode,
    pub(super) wrap_width: gpui::Pixels,
    pub(super) active_projection_source_ranges: Vec<Range<usize>>,
}

pub(super) fn text_wrap_width(window: &Window) -> gpui::Pixels {
    (window.bounds().size.width - gutter_width()).max(px(1.))
}

pub(super) fn compute_display_row_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    measure_layout: bool,
    window: &mut Window,
    cx: &mut App,
) -> DisplayRowLayout {
    if let Some(block_layout) = DisplayBlockLayout::for_display_row(
        snapshot,
        display_row,
        selection,
        mode,
        wrap_width,
        row_style,
        measure_layout,
        window,
        cx,
    ) {
        return DisplayRowLayout::Block(block_layout);
    }

    DisplayRowLayout::Text(text_layout_for_display_row(
        snapshot,
        display_row,
        mode,
        row_style,
        wrap_width,
        measure_layout,
        window,
        cx,
    ))
}

pub(super) fn text_layout_for_display_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    measure_inline_atoms: bool,
    window: &mut Window,
    cx: &mut App,
) -> DisplayRowTextLayout {
    let fragments = display_fragments_for_text_layout(snapshot, display_row, mode, row_style);
    text_layout_for_fragments(
        display_row,
        fragments,
        row_style,
        wrap_width,
        measure_inline_atoms,
        window,
        cx,
    )
}

pub(super) fn source_text_layout_for_display_row(
    display_row: &DisplayRow,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    measure_inline_atoms: bool,
    window: &mut Window,
    cx: &mut App,
) -> DisplayRowTextLayout {
    text_layout_for_fragments(
        display_row,
        source_display_fragments(display_row),
        row_style,
        wrap_width,
        measure_inline_atoms,
        window,
        cx,
    )
}

pub(super) fn text_layout_for_fragments(
    display_row: &DisplayRow,
    mut fragments: Vec<DisplayInlineFragment>,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    measure_inline_atoms: bool,
    window: &mut Window,
    cx: &mut App,
) -> DisplayRowTextLayout {
    let segments = text_segments_for_fragments(&fragments);
    let text_runs = text_runs_for_segments(&segments);
    let shaped_line = window.text_system().shape_line(
        SharedString::from(display_row.text.clone()),
        row_style.text_size,
        &text_runs,
        None,
    );
    let has_inline_atoms = has_inline_atoms(&fragments);
    let mut cacheable = true;
    if has_inline_atoms {
        if measure_inline_atoms {
            cacheable =
                measure_inline_atom_sizes(&mut fragments, &shaped_line, row_style, window, cx);
        } else {
            assign_inline_atom_fallback_sizes(&mut fragments, &shaped_line, row_style);
            cacheable = false;
        }
    }
    let visual_rows = if has_inline_atoms {
        visual_rows_for_fragments(
            &display_row.text,
            &fragments,
            &shaped_line,
            row_style,
            wrap_width,
            cx,
        )
    } else if let Some(visual_rows) = unwrapped_visual_rows_if_fits(
        display_row.text.len(),
        &fragments,
        row_style,
        shaped_line.width(),
        wrap_width,
    ) {
        visual_rows
    } else {
        match window.text_system().shape_text(
            SharedString::from(display_row.text.clone()),
            row_style.text_size,
            &text_runs,
            Some(wrap_width),
            None,
        ) {
            Ok(wrapped_lines) => wrapped_lines
                .first()
                .map(|wrapped_line| {
                    visual_rows_for_wrapped_line(wrapped_line, &fragments, row_style)
                })
                .unwrap_or_else(|| {
                    fallback_visual_rows(display_row.text.len(), &fragments, row_style)
                }),
            Err(_) => fallback_visual_rows(display_row.text.len(), &fragments, row_style),
        }
    };

    DisplayRowTextLayout {
        fragments,
        visual_rows,
        shaped_line,
        text_len: display_row.text.len(),
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

pub(super) fn display_fragments_for_text_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
) -> Vec<DisplayInlineFragment> {
    if mode == MarkdownEditorMode::Source {
        return source_display_fragments(display_row);
    }

    display_inline_fragments(snapshot, display_row, mode, row_style)
}

pub(super) fn assign_inline_atom_fallback_sizes(
    fragments: &mut [DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
) {
    for fragment in fragments {
        let DisplayInlineFragment::Atom(atom) = fragment else {
            continue;
        };
        let size = atom.fallback_size(shaped_line, row_style);
        atom.width = size.width;
        atom.height = size.height;
    }
}

pub(super) fn measure_inline_atom_sizes(
    fragments: &mut [DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    row_style: RowDisplayStyle,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let mut cacheable = true;
    for fragment in fragments {
        let DisplayInlineFragment::Atom(atom) = fragment else {
            continue;
        };
        let fallback_size = atom.fallback_size(shaped_line, row_style);
        let measurement = atom.measure_size(fallback_size, row_style, window, cx);
        atom.width = measurement.size.width;
        atom.height = measurement.size.height;
        cacheable &= measurement.cacheable;
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
        let boundary_index = atomic_wrap_boundary_index(fragments, boundary.ix, start);
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
        if glyph.index < start {
            if atom_range_containing_display_index(fragments, glyph.index)
                .is_some_and(|atom_range| atom_range.end <= start)
            {
                continue;
            }
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        }
        let boundary_index = atomic_wrap_boundary_index(fragments, glyph.index, start);
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

pub(super) fn text_runs_for_segments(segments: &[StyledDisplaySegment]) -> Vec<TextRun> {
    let palette = editor_palette();
    let mut runs = Vec::new();

    for segment in segments {
        if segment.text.is_empty() {
            continue;
        }

        let mut run_font = font(EDITOR_FONT_FAMILY);
        if let Some(font_weight) = segment.style.font_weight {
            run_font.weight = font_weight;
        }
        if segment.style.italic {
            run_font.style = FontStyle::Italic;
        }

        let color = segment.style.color.unwrap_or(palette.text);
        runs.push(TextRun {
            len: segment.text.len(),
            font: run_font,
            color,
            background_color: segment.style.text_background,
            underline: segment.style.underline.then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(color),
                wavy: false,
            }),
            strikethrough: segment.style.line_through.then_some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(color),
            }),
        });
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: 0,
            font: font(EDITOR_FONT_FAMILY),
            color: palette.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}

pub(super) fn display_inline_fragments(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
) -> Vec<DisplayInlineFragment> {
    if mode != MarkdownEditorMode::Rendered {
        return source_display_fragments(display_row);
    }

    let source_range = display_row.source_range.clone();
    let source_text = &display_row.source_text;
    if source_text.is_empty() {
        return vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..0,
            text: String::new(),
            style: DisplayTextStyle::default(),
        })];
    }

    let row_inputs = display_inline_row_inputs(snapshot, display_row, row_style);
    let hidden_ranges = display_row.projection.hidden_ranges();
    let mut breakpoints = vec![source_range.start, source_range.end];
    for hidden_range in hidden_ranges {
        breakpoints.push(hidden_range.start.max(source_range.start));
        breakpoints.push(hidden_range.end.min(source_range.end));
    }
    for (style_range, _) in &row_inputs.style_ranges {
        breakpoints.push(style_range.start);
        breakpoints.push(style_range.end);
    }
    for atom_range in &row_inputs.atom_ranges {
        breakpoints.push(atom_range.source_range.start);
        breakpoints.push(atom_range.source_range.end);
    }
    breakpoints.sort_unstable();
    breakpoints.dedup();

    let mut fragments: Vec<DisplayInlineFragment> = Vec::new();
    for window in breakpoints.windows(2) {
        let interval = window[0]..window[1];
        if interval.start >= interval.end {
            continue;
        }

        if let Some(atom) = row_inputs
            .atom_ranges
            .iter()
            .find(|atom| range_contains(&atom.source_range, &interval))
        {
            if !fragments.iter().any(|fragment| {
                matches!(fragment, DisplayInlineFragment::Atom(existing) if existing.source_range == atom.source_range)
            }) {
                fragments.push(DisplayInlineFragment::Atom(atom.clone()));
            }
            continue;
        }

        if hidden_ranges
            .iter()
            .any(|hidden_range| range_contains(hidden_range, &interval))
        {
            continue;
        }

        let local_start = interval.start - source_range.start;
        let local_end = interval.end - source_range.start;
        let Some(text) = source_text.get(local_start..local_end) else {
            continue;
        };
        let text = text.to_string();
        if text.is_empty() {
            continue;
        }

        let style = combined_style_for_range(&row_inputs.style_ranges, &interval);
        let display_range = display_row.source_to_display(interval.start)
            ..display_row.source_to_display(interval.end);

        if let Some(DisplayInlineFragment::Text(previous)) = fragments.last_mut()
            && previous.style == style
            && previous.display_range.end == display_range.start
        {
            previous.text.push_str(&text);
            previous.display_range.end = display_range.end;
        } else {
            fragments.push(DisplayInlineFragment::Text(StyledDisplaySegment {
                display_range,
                text,
                style,
            }));
        }
    }

    if fragments.is_empty() {
        vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..display_row.text.len(),
            text: display_row.text.clone(),
            style: DisplayTextStyle::default(),
        })]
    } else {
        fragments
    }
}

pub(super) fn text_segments_for_fragments(
    fragments: &[DisplayInlineFragment],
) -> Vec<StyledDisplaySegment> {
    let mut segments: Vec<StyledDisplaySegment> = Vec::new();
    for fragment in fragments {
        let segment = match fragment {
            DisplayInlineFragment::Text(segment) => segment.clone(),
            DisplayInlineFragment::Atom(atom) => StyledDisplaySegment {
                display_range: atom.display_range.clone(),
                text: atom.fallback_text.clone(),
                style: atom.style.clone(),
            },
        };

        if let Some(previous) = segments.last_mut()
            && previous.style == segment.style
            && previous.display_range.end == segment.display_range.start
        {
            previous.text.push_str(&segment.text);
            previous.display_range.end = segment.display_range.end;
        } else {
            segments.push(segment);
        }
    }

    segments
}

pub(super) fn display_inline_row_inputs(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    row_style: RowDisplayStyle,
) -> DisplayInlineRowInputs {
    let row_source_range = &display_row.source_range;
    let mut inputs = DisplayInlineRowInputs::default();

    collect_block_style_ranges_for_row(
        snapshot,
        row_source_range.clone(),
        &mut inputs.style_ranges,
    );

    let hidden_ranges = display_row.projection.hidden_ranges();
    for span in snapshot
        .syntax_tree()
        .inline_spans_in_source_range(row_source_range.clone())
    {
        let style = inline_style(span.kind);
        for content_range in &span.content_ranges {
            push_style_range(
                &mut inputs.style_ranges,
                row_source_range.clone(),
                content_range.clone(),
                style.clone(),
            );
        }

        if !range_contains(row_source_range, &span.source_range)
            || !span.marker_ranges.iter().any(|marker_range| {
                hidden_ranges
                    .iter()
                    .any(|hidden_range| ranges_overlap(marker_range, hidden_range))
            })
        {
            continue;
        }

        let atom = rendered_element_descriptor_for_inline_span_in_row(
            span,
            &display_row.source_text,
            row_source_range,
        )
        .and_then(|descriptor| {
            DisplayInlineAtom::from_descriptor(display_row, descriptor, row_style)
        });
        if let Some(atom) = atom {
            inputs.atom_ranges.push(atom);
        }
    }

    inputs
}

pub(super) fn line_fragments_for_wrapping<'a>(
    display_text: &'a str,
    fragments: &'a [DisplayInlineFragment],
) -> Option<Vec<LineFragment<'a>>> {
    let mut line_fragments = Vec::new();
    for fragment in fragments {
        match fragment {
            DisplayInlineFragment::Text(segment) => {
                let text = display_text.get(segment.display_range.clone())?;
                if !text.is_empty() {
                    line_fragments.push(LineFragment::text(text));
                }
            }
            DisplayInlineFragment::Atom(atom) => {
                atom.push_line_fragment(display_text, &mut line_fragments)?
            }
        }
    }
    Some(line_fragments)
}

pub(super) fn row_display_style_for_display_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
) -> RowDisplayStyle {
    if mode == MarkdownEditorMode::Rendered {
        if let Some(level) = heading_level_for_display_row(snapshot, display_row) {
            return heading_row_metrics(level).into();
        }
    }

    default_row_metrics().into()
}

fn has_inline_atoms(fragments: &[DisplayInlineFragment]) -> bool {
    fragments
        .iter()
        .any(|fragment| matches!(fragment, DisplayInlineFragment::Atom(_)))
}

pub(super) fn atomic_wrap_boundary_index(
    fragments: &[DisplayInlineFragment],
    boundary_index: usize,
    row_start: usize,
) -> usize {
    let Some(atom_range) = atom_range_containing_display_index(fragments, boundary_index) else {
        return boundary_index;
    };

    if atom_range.start > row_start {
        atom_range.start
    } else {
        atom_range.end
    }
}

pub(super) fn atom_range_containing_display_index(
    fragments: &[DisplayInlineFragment],
    display_index: usize,
) -> Option<Range<usize>> {
    fragments.iter().find_map(|fragment| match fragment {
        DisplayInlineFragment::Text(_) => None,
        DisplayInlineFragment::Atom(atom) if atom.contains_display_index(display_index) => {
            Some(atom.display_range.clone())
        }
        DisplayInlineFragment::Atom(_) => None,
    })
}

fn wrap_boundary_glyph(
    wrapped_line: &gpui::WrappedLine,
    wrap_boundary: gpui::WrapBoundary,
) -> Option<&gpui::ShapedGlyph> {
    wrapped_line
        .unwrapped_layout
        .runs
        .get(wrap_boundary.run_ix)
        .and_then(|run| run.glyphs.get(wrap_boundary.glyph_ix))
}

fn push_style_range(
    style_ranges: &mut Vec<(Range<usize>, DisplayTextStyle)>,
    row_source_range: Range<usize>,
    style_range: Range<usize>,
    style: DisplayTextStyle,
) {
    let clipped_start = style_range.start.max(row_source_range.start);
    let clipped_end = style_range.end.min(row_source_range.end);
    if clipped_start < clipped_end {
        style_ranges.push((clipped_start..clipped_end, style));
    }
}

fn range_contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

pub(super) fn source_display_fragments(display_row: &DisplayRow) -> Vec<DisplayInlineFragment> {
    vec![DisplayInlineFragment::Text(StyledDisplaySegment {
        display_range: 0..display_row.text.len(),
        text: display_row.text.clone(),
        style: DisplayTextStyle::default(),
    })]
}

fn collect_block_style_ranges_for_row(
    snapshot: &BufferSnapshot,
    row_source_range: Range<usize>,
    style_ranges: &mut Vec<(Range<usize>, DisplayTextStyle)>,
) {
    for block in snapshot
        .syntax_tree()
        .blocks_in_source_range(row_source_range.clone())
    {
        match block.kind {
            MarkdownBlockKind::AtxHeading { level } => {
                push_style_range(
                    style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    heading_style(level),
                );
            }
            MarkdownBlockKind::FencedCodeBlock => {
                push_style_range(
                    style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    fenced_code_style(),
                );
            }
            MarkdownBlockKind::PipeTable => {
                push_style_range(
                    style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    pipe_table_style(),
                );
            }
            MarkdownBlockKind::Blank | MarkdownBlockKind::Paragraph => {}
        }
    }
}

fn combined_style_for_range(
    style_ranges: &[(Range<usize>, DisplayTextStyle)],
    interval: &Range<usize>,
) -> DisplayTextStyle {
    let mut combined = DisplayTextStyle::default();
    for (style_range, style) in style_ranges {
        if range_contains(style_range, interval) {
            combined = combined.merge(style);
        }
    }
    combined
}

fn heading_level_for_display_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
) -> Option<u8> {
    heading_level_for_source_range(
        snapshot,
        display_row.source_range.clone(),
        display_row.row as usize,
    )
}

fn heading_level_for_source_range(
    snapshot: &BufferSnapshot,
    source_range: Range<usize>,
    row: usize,
) -> Option<u8> {
    snapshot
        .syntax_tree()
        .blocks_in_source_range(source_range)
        .find_map(|block| match block.kind {
            MarkdownBlockKind::AtxHeading { level } if block.row_range.start == row => Some(level),
            _ => None,
        })
}

pub(super) fn inline_style(kind: MarkdownInlineKind) -> DisplayTextStyle {
    let palette = editor_palette();
    match kind {
        MarkdownInlineKind::Strong => DisplayTextStyle {
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        },
        MarkdownInlineKind::Emphasis => DisplayTextStyle {
            italic: true,
            ..Default::default()
        },
        MarkdownInlineKind::InlineCode => DisplayTextStyle {
            color: Some(palette.inline_code_text),
            text_background: Some(palette.inline_code_background),
            font_weight: Some(FontWeight::MEDIUM),
            ..Default::default()
        },
        MarkdownInlineKind::Link | MarkdownInlineKind::Image => DisplayTextStyle {
            color: Some(palette.link_text),
            underline: true,
            ..Default::default()
        },
        MarkdownInlineKind::Strikethrough => DisplayTextStyle {
            line_through: true,
            color: Some(palette.muted_text),
            ..Default::default()
        },
        MarkdownInlineKind::InlineMath => DisplayTextStyle {
            color: Some(palette.inline_math_text),
            italic: true,
            ..Default::default()
        },
    }
}

fn heading_style(level: u8) -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        color: Some(match level {
            1 | 2 => palette.heading_primary,
            3 => palette.heading_accent,
            _ => palette.heading_muted,
        }),
        font_weight: Some(match level {
            1 => FontWeight::BLACK,
            2 => FontWeight::EXTRA_BOLD,
            3 => FontWeight::BOLD,
            _ => FontWeight::SEMIBOLD,
        }),
        ..Default::default()
    }
}

fn fenced_code_style() -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        text_background: Some(palette.fenced_code_background),
        ..Default::default()
    }
}

fn pipe_table_style() -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        color: Some(palette.pipe_table_text),
        text_background: Some(palette.pipe_table_background),
        ..Default::default()
    }
}
