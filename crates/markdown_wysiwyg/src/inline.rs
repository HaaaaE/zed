use std::{collections::HashMap, ops::Range, sync::OnceLock};

#[cfg(any(test, perf_enabled))]
use comrak::{
    Arena, Options,
    nodes::{Node as ComrakNode, NodeValue, Sourcepos},
    parse_document,
};
use tree_sitter::Node;

use super::{
    MarkdownBlock, MarkdownBlockKind, MarkdownInlineKind, MarkdownInlineSpan, MarkdownInlineTree,
    MarkdownProjectionReplacement, MarkdownSyntaxTree, ProjectionMarkerDependency,
    source::{
        old_range_for_clean_new_range, range_contains, ranges_overlap, ranges_touch,
        shift_clean_old_range_to_new,
    },
    structure::{MarkdownInlineSemantics, MarkdownStructure},
};
#[cfg(any(test, perf_enabled))]
use super::{record_timed_comrak_marker_scan, record_timed_comrak_sourcepos_mapping};

impl MarkdownSyntaxTree {
    pub fn inline_spans(&self) -> &[MarkdownInlineSpan] {
        &self.data.inline_spans
    }

    pub fn inline_spans_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownInlineSpan> {
        let start = range.start;
        let end = range.end;
        let start_index = self.partition_inline_spans_by_prefix_end(start);
        self.data.inline_spans[start_index..]
            .iter()
            .take_while(move |span| span.source_range.start < end)
            .filter(move |span| span.source_range.start < end && span.source_range.end > start)
    }

    fn partition_inline_spans_by_prefix_end(&self, offset: usize) -> usize {
        self.data
            .inline_span_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }
}

pub(super) fn collect_inline_semantics_for_inline_trees(
    source: &str,
    inline_trees: &[MarkdownInlineTree],
) -> Vec<MarkdownInlineSemantics> {
    inline_trees
        .iter()
        .map(|inline_tree| MarkdownInlineSemantics {
            parent_id: inline_tree.parent_id,
            parent_range: inline_tree.parent_range.clone(),
            spans: collect_inline_spans_for_inline_tree(source, inline_tree),
            replacements: collect_projection_replacements_for_inline_tree(source, inline_tree),
        })
        .inspect(|semantics| {
            debug_assert_ne!(semantics.parent_id, 0);
        })
        .collect()
}

#[cfg(any(test, perf_enabled))]
pub(super) fn collect_comrak_inline_semantics_for_inline_trees(
    source: &str,
    inline_trees: &[MarkdownInlineTree],
) -> Vec<MarkdownInlineSemantics> {
    inline_trees
        .iter()
        .map(|inline_tree| {
            let parent_range = inline_tree.parent_range.clone();
            let mut spans = collect_comrak_inline_spans(source, parent_range.clone());
            spans.extend(scan_entity_spans(source, parent_range.clone()));
            spans.sort_by_key(|span| (span.source_range.start, span.source_range.end));
            spans.dedup_by_key(|span| {
                (
                    span.kind,
                    span.source_range.start,
                    span.source_range.end,
                    span.url.clone(),
                )
            });
            collect_soft_break_spans(source, parent_range.clone(), &mut spans);

            let mut replacements = collect_comrak_projection_replacements(source, &spans);
            replacements.sort_by_key(|replacement| {
                (
                    replacement.source_range.start,
                    replacement.source_range.end,
                    replacement.owner_source_range.start,
                    replacement.owner_source_range.end,
                )
            });

            MarkdownInlineSemantics {
                parent_id: inline_tree.parent_id,
                parent_range,
                spans,
                replacements,
            }
        })
        .collect()
}

pub(super) fn collect_structure_inline_spans(
    structure: &MarkdownStructure,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = structure
        .inline_semantics()
        .iter()
        .flat_map(|semantics| semantics.spans.iter().cloned())
        .collect::<Vec<_>>();
    spans.sort_by_key(|span| (span.source_range.start, span.source_range.end));
    spans
}

