//! Read-only preview pane content.
//!
//! Preview panes carry a plain text or minimal-Markdown payload on their
//! [`crate::terminal::TerminalState`] (`TerminalState::preview`). They never
//! spawn a PTY or a child process; the parsed lines below are the entire
//! "surface" a preview exposes to the renderer.

use crate::api::schema::{PreviewContentType, PREVIEW_CONTENT_MAX_BYTES};

/// One span of styled text inside a rendered preview line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreviewSpan {
    pub(crate) text: String,
    pub(crate) style: PreviewSpanStyle,
}

/// Reduced style vocabulary for minimal markdown rendering. The renderer maps
/// these onto the terminal theme; `Heading` is carried on the line so the
/// whole line can be styled regardless of spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewSpanStyle {
    Text,
    Strong,
    Emphasis,
    Code,
}

/// A single renderable line produced from preview content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreviewLine {
    /// Markdown heading level (1-6) when the line is a heading.
    pub(crate) heading: Option<u8>,
    /// Ruler/horizontal-rule lines keep a single muted span.
    pub(crate) is_rule: bool,
    pub(crate) spans: Vec<PreviewSpan>,
}

/// The parsed content of a preview pane.
#[derive(Debug, Clone)]
pub struct PreviewState {
    pub content: String,
    pub content_type: PreviewContentType,
    pub(crate) lines: Vec<PreviewLine>,
    /// Lines hidden below the bottom edge of the viewport. `None` (the
    /// default) pins the view to the top of the content; `Some(0)` is the
    /// bottom. Set via `pane.scroll`/`preview.set` keeps the pinned edge.
    scroll_from_bottom: Option<usize>,
}

impl PreviewState {
    pub fn new(content: String, content_type: PreviewContentType) -> Self {
        let lines = render_preview_lines(&content, content_type);
        Self {
            content,
            content_type,
            lines,
            scroll_from_bottom: None,
        }
    }

    /// Replaces the payload and re-parses, preserving the scroll pin.
    pub fn set_content(&mut self, content: String, content_type: PreviewContentType) {
        self.lines = render_preview_lines(&content, content_type);
        self.content = content;
        self.content_type = content_type;
    }

    /// Implements `pane.scroll`: `offset_from_bottom` is the number of lines
    /// hidden below the viewport (`0` = bottom, `None` = top).
    pub(crate) fn set_scroll_from_bottom(&mut self, offset_from_bottom: Option<u64>) {
        self.scroll_from_bottom = offset_from_bottom.map(|value| value as usize);
    }

    /// Scroll metrics matching `TerminalRuntime::scroll_metrics` semantics so
    /// `pane.info` can report them through the same `PaneScrollInfo` shape.
    /// `viewport_rows` is the pane's content height when on screen (0 when the
    /// pane is on a hidden tab/surface).
    pub(crate) fn scroll_metrics(
        &self,
        viewport_rows: usize,
    ) -> crate::api::schema::PaneScrollInfo {
        let max = self.lines.len().saturating_sub(viewport_rows);
        crate::api::schema::PaneScrollInfo {
            offset_from_bottom: self.scroll_from_bottom.unwrap_or(max).min(max) as u64,
            max_offset_from_bottom: max as u64,
            viewport_rows: viewport_rows as u64,
        }
    }

    /// The line range to draw for a viewport `height` rows tall.
    /// Returns `(first_line_index, offset_from_bottom)`.
    pub(crate) fn scroll_window(&self, height: usize) -> (usize, usize) {
        let total = self.lines.len();
        let height = height.min(total);
        let offset = self
            .scroll_from_bottom
            .unwrap_or(usize::MAX)
            .min(total.saturating_sub(height));
        let end = total - offset;
        (end - height, offset)
    }
}

/// Public so `preview.open`/`preview.set` can enforce the limit before
/// allocating a `PreviewState`.
pub(crate) fn validate_preview_content_size(content: &str) -> Result<(), ()> {
    (content.len() <= PREVIEW_CONTENT_MAX_BYTES)
        .then_some(())
        .ok_or(())
}

fn render_preview_lines(content: &str, content_type: PreviewContentType) -> Vec<PreviewLine> {
    match content_type {
        PreviewContentType::Text => render_text_lines(content),
        PreviewContentType::Markdown => render_markdown_lines(content),
    }
}

fn render_text_lines(content: &str) -> Vec<PreviewLine> {
    content
        .lines()
        .map(|line| PreviewLine {
            heading: None,
            is_rule: false,
            spans: vec![PreviewSpan {
                text: line.to_string(),
                style: PreviewSpanStyle::Text,
            }],
        })
        .collect()
}

fn render_markdown_lines(content: &str) -> Vec<PreviewLine> {
    let mut lines = Vec::new();
    let mut in_fence = false;
    for raw in content.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            lines.push(PreviewLine {
                heading: None,
                is_rule: false,
                spans: vec![PreviewSpan {
                    text: line.to_string(),
                    style: PreviewSpanStyle::Code,
                }],
            });
            continue;
        }
        lines.push(render_markdown_line(line));
    }
    lines
}

