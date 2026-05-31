use std::{ops::Range, path::Path};

use gpui::{
    FontStyle, FontWeight, LineFragment, StrikethroughStyle, TextRun, UnderlineStyle, font, px,
};
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind};
use md_assets::EDITOR_FONT_FAMILY;
use md_buffer::BufferSnapshot;
use md_theme::{default_row_metrics, editor_palette, heading_row_metrics};

use crate::{
    DisplayInlineAtom, DisplayInlineFragment, DisplayInlineRowInputs, MarkdownEditorMode,
    RowDisplayStyle,
    display_model::{DisplayRow, DisplayTextStyle, StyledDisplaySegment},
    ranges_overlap,
    rendered_element::rendered_element_descriptor_for_inline_span_in_row,
};
pub(super) fn text_runs_for_segments(segments: &[StyledDisplaySegment]) -> Vec<TextRun> {
    text_runs_for_segment_lengths(
        segments
            .iter()
            .map(|segment| (segment.text.len(), segment.style.clone())),
    )
}

pub(super) fn text_runs_for_display_segments(
    display_text: &str,
    segments: &[StyledDisplaySegment],
) -> Vec<TextRun> {
    text_runs_for_segment_lengths(segments.iter().filter_map(|segment| {
        let text = display_text.get(segment.display_range.clone())?;
        Some((text.len(), segment.style.clone()))
    }))
}

pub(super) fn text_runs_for_segment_lengths(
    segments: impl IntoIterator<Item = (usize, DisplayTextStyle)>,
) -> Vec<TextRun> {
    let palette = editor_palette();
    let mut runs = Vec::new();

    for (len, style) in segments {
        if len == 0 {
            continue;
        }

        let mut run_font = font(EDITOR_FONT_FAMILY);
        if let Some(font_weight) = style.font_weight {
            run_font.weight = font_weight;
        }
        if style.italic {
            run_font.style = FontStyle::Italic;
        }

        let color = style.color.unwrap_or(palette.text);
        runs.push(TextRun {
            len,
            font: run_font,
            color,
            background_color: style.text_background,
            underline: style.underline.then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(color),
                wavy: false,
            }),
            strikethrough: style.line_through.then_some(StrikethroughStyle {
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

pub(super) fn text_runs_on_char_boundaries(text: &str, runs: &[TextRun]) -> Vec<TextRun> {
    if runs.is_empty() {
        return Vec::new();
    }

    let mut normalized = Vec::with_capacity(runs.len());
    let mut start = 0;
    let mut desired_end = 0usize;
    for (index, run) in runs.iter().enumerate() {
        desired_end = desired_end.saturating_add(run.len);
        let end = if index == runs.len() - 1 || desired_end >= text.len() {
            text.len()
        } else {
            text.floor_char_boundary(desired_end)
        };
        if end <= start {
            continue;
        }

        let mut run = run.clone();
        run.len = end - start;
        normalized.push(run);
        start = end;
    }

    if start < text.len()
        && let Some(last) = normalized.last_mut()
    {
        last.len += text.len() - start;
    }

    normalized
}

pub(super) fn display_inline_fragments(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    document_path: Option<&Path>,
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

    let row_inputs = display_inline_row_inputs(snapshot, display_row, row_style, document_path);
    let hidden_ranges = display_row.projection.hidden_ranges();
    let mut breakpoints = vec![source_range.start, source_range.end];
    for operation in display_row.projection.operations() {
        let operation_range = operation.source_range();
        breakpoints.push(operation_range.start.max(source_range.start));
        breakpoints.push(operation_range.end.min(source_range.end));
    }
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
    display_text: &str,
    fragments: &[DisplayInlineFragment],
) -> Vec<StyledDisplaySegment> {
    let mut segments: Vec<StyledDisplaySegment> = Vec::new();
    for fragment in fragments {
        let segment = match fragment {
            DisplayInlineFragment::Text(segment) => {
                let text = segment_text(display_text, segment).unwrap_or_default();
                StyledDisplaySegment {
                    display_range: segment.display_range.clone(),
                    text,
                    style: segment.style.clone(),
                }
            }
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

pub(super) fn segment_text(display_text: &str, segment: &StyledDisplaySegment) -> Option<String> {
    if !segment.text.is_empty() || segment.display_range.is_empty() {
        if let Some(text) = display_text.get(segment.display_range.clone()) {
            return Some(text.to_string());
        }
        return Some(segment.text.clone());
    }

    display_text
        .get(segment.display_range.clone())
        .map(str::to_string)
}

pub(super) fn display_inline_row_inputs(
    _snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    row_style: RowDisplayStyle,
    document_path: Option<&Path>,
) -> DisplayInlineRowInputs {
    let row_source_range = &display_row.source_range;
    let mut inputs = DisplayInlineRowInputs::default();

    collect_block_style_ranges_for_row(display_row, &mut inputs.style_ranges);

    let hidden_ranges = display_row.projection.hidden_ranges();
    for span in &display_row.inline_spans {
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

        let atom = display_row
            .rendered_element_descriptors
            .iter()
            .find(|descriptor| descriptor.source_range == span.source_range)
            .cloned()
            .or_else(|| {
                if document_path.is_none()
                    || display_row.rendered_element_descriptors_have_document_path
                {
                    return None;
                }

                rendered_element_descriptor_for_inline_span_in_row(
                    span,
                    &display_row.source_text,
                    row_source_range,
                    document_path,
                )
            })
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
    _snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
) -> RowDisplayStyle {
    if mode == MarkdownEditorMode::Rendered {
        if let Some(level) = display_row.heading_level {
            return heading_row_metrics(level).into();
        }
    }

    default_row_metrics().into()
}

pub(super) fn has_inline_atoms(fragments: &[DisplayInlineFragment]) -> bool {
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

pub(super) fn wrap_boundary_glyph(
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
        text: String::new(),
        style: DisplayTextStyle::default(),
    })]
}

fn collect_block_style_ranges_for_row(
    display_row: &DisplayRow,
    style_ranges: &mut Vec<(Range<usize>, DisplayTextStyle)>,
) {
    let row_source_range = &display_row.source_range;
    for block in &display_row.markdown_blocks {
        match block.kind {
            MarkdownBlockKind::AtxHeading { level }
            | MarkdownBlockKind::SetextHeading { level } => {
                push_style_range(
                    style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    heading_style(level),
                );
            }
            MarkdownBlockKind::FencedCodeBlock | MarkdownBlockKind::IndentedCodeBlock => {
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
            MarkdownBlockKind::HtmlBlock => {
                push_style_range(
                    style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    html_raw_style(),
                );
            }
            MarkdownBlockKind::Blank
            | MarkdownBlockKind::Paragraph
            | MarkdownBlockKind::ThematicBreak
            | MarkdownBlockKind::BlockQuote
            | MarkdownBlockKind::OrderedList
            | MarkdownBlockKind::UnorderedList
            | MarkdownBlockKind::ListItem
            | MarkdownBlockKind::TaskListItem { .. }
            | MarkdownBlockKind::LinkReferenceDefinition => {}
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
        MarkdownInlineKind::InlineHtml => html_raw_style(),
        MarkdownInlineKind::Escape
        | MarkdownInlineKind::Entity
        | MarkdownInlineKind::HardBreak
        | MarkdownInlineKind::SoftBreak => DisplayTextStyle::default(),
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

fn html_raw_style() -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        color: Some(palette.muted_text),
        ..Default::default()
    }
}