pub(super) fn collect_incremental_inline_spans(
    previous: &MarkdownSyntaxTree,
    structure: &MarkdownStructure,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let previous_inline_parent_ranges = previous
        .parser_state
        .inline_trees()
        .iter()
        .map(|inline_tree| (inline_tree.parent_range.clone(), ()))
        .collect::<HashMap<_, _>>();
    for semantics in structure.inline_semantics() {
        let new_parent_range = &semantics.parent_range;
        if ranges_touch(new_parent_range, new_range) {
            spans.extend(semantics.spans.iter().cloned());
            continue;
        }

        let old_parent_range =
            old_range_for_clean_new_range(new_parent_range, new_range, old_range);
        if ranges_touch(&old_parent_range, old_range) {
            spans.extend(semantics.spans.iter().cloned());
            continue;
        }

        if !previous_inline_parent_ranges.contains_key(&old_parent_range) {
            spans.extend(semantics.spans.iter().cloned());
            continue;
        }

        let first_span_index = previous
            .data
            .inline_spans
            .partition_point(|span| span.source_range.start < old_parent_range.start);
        spans.extend(
            previous
                .data
                .inline_spans
                .get(first_span_index..)
                .unwrap_or_default()
                .iter()
                .take_while(|span| span.source_range.start < old_parent_range.end)
                .filter(|span| range_contains(&old_parent_range, &span.source_range))
                .cloned()
                .map(|span| shift_inline_span_after_edit(span, old_range, new_range)),
        );
    }
    spans.sort_by_key(|span| (span.source_range.start, span.source_range.end));
    spans
}

pub(super) fn collect_inline_spans_for_inline_tree(
    source: &str,
    inline_tree: &MarkdownInlineTree,
) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    collect_inline_span_nodes(source, inline_tree.tree().root_node(), &mut spans);
    collect_soft_break_spans(source, inline_tree.parent_range.clone(), &mut spans);
    spans
}

fn collect_soft_break_spans(
    source: &str,
    parent_range: Range<usize>,
    spans: &mut Vec<MarkdownInlineSpan>,
) {
    let mut cursor = parent_range.start;
    while cursor < parent_range.end {
        let Some(relative_newline) = source[cursor..parent_range.end].find('\n') else {
            break;
        };
        let newline = cursor + relative_newline;
        let next = newline + 1;
        if next >= parent_range.end {
            break;
        }

        let has_carriage_return =
            newline > parent_range.start && source.as_bytes()[newline - 1] == b'\r';
        let break_start = if has_carriage_return {
            newline - 1
        } else {
            newline
        };
        let break_range = break_start..next;
        let is_hard_break = spans.iter().any(|span| {
            span.kind == MarkdownInlineKind::HardBreak
                && ranges_overlap(&span.source_range, &break_range)
        });
        if !is_hard_break {
            spans.push(MarkdownInlineSpan {
                kind: MarkdownInlineKind::SoftBreak,
                source_range: break_range.clone(),
                content_ranges: vec![break_range],
                marker_ranges: Vec::new(),
                url: None,
                tagfilter_disallowed: false,
            });
        }

        cursor = next;
    }
}

pub(super) fn inline_span_prefix_maximum_ends(inline_spans: &[MarkdownInlineSpan]) -> Vec<usize> {
    let mut maximum_end = 0;
    inline_spans
        .iter()
        .map(|span| {
            maximum_end = maximum_end.max(span.source_range.end);
            maximum_end
        })
        .collect()
}

pub(super) fn collect_projection_replacements_for_blocks(
    source: &str,
    blocks: &[MarkdownBlock],
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = Vec::new();
    for block in blocks {
        if let Some(replacement) = task_list_marker_replacement_from_block(source, block) {
            replacements.push(replacement);
        }
    }
    replacements
}

pub(super) fn collect_structure_projection_replacements(
    source: &str,
    structure: &MarkdownStructure,
    blocks: &[MarkdownBlock],
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = collect_projection_replacements_for_blocks(source, blocks);
    replacements.extend(
        structure
            .inline_semantics()
            .iter()
            .flat_map(|semantics| semantics.replacements.iter().cloned()),
    );
    replacements.sort_by_key(|replacement| {
        (
            replacement.source_range.start,
            replacement.source_range.end,
            replacement.owner_source_range.start,
            replacement.owner_source_range.end,
        )
    });
    replacements
}