fn render_markdown_line(line: &str) -> PreviewLine {
    let trimmed = line.trim_start();
    if let Some(rest) = heading_rest(trimmed) {
        let level = trimmed.len() - rest.len() - 1; // count of '#'
        return PreviewLine {
            heading: Some(level as u8),
            is_rule: false,
            spans: vec![PreviewSpan {
                text: rest.trim_start().to_string(),
                style: PreviewSpanStyle::Text,
            }],
        };
    }
    if is_rule_line(trimmed) {
        return PreviewLine {
            heading: None,
            is_rule: true,
            spans: vec![PreviewSpan {
                text: String::new(),
                style: PreviewSpanStyle::Text,
            }],
        };
    }
    let (indent, body) = list_item_rest(trimmed).unwrap_or(("", trimmed));
    let mut spans = Vec::new();
    if !indent.is_empty() {
        spans.push(PreviewSpan {
            text: indent.to_string(),
            style: PreviewSpanStyle::Text,
        });
    }
    spans.extend(parse_inline_spans(body, PreviewSpanStyle::Text));
    PreviewLine {
        heading: None,
        is_rule: false,
        spans,
    }
}

/// `## title` → `Some("title")` (after the leading marker and space).
fn heading_rest(trimmed: &str) -> Option<&str> {
    let level = trimmed.bytes().take_while(|b| *b == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    trimmed[level..]
        .strip_prefix(' ')
        .or_else(|| trimmed[level..].is_empty().then_some(""))
}

fn is_rule_line(trimmed: &str) -> bool {
    let chars = trimmed.chars().filter(|ch| *ch != ' ').collect::<String>();
    chars.len() >= 3
        && (chars.chars().all(|ch| ch == '-')
            || chars.chars().all(|ch| ch == '*')
            || chars.chars().all(|ch| ch == '_'))
}

/// `- item` / `* item` / `1. item` / `> quote` → `(marker_prefix, body)`.
fn list_item_rest(line: &str) -> Option<(&str, &str)> {
    if let Some(rest) = line.strip_prefix("> ") {
        return Some(("> ", rest));
    }
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some((&line[..2], rest));
    }
    let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 {
        if let Some(rest) = line[digits..].strip_prefix(". ") {
            return Some((&line[..digits + 2], rest));
        }
    }
    None
}

/// Parses `**bold**`, `*italic*`/`_italic_`, `` `code` `` into styled spans.
/// Unmatched markers render literally; links and images keep their raw text.
fn parse_inline_spans(text: &str, base: PreviewSpanStyle) -> Vec<PreviewSpan> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let marker = find_next_marker(rest);
        let Some((marker_index, marker_len, style)) = marker else {
            push_span(&mut spans, rest, base);
            break;
        };
        if marker_index > 0 {
            push_span(&mut spans, &rest[..marker_index], base);
        }
        let after = &rest[marker_index + marker_len..];
        let Some(close) = find_marker(after, &rest[marker_index..marker_index + marker_len]) else {
            push_span(
                &mut spans,
                &rest[marker_index..marker_index + marker_len],
                base,
            );
            rest = after;
            continue;
        };
        let (inner, tail) = after.split_at(close);
        push_span(&mut spans, inner, style);
        rest = &tail[marker_len..];
    }
    if spans.is_empty() {
        spans.push(PreviewSpan {
            text: String::new(),
            style: base,
        });
    }
    spans
}

fn push_span(spans: &mut Vec<PreviewSpan>, text: &str, style: PreviewSpanStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    spans.push(PreviewSpan {
        text: text.to_string(),
        style,
    });
}

/// Finds the earliest marker in `rest`; prefers `**` over `*` at the same spot.
fn find_next_marker(rest: &str) -> Option<(usize, usize, PreviewSpanStyle)> {
    let mut best: Option<(usize, usize, PreviewSpanStyle)> = None;
    let consider = |best: &mut Option<(usize, usize, PreviewSpanStyle)>,
                    index: usize,
                    len: usize,
                    style: PreviewSpanStyle| {
        if best.is_none_or(|(i, _, _)| index < i) {
            *best = Some((index, len, style));
        }
    };
    if let Some(index) = rest.find("**") {
        consider(&mut best, index, 2, PreviewSpanStyle::Strong);
    }
    if let Some(index) = rest.find('`') {
        consider(&mut best, index, 1, PreviewSpanStyle::Code);
    }
    // Single-emphasis markers must not double-count the `**` we may have found.
    let strong_pos = best
        .filter(|(_, len, style)| *len == 2 && *style == PreviewSpanStyle::Strong)
        .map(|(i, _, _)| i);
    for (index, _) in rest.match_indices('*') {
        if Some(index) == strong_pos || Some(index) == strong_pos.map(|p| p + 1) {
            continue;
        }
        consider(&mut best, index, 1, PreviewSpanStyle::Emphasis);
        break;
    }
    for (index, _) in rest.match_indices('_') {
        // `_` mid-word is literal text, not emphasis.
        let prev_is_word = rest[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric());
        let next_is_word = rest[index + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric());
        if prev_is_word && next_is_word {
            continue;
        }
        consider(&mut best, index, 1, PreviewSpanStyle::Emphasis);
        break;
    }
    best
}

