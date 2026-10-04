//! Minimal XML plucker: no general parser, just enough to pull one element's
//! text (handling CDATA and the five predefined entities) and one attribute
//! value, given a parent chain of local names. std only.

fn local_name(tag: &str) -> &str {
    tag.rsplit(':').next().unwrap_or(tag)
}

/// Find the start tag `<prefix:Name ...>` (any prefix, or none) for a given
/// local name, searching `haystack` from `from`. Returns (tag_start, tag_end,
/// attrs_slice) where `attrs_slice` is the text between the name and the
/// closing `>`/`/>`.
fn find_start_tag<'a>(haystack: &'a str, from: usize, local: &str) -> Option<(usize, usize)> {
    let mut i = from;
    let bytes = haystack.as_bytes();
    while let Some(lt) = haystack[i..].find('<') {
        let tag_open = i + lt;
        if bytes.get(tag_open + 1) == Some(&b'/') || bytes.get(tag_open + 1) == Some(&b'?') {
            i = tag_open + 1;
            continue;
        }
        let gt = haystack[tag_open..].find('>')?;
        let tag_end = tag_open + gt + 1;
        let inner = &haystack[tag_open + 1..tag_end - 1];
        let name_end = inner
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(inner.len());
        let name = &inner[..name_end];
        if local_name(name) == local {
            return Some((tag_open, tag_end));
        }
        i = tag_end;
    }
    None
}

/// Find the first element matching `local` whose ancestor chain (nearest
/// first is last in `parents`, i.e. `parents` is root..parent order) all
/// appear, nested, before it. Returns the decoded text content.
pub fn find_text(xml: &str, parents: &[&str], local: &str) -> Option<String> {
    let mut pos = 0usize;
    for p in parents {
        let (_, end) = find_start_tag(xml, pos, p)?;
        pos = end;
    }
    let (_, open_end) = find_start_tag(xml, pos, local)?;
    if xml.as_bytes()[open_end - 2] == b'/' {
        return Some(String::new()); // self-closing, no text
    }
    // Walk forward to find the first close tag whose local name matches
    // (no same-named nesting is assumed, true for every field this spike reads).
    let rest = &xml[open_end..];
    let mut cursor = 0usize;
    loop {
        let idx = rest[cursor..].find("</")?;
        let abs = cursor + idx;
        let gt = rest[abs..].find('>')?;
        let name = &rest[abs + 2..abs + gt];
        if local_name(name) == local {
            return Some(decode_text(&rest[..abs]));
        }
        cursor = abs + gt + 1;
    }
}

/// Find one attribute's value on the first element matching `local` (no
/// parent-chain search; used for e.g. `currencyID` on an amount element).
pub fn find_attr(xml: &str, local: &str, attr: &str) -> Option<String> {
    let (tag_open, tag_end) = find_start_tag(xml, 0, local)?;
    let inner = &xml[tag_open..tag_end];
    let needle = format!("{attr}=\"");
    let start = inner.find(&needle)? + needle.len();
    let end = inner[start..].find('"')? + start;
    Some(inner[start..end].to_string())
}

fn decode_text(raw: &str) -> String {
    let trimmed = raw.trim();
    let body = if let Some(stripped) = trimmed.strip_prefix("<![CDATA[") {
        stripped.strip_suffix("]]>").unwrap_or(stripped)
    } else {
        trimmed
    };
    decode_entities(body)
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut ent = String::new();
        let mut closed = false;
        for nc in chars.by_ref() {
            if nc == ';' {
                closed = true;
                break;
            }
            ent.push(nc);
        }
        if !closed {
            out.push('&');
            out.push_str(&ent);
            continue;
        }
        match ent.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "apos" => out.push('\''),
            "quot" => out.push('"'),
            _ => {
                out.push('&');
                out.push_str(&ent);
                out.push(';');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<Invoice><cac:AccountingSupplierParty><cac:Party><cac:PartyName><cbc:Name>Acme &amp; Sons</cbc:Name></cac:PartyName></cac:Party></cac:AccountingSupplierParty></Invoice>"#;

    #[test]
    fn plucks_nested_text_and_decodes_entity() {
        let got = find_text(
            FIXTURE,
            &["AccountingSupplierParty", "Party", "PartyName"],
            "Name",
        );
        assert_eq!(got, Some("Acme & Sons".to_string()));
    }

    #[test]
    fn missing_element_is_none() {
        assert_eq!(find_text(FIXTURE, &["Nope"], "Name"), None);
    }
}