pub(super) fn collect_incremental_projection_replacements(
    source: &str,
    previous: &MarkdownSyntaxTree,
    structure: &MarkdownStructure,
    blocks: &[MarkdownBlock],
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = collect_projection_replacements_for_blocks(source, blocks);
    let previous_inline_parent_ranges = previous
        .parser_state
        .inline_trees()
        .iter()
        .map(|inline_tree| (inline_tree.parent_range.clone(), ()))
        .collect::<HashMap<_, _>>();
    for semantics in structure.inline_semantics() {
        let new_parent_range = &semantics.parent_range;
        if ranges_touch(new_parent_range, new_range) {
            replacements.extend(semantics.replacements.iter().cloned());
            continue;
        }

        let old_parent_range =
            old_range_for_clean_new_range(new_parent_range, new_range, old_range);
        if ranges_touch(&old_parent_range, old_range) {
            replacements.extend(semantics.replacements.iter().cloned());
            continue;
        }

        if !previous_inline_parent_ranges.contains_key(&old_parent_range) {
            replacements.extend(semantics.replacements.iter().cloned());
            continue;
        }

        let first_replacement_index = previous
            .data
            .projection_replacements
            .partition_point(|replacement| replacement.source_range.start < old_parent_range.start);
        replacements.extend(
            previous
                .data
                .projection_replacements
                .get(first_replacement_index..)
                .unwrap_or_default()
                .iter()
                .take_while(|replacement| replacement.source_range.start < old_parent_range.end)
                .filter(|replacement| {
                    range_contains(&old_parent_range, &replacement.owner_source_range)
                })
                .cloned()
                .map(|replacement| {
                    shift_projection_replacement_after_edit(replacement, old_range, new_range)
                }),
        );
    }
    replacements.sort_by_key(|replacement| {
        (
            replacement.source_range.start,
            replacement.source_range.end,
            replacement.owner_source_range.start,
            replacement.owner_source_range.end,
        )
    });
    replacements
}

pub(super) fn collect_projection_replacements_for_inline_tree(
    source: &str,
    inline_tree: &MarkdownInlineTree,
) -> Vec<MarkdownProjectionReplacement> {
    let mut replacements = Vec::new();
    collect_projection_replacement_nodes(source, inline_tree.tree().root_node(), &mut replacements);
    replacements
}

pub(super) fn projection_replacement_prefix_maximum_ends(
    replacements: &[MarkdownProjectionReplacement],
) -> Vec<usize> {
    let mut maximum_end = 0;
    replacements
        .iter()
        .map(|replacement| {
            maximum_end = maximum_end.max(replacement.source_range.end);
            maximum_end
        })
        .collect()
}

pub(super) fn projection_marker_dependencies(
    blocks: &[MarkdownBlock],
    inline_spans: &[MarkdownInlineSpan],
    replacements: &[MarkdownProjectionReplacement],
) -> Vec<ProjectionMarkerDependency> {
    let mut dependencies = Vec::new();
    for block in blocks {
        dependencies.extend(block.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: block.source_range.clone(),
            }
        }));
    }
    for span in inline_spans {
        if matches!(
            span.kind,
            MarkdownInlineKind::SoftBreak | MarkdownInlineKind::HardBreak
        ) {
            dependencies.push(ProjectionMarkerDependency {
                marker_range: span.source_range.clone(),
                owner_source_range: span.source_range.clone(),
            });
        }
        dependencies.extend(span.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: span.source_range.clone(),
            }
        }));
    }
    dependencies.extend(
        replacements
            .iter()
            .map(|replacement| ProjectionMarkerDependency {
                marker_range: replacement.source_range.clone(),
                owner_source_range: replacement.owner_source_range.clone(),
            }),
    );
    dependencies.sort_by_key(|dependency| {
        (
            dependency.marker_range.start,
            dependency.marker_range.end,
            dependency.owner_source_range.start,
            dependency.owner_source_range.end,
        )
    });
    dependencies
}

pub(super) fn projection_marker_prefix_maximum_ends(
    dependencies: &[ProjectionMarkerDependency],
) -> Vec<usize> {
    let mut maximum_end = 0;
    dependencies
        .iter()
        .map(|dependency| {
            maximum_end = maximum_end.max(dependency.marker_range.end);
            maximum_end
        })
        .collect()
}

