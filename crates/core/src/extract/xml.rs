//! Minimal XML plucker for UBL documents: no general parser (decision: #27's non-goal), just
//! enough to find one element's text (decoding CDATA and the five predefined entities) and an
//! attribute on its opening tag, anchored under a parent chain of local names (any namespace
//! prefix, or none). std only; every lookup is `.get`-based, so arbitrary bytes never panic.

/// One located element: its decoded text content, the byte span of its body (the opening
/// tag's end to the closing tag's start) in the scanned text, and its opening tag's raw
/// attribute text (queried via [`Element::attr`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Element {
    pub(crate) text: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
    attrs: String,
}

impl Element {
    /// The value of attribute `name` on this element's opening tag (`name="value"`), if
    /// present.
    pub(crate) fn attr(&self, name: &str) -> Option<String> {
        find_attr(&self.attrs, name)
    }
}

/// An opening tag found by [`find_open_tag`]: where its attribute text is, where it ends
/// (just past `>`), and whether it is self-closing (`<Foo/>`, no body, no closing tag).
struct OpenTag<'a> {
    attrs: &'a str,
    end: usize,
    self_closing: bool,
}

/// The local part of a (possibly prefixed) tag or attribute name: the text after the last
/// `:`, or the whole name when there is none.
fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Finds the next opening tag (any prefix, or none) whose local name is `local`, searching
/// `haystack` from byte offset `from`. Closing tags (`</...>`), processing instructions
/// (`<?...?>`), and markup declarations (`<!...>`, which covers CDATA sections and comments
/// so their content is never mistaken for a tag) are skipped as candidates, but their
/// contents are still plain text a later `find_open_tag` call can match a real tag inside.
fn find_open_tag<'a>(haystack: &'a str, from: usize, local: &str) -> Option<OpenTag<'a>> {
    let mut i = from;
    loop {
        let window = haystack.get(i..)?;
        let rel_lt = window.find('<')?;
        let tag_open = i + rel_lt;
        let next_byte = haystack.as_bytes().get(tag_open + 1).copied();
        if matches!(next_byte, Some(b'/') | Some(b'?') | Some(b'!')) {
            i = tag_open + 1;
            continue;
        }
        let rest = haystack.get(tag_open..)?;
        let rel_gt = rest.find('>')?;
        let tag_end = tag_open + rel_gt + 1;
        let inner_end = tag_end.checked_sub(1)?;
        let inner = haystack.get(tag_open + 1..inner_end)?;
        let trimmed = inner.trim_end();
        let self_closing = trimmed.ends_with('/');
        let core = if self_closing {
            trimmed.strip_suffix('/').unwrap_or(trimmed).trim_end()
        } else {
            inner
        };
        let name_end = core.find(char::is_whitespace).unwrap_or(core.len());
        let name = core.get(..name_end)?;
        if local_name(name) == local {
            let attrs = core.get(name_end..)?.trim();
            return Some(OpenTag {
                attrs,
                end: tag_end,
                self_closing,
            });
        }
        i = tag_end;
    }
}

/// Finds the first element matching `local` whose ancestor chain (root first) all appear,
/// nested, before it — searched linearly with no awareness of sibling scope, so a later
/// sibling's descendant can still match if no earlier one does (true of every field #27's
/// fixtures need; a general-purpose parser is explicitly out of scope). Returns the decoded
/// text content, its byte span, and the opening tag's attributes.
pub(crate) fn find_element(xml: &str, parents: &[&str], local: &str) -> Option<Element> {
    let mut pos = 0usize;
    for parent in parents {
        let tag = find_open_tag(xml, pos, parent)?;
        pos = tag.end;
    }
    let tag = find_open_tag(xml, pos, local)?;
    if tag.self_closing {
        return Some(Element {
            text: String::new(),
            start: tag.end,
            end: tag.end,
            attrs: tag.attrs.to_string(),
        });
    }
    let rest = xml.get(tag.end..)?;
    let mut cursor = 0usize;
    loop {
        let window = rest.get(cursor..)?;
        let rel = window.find("</")?;
        let abs = cursor + rel;
        let after = rest.get(abs + 2..)?;
        let rel_gt = after.find('>')?;
        let name = after.get(..rel_gt)?;
        if local_name(name) == local {
            let body = rest.get(..abs)?;
            return Some(Element {
                text: decode_text(body),
                start: tag.end,
                end: tag.end + abs,
                attrs: tag.attrs.to_string(),
            });
        }
        cursor = abs + 2 + rel_gt + 1;
    }
}

/// One attribute's value (`name="value"`) in a raw attribute-text slice (as captured by
/// [`find_open_tag`]).
fn find_attr(attrs: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = attrs.find(&needle)?.checked_add(needle.len())?;
    let rest = attrs.get(start..)?;
    let end = rest.find('"')?;
    rest.get(..end).map(str::to_string)
}

/// Trims an element body, unwraps a `<![CDATA[...]]>` wrapper when present (a missing `]]>`
/// suffix keeps the prefix-stripped text rather than failing), and decodes entities.
fn decode_text(raw: &str) -> String {
    let trimmed = raw.trim();
    let body = if let Some(stripped) = trimmed.strip_prefix("<![CDATA[") {
        stripped.strip_suffix("]]>").unwrap_or(stripped)
    } else {
        trimmed
    };
    decode_entities(body)
}

/// Decodes the five predefined XML entities (`&amp;`, `&lt;`, `&gt;`, `&apos;`, `&quot;`);
/// anything else between `&` and `;` (or an unterminated `&`) passes through unchanged.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut entity = String::new();
        let mut closed = false;
        for next in chars.by_ref() {
            if next == ';' {
                closed = true;
                break;
            }
            entity.push(next);
        }
        if !closed {
            out.push('&');
            out.push_str(&entity);
            continue;
        }
        match entity.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "apos" => out.push('\''),
            "quot" => out.push('"'),
            _ => {
                out.push('&');
                out.push_str(&entity);
                out.push(';');
            }
        }
    }
    out
}