fn find_marker(after: &str, marker: &str) -> Option<usize> {
    if marker == "_" {
        // `_` closing marker must not sit mid-word either (`foo_bar_`).
        for (index, _) in after.match_indices('_') {
            let prev_is_word = after[..index]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric());
            let next_is_word = after[index + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric());
            if prev_is_word && next_is_word {
                continue;
            }
            return Some(index);
        }
        None
    } else {
        after.find(marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styles(line: &PreviewLine) -> Vec<(&str, PreviewSpanStyle)> {
        line.spans
            .iter()
            .map(|span| (span.text.as_str(), span.style))
            .collect()
    }

    #[test]
    fn text_content_renders_verbatim_lines() {
        let state = PreviewState::new("a\n**not bold**\n".into(), PreviewContentType::Text);
        assert_eq!(state.lines.len(), 2);
        assert_eq!(styles(&state.lines[1])[0].0, "**not bold**");
    }

    #[test]
    fn markdown_headings_lists_and_rules_render() {
        let state = PreviewState::new(
            "# Title\n\n- one `x`\n* two\n1. three\n> quote\n---\ntail".to_string(),
            PreviewContentType::Markdown,
        );
        assert_eq!(state.lines[0].heading, Some(1));
        assert_eq!(state.lines[0].spans[0].text, "Title");
        let bullet = styles(&state.lines[2]);
        assert_eq!(bullet[0].0, "- ");
        assert_eq!(bullet[2], ("x", PreviewSpanStyle::Code));
        let quote = styles(&state.lines[5]);
        assert_eq!(quote[0].0, "> ");
        assert!(state.lines[6].is_rule);
        assert_eq!(state.lines[7].spans[0].text, "tail");
    }

    #[test]
    fn markdown_inline_emphasis_and_code() {
        let state = PreviewState::new(
            "pre **strong** and *em* and _em2_ and `co` post".to_string(),
            PreviewContentType::Markdown,
        );
        let spans = styles(&state.lines[0]);
        assert_eq!(
            spans,
            vec![
                ("pre ", PreviewSpanStyle::Text),
                ("strong", PreviewSpanStyle::Strong),
                (" and ", PreviewSpanStyle::Text),
                ("em", PreviewSpanStyle::Emphasis),
                (" and ", PreviewSpanStyle::Text),
                ("em2", PreviewSpanStyle::Emphasis),
                (" and ", PreviewSpanStyle::Text),
                ("co", PreviewSpanStyle::Code),
                (" post", PreviewSpanStyle::Text),
            ]
        );
    }

    #[test]
    fn markdown_fence_renders_code_lines_and_skips_markers() {
        let state = PreviewState::new(
            "```rust\nfn main() {}\n```\nafter".to_string(),
            PreviewContentType::Markdown,
        );
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0].spans[0].style, PreviewSpanStyle::Code);
        assert_eq!(state.lines[0].spans[0].text, "fn main() {}");
        assert_eq!(state.lines[1].spans[0].text, "after");
    }

    #[test]
    fn unclosed_markers_render_literally() {
        let state = PreviewState::new("a **b c".to_string(), PreviewContentType::Markdown);
        assert_eq!(state.lines[0].spans[0].text, "a **b c");
    }

    #[test]
    fn mid_word_underscore_is_literal() {
        let state = PreviewState::new("foo_bar_baz".to_string(), PreviewContentType::Markdown);
        assert_eq!(state.lines[0].spans[0].text, "foo_bar_baz");
        assert_eq!(state.lines[0].spans[0].style, PreviewSpanStyle::Text);
    }

    #[test]
    fn scroll_window_pins_top_by_default() {
        let mut state = PreviewState::new(
            (1..=10)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            PreviewContentType::Text,
        );
        assert_eq!(state.scroll_window(4), (0, 6));
        state.set_scroll_from_bottom(Some(0));
        assert_eq!(state.scroll_window(4), (6, 0));
        state.set_scroll_from_bottom(Some(2));
        assert_eq!(state.scroll_window(4), (4, 2));
        state.set_scroll_from_bottom(None);
        assert_eq!(state.scroll_window(4), (0, 6));
    }

    #[test]
    fn content_size_limit_is_enforced() {
        let ok = "x".repeat(PREVIEW_CONTENT_MAX_BYTES);
        assert!(validate_preview_content_size(&ok).is_ok());
        let too_big = "x".repeat(PREVIEW_CONTENT_MAX_BYTES + 1);
        assert!(validate_preview_content_size(&too_big).is_err());
    }
}