fn shift_inline_span_after_edit(
    mut span: MarkdownInlineSpan,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> MarkdownInlineSpan {
    span.source_range = shift_clean_old_range_to_new(span.source_range, old_range, new_range);
    span.content_ranges = span
        .content_ranges
        .into_iter()
        .map(|range| shift_clean_old_range_to_new(range, old_range, new_range))
        .collect();
    span.marker_ranges = span
        .marker_ranges
        .into_iter()
        .map(|range| shift_clean_old_range_to_new(range, old_range, new_range))
        .collect();
    span
}

fn shift_projection_replacement_after_edit(
    mut replacement: MarkdownProjectionReplacement,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> MarkdownProjectionReplacement {
    replacement.source_range =
        shift_clean_old_range_to_new(replacement.source_range, old_range, new_range);
    replacement.owner_source_range =
        shift_clean_old_range_to_new(replacement.owner_source_range, old_range, new_range);
    replacement
}

fn collect_projection_replacement_nodes(
    source: &str,
    node: Node<'_>,
    replacements: &mut Vec<MarkdownProjectionReplacement>,
) {
    if let Some(replacement) = projection_replacement_from_node(source, node) {
        replacements.push(replacement);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_projection_replacement_nodes(source, child, replacements);
    }
}

fn projection_replacement_from_node(
    source: &str,
    node: Node<'_>,
) -> Option<MarkdownProjectionReplacement> {
    let source_range = node.byte_range();
    let display_text = match node.kind() {
        "backslash_escape" => source
            .get(source_range.start + 1..source_range.end)?
            .to_string(),
        "entity_reference" | "numeric_character_reference" => {
            decode_markdown_entity(source.get(source_range.clone())?)?
        }
        "task_list_marker_checked" => "\u{2611}".to_string(),
        "task_list_marker_unchecked" => "\u{2610}".to_string(),
        _ => return None,
    };

    Some(MarkdownProjectionReplacement {
        source_range: source_range.clone(),
        owner_source_range: source_range,
        display_text,
    })
}

fn task_list_marker_replacement_from_block(
    source: &str,
    block: &MarkdownBlock,
) -> Option<MarkdownProjectionReplacement> {
    let MarkdownBlockKind::TaskListItem { checked } = block.kind else {
        return None;
    };

    let source_range_start = block
        .marker_ranges
        .first()
        .map_or(block.source_range.start, |range| range.end);
    let source_range_end = source_range_start.checked_add(3)?;
    if source_range_end > block.source_range.end {
        return None;
    }

    let bytes = source.as_bytes();
    if bytes[source_range_start] != b'[' || bytes[source_range_start + 2] != b']' {
        return None;
    }

    let source_range = source_range_start..source_range_end;
    Some(MarkdownProjectionReplacement {
        source_range: source_range.clone(),
        owner_source_range: source_range,
        display_text: if checked {
            "\u{2611}".to_string()
        } else {
            "\u{2610}".to_string()
        },
    })
}

fn decode_markdown_entity(entity: &str) -> Option<String> {
    let entity_body = entity.strip_prefix('&')?.strip_suffix(';')?;
    if let Some(decimal) = entity_body.strip_prefix('#') {
        let codepoint = if let Some(hex) = decimal
            .strip_prefix('x')
            .or_else(|| decimal.strip_prefix('X'))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            decimal.parse::<u32>().ok()?
        };
        return char::from_u32(codepoint).map(|character| character.to_string());
    }

    decode_html5_named_character_reference(entity).map(str::to_string)
}

fn decode_html5_named_character_reference(entity: &str) -> Option<&'static str> {
    static NAMED_ENTITIES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    NAMED_ENTITIES
        .get_or_init(|| {
            entities::ENTITIES
                .iter()
                .filter(|entry| entry.entity.ends_with(';'))
                .map(|entry| (entry.entity, entry.characters))
                .collect()
        })
        .get(entity)
        .copied()
}

fn collect_inline_span_nodes(source: &str, node: Node<'_>, spans: &mut Vec<MarkdownInlineSpan>) {
    if let Some(span) = inline_span_from_node(source, node) {
        spans.push(span);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_inline_span_nodes(source, child, spans);
    }
}

fn inline_span_from_node(source: &str, node: Node<'_>) -> Option<MarkdownInlineSpan> {
    let kind = match node.kind() {
        "emphasis" => MarkdownInlineKind::Emphasis,
        "strong_emphasis" => MarkdownInlineKind::Strong,
        "code_span" => MarkdownInlineKind::InlineCode,
        "inline_link"
        | "full_reference_link"
        | "collapsed_reference_link"
        | "shortcut_link"
        | "uri_autolink"
        | "email_autolink" => MarkdownInlineKind::Link,
        "strikethrough" => MarkdownInlineKind::Strikethrough,
        "backslash_escape" => MarkdownInlineKind::Escape,
        "entity_reference" | "numeric_character_reference" => MarkdownInlineKind::Entity,
        "hard_line_break" => MarkdownInlineKind::HardBreak,
        "soft_line_break" => MarkdownInlineKind::SoftBreak,
        "html_tag" => MarkdownInlineKind::InlineHtml,
        "image" => MarkdownInlineKind::Image,
        "latex_block" => MarkdownInlineKind::InlineMath,
        _ => return None,
    };

    let source_range = node.byte_range();
    let marker_ranges = inline_marker_ranges(source, node);
    let content_ranges = inline_content_ranges(source_range.clone(), &marker_ranges);
    let url = match kind {
        MarkdownInlineKind::Image | MarkdownInlineKind::Link => extract_link_url(source, &node),
        _ => None,
    };
    let tagfilter_disallowed = kind == MarkdownInlineKind::InlineHtml
        && raw_html_tagfilter_disallowed(source, source_range.clone());

    Some(MarkdownInlineSpan {
        kind,
        source_range,
        content_ranges,
        marker_ranges,
        url,
        tagfilter_disallowed,
    })
}

pub(crate) fn raw_html_tagfilter_disallowed(source: &str, source_range: Range<usize>) -> bool {
    const DISALLOWED_TAGS: [&str; 9] = [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];

    let Some(text) = source.get(source_range) else {
        return false;
    };
    let bytes = text.as_bytes();
    let mut cursor = 0;

    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'<') {
        return false;
    }
    cursor += 1;
    if bytes.get(cursor) == Some(&b'/') {
        cursor += 1;
    }

    let tag_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_alphanumeric() {
        cursor += 1;
    }
    if cursor == tag_start {
        return false;
    }

    let tag_name = &text[tag_start..cursor];
    if !DISALLOWED_TAGS
        .iter()
        .any(|tag| tag_name.eq_ignore_ascii_case(tag))
    {
        return false;
    }

    matches!(
        bytes.get(cursor).copied(),
        None | Some(b'>') | Some(b'/') | Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
    )
}

fn inline_marker_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    if node.kind() == "latex_block" {
        return latex_block_marker_ranges(source, node);
    }
    if node.kind() == "html_tag" {
        return Vec::new();
    }

    let mut marker_ranges = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "latex_span_delimiter" => marker_ranges.push(child.byte_range()),
            "emphasis_delimiter"
            | "code_span_delimiter"
            | "link_destination"
            | "link_label"
            | "link_title" => marker_ranges.push(child.byte_range()),
            _ if !child.is_named() => marker_ranges.push(child.byte_range()),
            _ => {}
        }
    }
    marker_ranges.sort_by_key(|range| (range.start, range.end));
    marker_ranges
}

fn latex_block_marker_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    let source_range = node.byte_range();
    let delimiter_len = if source
        .get(source_range.clone())
        .is_some_and(|text| text.starts_with("$$") && text.ends_with("$$"))
    {
        2
    } else {
        1
    };

    let start = source_range.start;
    let end = source_range.end;
    let content_start = start.saturating_add(delimiter_len).min(end);
    let content_end = end.saturating_sub(delimiter_len).max(content_start);

    let mut marker_ranges = Vec::new();
    if start < content_start {
        marker_ranges.push(start..content_start);
    }
    if content_end < end {
        marker_ranges.push(content_end..end);
    }
    marker_ranges
}

fn extract_link_url(source: &str, node: &Node<'_>) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "link_destination" {
            let range = child.byte_range();
            return Some(source[range].to_string());
        }
    }
    None
}

fn inline_content_ranges(
    source_range: Range<usize>,
    marker_ranges: &[Range<usize>],
) -> Vec<Range<usize>> {
    let mut content_ranges = Vec::new();
    let mut start = source_range.start;
    for marker_range in marker_ranges {
        if start < marker_range.start {
            content_ranges.push(start..marker_range.start);
        }
        start = start.max(marker_range.end);
    }
    if start < source_range.end {
        content_ranges.push(start..source_range.end);
    }
    content_ranges
}

#[cfg(any(test, perf_enabled))]
fn collect_comrak_inline_spans(
    source: &str,
    parent_range: Range<usize>,
) -> Vec<MarkdownInlineSpan> {
    let Some(parent_source) = source.get(parent_range.clone()) else {
        return Vec::new();
    };
    let arena = Arena::new();
    let options = comrak_inline_options();
    let root = parse_document(&arena, parent_source, &options);
    let line_starts = super::source::line_starts(parent_source);
    let mut spans = Vec::new();
    collect_comrak_inline_span_nodes(source, parent_range.start, &line_starts, root, &mut spans);
    spans
}

#[cfg(any(test, perf_enabled))]
fn comrak_inline_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.math_dollars = true;
    options.extension.tagfilter = true;
    options.parse.escaped_char_spans = true;
    options.parse.sourcepos_chars = false;
    options
}

#[cfg(any(test, perf_enabled))]
fn collect_comrak_inline_span_nodes<'a>(
    source: &str,
    parent_start: usize,
    line_starts: &[usize],
    node: ComrakNode<'a>,
    spans: &mut Vec<MarkdownInlineSpan>,
) {
    if let Some(span) = comrak_inline_span_from_node(source, parent_start, line_starts, node) {
        spans.extend(comrak_synthetic_nested_inline_spans(source, &span));
        spans.push(span);
    }

    for child in node.children() {
        collect_comrak_inline_span_nodes(source, parent_start, line_starts, child, spans);
    }
}

#[cfg(any(test, perf_enabled))]
fn comrak_synthetic_nested_inline_spans(
    source: &str,
    span: &MarkdownInlineSpan,
) -> Vec<MarkdownInlineSpan> {
    if span.kind != MarkdownInlineKind::Strikethrough {
        return Vec::new();
    }
    let Some(text) = source.get(span.source_range.clone()) else {
        return Vec::new();
    };
    if !text.starts_with("~~") || !text.ends_with("~~") || text.len() < 4 {
        return Vec::new();
    }

    let source_range = span.source_range.start + 1..span.source_range.end - 1;
    let marker_ranges = delimiter_marker_ranges(source, source_range.clone(), &["~"]);
    let content_ranges = inline_content_ranges(source_range.clone(), &marker_ranges);
    vec![MarkdownInlineSpan {
        kind: MarkdownInlineKind::Strikethrough,
        source_range,
        content_ranges,
        marker_ranges,
        url: None,
        tagfilter_disallowed: false,
    }]
}

#[cfg(any(test, perf_enabled))]
fn comrak_inline_span_from_node<'a>(
    source: &str,
    parent_start: usize,
    line_starts: &[usize],
    node: ComrakNode<'a>,
) -> Option<MarkdownInlineSpan> {
    let data = node.data();
    let kind = match &data.value {
        NodeValue::Emph => MarkdownInlineKind::Emphasis,
        NodeValue::Strong => MarkdownInlineKind::Strong,
        NodeValue::Strikethrough => MarkdownInlineKind::Strikethrough,
        NodeValue::Code(_) => MarkdownInlineKind::InlineCode,
        NodeValue::Link(_) => MarkdownInlineKind::Link,
        NodeValue::Image(_) => MarkdownInlineKind::Image,
        NodeValue::HtmlInline(_) => MarkdownInlineKind::InlineHtml,
        NodeValue::Escaped => MarkdownInlineKind::Escape,
        NodeValue::Math(math) if math.dollar_math => MarkdownInlineKind::InlineMath,
        NodeValue::SoftBreak => return None,
        NodeValue::LineBreak => MarkdownInlineKind::HardBreak,
        _ => return None,
    };
    let source_range = record_timed_comrak_sourcepos_mapping(|| {
        source_range_from_comrak_sourcepos(parent_start, line_starts, data.sourcepos)
    })?;
    if source_range.is_empty() || source_range.end > source.len() {
        return None;
    }

    let marker_ranges = record_timed_comrak_marker_scan(|| {
        comrak_inline_marker_ranges(source, kind, source_range.clone())
    });
    let content_ranges = inline_content_ranges(source_range.clone(), &marker_ranges);
    let url = match &data.value {
        NodeValue::Link(link) => comrak_link_url(source, source_range.clone(), link.url.clone()),
        NodeValue::Image(link) => Some(link.url.clone()),
        _ => None,
    };
    let tagfilter_disallowed = kind == MarkdownInlineKind::InlineHtml
        && raw_html_tagfilter_disallowed(source, source_range.clone());

    Some(MarkdownInlineSpan {
        kind,
        source_range,
        content_ranges,
        marker_ranges,
        url,
        tagfilter_disallowed,
    })
}

#[cfg(any(test, perf_enabled))]
fn comrak_link_url(source: &str, source_range: Range<usize>, url: String) -> Option<String> {
    if source
        .get(source_range)
        .is_some_and(|text| text.starts_with('<') && text.ends_with('>'))
    {
        None
    } else {
        Some(url)
    }
}

#[cfg(any(test, perf_enabled))]
fn source_range_from_comrak_sourcepos(
    parent_start: usize,
    line_starts: &[usize],
    sourcepos: Sourcepos,
) -> Option<Range<usize>> {
    if sourcepos.start.line == 0 || sourcepos.end.line == 0 {
        return None;
    }
    let start_line = sourcepos.start.line.checked_sub(1)?;
    let end_line = sourcepos.end.line.checked_sub(1)?;
    let start_column = sourcepos.start.column.checked_sub(1)?;
    let end_column = sourcepos.end.column;
    let start = parent_start + line_starts.get(start_line).copied()? + start_column;
    let end = parent_start + line_starts.get(end_line).copied()? + end_column;
    (start <= end).then_some(start..end)
}

#[cfg(any(test, perf_enabled))]
fn comrak_inline_marker_ranges(
    source: &str,
    kind: MarkdownInlineKind,
    source_range: Range<usize>,
) -> Vec<Range<usize>> {
    match kind {
        MarkdownInlineKind::Emphasis => delimiter_marker_ranges(source, source_range, &["*", "_"]),
        MarkdownInlineKind::Strong => delimiter_marker_ranges(source, source_range, &["**", "__"]),
        MarkdownInlineKind::Strikethrough => delimiter_marker_ranges(source, source_range, &["~"]),
        MarkdownInlineKind::InlineCode => code_marker_ranges(source, source_range),
        MarkdownInlineKind::InlineMath => math_marker_ranges(source, source_range),
        MarkdownInlineKind::Link => link_marker_ranges(source, source_range, false),
        MarkdownInlineKind::Image => link_marker_ranges(source, source_range, true),
        MarkdownInlineKind::Escape
        | MarkdownInlineKind::HardBreak
        | MarkdownInlineKind::SoftBreak
        | MarkdownInlineKind::InlineHtml
        | MarkdownInlineKind::Entity => Vec::new(),
    }
}

#[cfg(any(test, perf_enabled))]
fn delimiter_marker_ranges(
    source: &str,
    source_range: Range<usize>,
    delimiters: &[&str],
) -> Vec<Range<usize>> {
    let Some(text) = source.get(source_range.clone()) else {
        return Vec::new();
    };
    for delimiter in delimiters {
        if text.starts_with(delimiter)
            && text.ends_with(delimiter)
            && text.len() >= delimiter.len() * 2
        {
            let start = source_range.start;
            let end = source_range.end;
            return (0..delimiter.len())
                .map(|index| start + index..start + index + 1)
                .chain((0..delimiter.len()).map(|index| {
                    let marker_start = end - delimiter.len() + index;
                    marker_start..marker_start + 1
                }))
                .collect();
        }
    }
    Vec::new()
}

#[cfg(any(test, perf_enabled))]
fn code_marker_ranges(source: &str, source_range: Range<usize>) -> Vec<Range<usize>> {
    let Some(text) = source.get(source_range.clone()) else {
        return Vec::new();
    };
    let marker_len = text.bytes().take_while(|byte| *byte == b'`').count();
    if marker_len == 0 || !text.ends_with(&"`".repeat(marker_len)) || text.len() < marker_len * 2 {
        return Vec::new();
    }
    vec![
        source_range.start..source_range.start + marker_len,
        source_range.end - marker_len..source_range.end,
    ]
}

#[cfg(any(test, perf_enabled))]
fn math_marker_ranges(source: &str, source_range: Range<usize>) -> Vec<Range<usize>> {
    let Some(text) = source.get(source_range.clone()) else {
        return Vec::new();
    };
    let marker_len = if text.starts_with("$$") && text.ends_with("$$") {
        2
    } else if text.starts_with('$') && text.ends_with('$') {
        1
    } else {
        return Vec::new();
    };
    vec![
        source_range.start..source_range.start + marker_len,
        source_range.end - marker_len..source_range.end,
    ]
}

#[cfg(any(test, perf_enabled))]
fn link_marker_ranges(source: &str, source_range: Range<usize>, image: bool) -> Vec<Range<usize>> {
    let Some(text) = source.get(source_range.clone()) else {
        return Vec::new();
    };
    let mut marker_ranges = Vec::new();
    if image && text.starts_with("![") {
        marker_ranges.push(source_range.start..source_range.start + 1);
        marker_ranges.push(source_range.start + 1..source_range.start + 2);
    } else if !image && text.starts_with('[') {
        marker_ranges.push(source_range.start..source_range.start + 1);
    }
    if let Some(label_end) = text.find("](") {
        let close_label = source_range.start + label_end;
        marker_ranges.push(close_label..close_label + 1);
        marker_ranges.push(close_label + 1..close_label + 2);
        if close_label + 2 < source_range.end.saturating_sub(1) {
            marker_ranges.push(close_label + 2..source_range.end - 1);
        }
        if source_range.end > 0 && text.ends_with(')') {
            marker_ranges.push(source_range.end - 1..source_range.end);
        }
    }
    marker_ranges
}

#[cfg(any(test, perf_enabled))]
fn scan_entity_spans(source: &str, parent_range: Range<usize>) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    let mut cursor = parent_range.start;
    while cursor < parent_range.end {
        let Some(relative_start) = source[cursor..parent_range.end].find('&') else {
            break;
        };
        let start = cursor + relative_start;
        let Some(relative_end) = source[start..parent_range.end].find(';') else {
            break;
        };
        let end = start + relative_end + 1;
        if decode_markdown_entity(&source[start..end]).is_some() {
            spans.push(MarkdownInlineSpan {
                kind: MarkdownInlineKind::Entity,
                source_range: start..end,
                content_ranges: vec![start..end],
                marker_ranges: Vec::new(),
                url: None,
                tagfilter_disallowed: false,
            });
        }
        cursor = end;
    }
    spans
}

#[cfg(any(test, perf_enabled))]
fn collect_comrak_projection_replacements(
    source: &str,
    spans: &[MarkdownInlineSpan],
) -> Vec<MarkdownProjectionReplacement> {
    spans
        .iter()
        .filter_map(|span| match span.kind {
            MarkdownInlineKind::Escape => Some(MarkdownProjectionReplacement {
                source_range: span.source_range.clone(),
                owner_source_range: span.source_range.clone(),
                display_text: source
                    .get(span.source_range.start + 1..span.source_range.end)?
                    .to_string(),
            }),
            MarkdownInlineKind::Entity => Some(MarkdownProjectionReplacement {
                source_range: span.source_range.clone(),
                owner_source_range: span.source_range.clone(),
                display_text: decode_markdown_entity(source.get(span.source_range.clone())?)?,
            }),
            _ => None,
        })
        .collect()
}
