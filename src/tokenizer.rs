// Tokenizer primitives and state machine helpers.
use memchr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Data,
    TagOpen,
    StartTagName,
    EndTagName,
    InsideStartTag,
    AttrName,
    AttrBeforeValue,
    AttrValue,
    CommentStart,
    CommentBody,
    CDataStart,
    CDataBody,
    PiStart,
    PiBody,
    SkipDoctype,
    ErrorRecovery,
    Eof,
}

impl Default for State {
    fn default() -> Self {
        State::Data
    }
}

/// Find the first occurrence of `needle` inside `haystack` and return the
/// byte index, or `None` if not found. Uses memchr for SIMD-accelerated search.
pub fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() == 1 {
        return memchr::memchr(needle[0], haystack);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    memchr::memmem::find(haystack, needle)
}

/// Scan a name from the start of `buf` returning the length of the name and
/// the delimiter byte if any (whitespace, '/', '>', '='). The caller must
/// supply the allowed maximum length; the returned length is clamped to it.
pub fn scan_name(buf: &[u8], max_len: usize) -> (usize, Option<u8>) {
    let mut len = 0usize;
    while len < buf.len() && len < max_len {
        let b = buf[len];
        match b {
            b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'=' => return (len, Some(b)),
            _ => {
                len += 1;
            }
        }
    }
    let delim = if len < buf.len() {
        Some(buf[len])
    } else {
        None
    };
    (len, delim)
}

/// Find the end of an XML comment (`-->`) starting anywhere in `haystack`.
pub fn find_comment_end(haystack: &[u8]) -> Option<usize> {
    find_sequence(haystack, b"-->")
}

/// Find the end of a CDATA section (`]]>`) starting anywhere in `haystack`.
pub fn find_cdata_end(haystack: &[u8]) -> Option<usize> {
    find_sequence(haystack, b"]]>")
}

/// Find the end of a processing instruction (`?>`) starting anywhere in `haystack`.
pub fn find_pi_end(haystack: &[u8]) -> Option<usize> {
    find_sequence(haystack, b"?>")
}

/// Scan an attribute value from the start of `buf`.
/// If the value begins with a quote (`'` or `"`), scan until the matching quote.
/// Otherwise scan until whitespace or `>` or `/`. The returned length is
/// clamped to `max_len`. The delimiter returned is the byte that terminated
/// the value (the closing quote, whitespace, `>`, `/`, or `=` if present), or
/// `None` when the buffer ended before a terminator was seen.
pub fn scan_attr_value(buf: &[u8], max_len: usize) -> (usize, Option<u8>) {
    if buf.is_empty() {
        return (0, None);
    }
    let first = buf[0];
    let mut len = 0usize;
    if first == b'\'' || first == b'\"' {
        // quoted value: include anything until matching quote
        len = 1; // start after opening quote
        while len < buf.len() && len < max_len {
            if buf[len] == first {
                return (len + 1, Some(first));
            }
            len += 1;
        }
        let delim = if len < buf.len() {
            Some(buf[len])
        } else {
            None
        };
        (len.min(max_len), delim)
    } else {
        // unquoted: stop at whitespace, '>' or '/'
        while len < buf.len() && len < max_len {
            let b = buf[len];
            match b {
                b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/' => return (len, Some(b)),
                _ => len += 1,
            }
        }
        let delim = if len < buf.len() {
            Some(buf[len])
        } else {
            None
        };
        (len, delim)
    }
}

/// Consume a comment starting at the beginning of `haystack`.
/// Returns the number of bytes consumed if a complete comment (`<!-- ... -->`) is
/// present, or `None` if the end sequence wasn't found.
pub fn consume_comment(haystack: &[u8]) -> Option<usize> {
    find_comment_end(haystack).map(|idx| idx + 3)
}

/// Consume a CDATA section starting at the beginning of `haystack`.
/// Returns the number of bytes consumed if a complete CDATA (`<![CDATA[...]]>`) is
/// present, or `None` if the end sequence wasn't found.
pub fn consume_cdata(haystack: &[u8]) -> Option<usize> {
    find_cdata_end(haystack).map(|idx| idx + 3)
}

/// Consume a processing instruction starting at the beginning of `haystack`.
/// Returns the number of bytes consumed if a complete PI (`<? ... ?>`) is
/// present, or `None` if the end sequence wasn't found.
pub fn consume_pi(haystack: &[u8]) -> Option<usize> {
    find_pi_end(haystack).map(|idx| idx + 2)
}

/// Check if a byte slice contains XML entity references (&...;).
pub fn contains_entities(data: &[u8]) -> bool {
    data.contains(&b'&')
}

/// A simple attribute parser for use in tokenizer unit tests.
/// Parses consecutive attribute `name=value` pairs from `buf` until a `>` or `/>`
/// is encountered or limits are reached. Returns the parsed attributes as a
/// vector of `(name, value)` pairs (both as owned `Vec<u8>`), the number of
/// bytes consumed from `buf`, and a boolean flag indicating whether any
/// truncation/limits were hit.
pub fn parse_attributes(
    mut buf: &[u8],
    max_name_len: usize,
    max_value_len: usize,
    max_attrs: usize,
) -> (Vec<(Vec<u8>, Vec<u8>)>, usize, bool, bool, bool) {
    let mut out = Vec::new();
    let mut consumed = 0usize;
    let mut hit_limit = false;
    let mut name_truncated_any = false;
    let mut value_truncated_any = false;

    // helper to skip ASCII whitespace
    fn skip_ws(s: &[u8]) -> (usize, &[u8]) {
        let mut i = 0usize;
        while i < s.len() {
            match s[i] {
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                _ => break,
            }
        }
        (i, &s[i..])
    }

    // main loop
    loop {
        let (sk, rest) = skip_ws(buf);
        consumed += sk;
        buf = rest;

        if buf.is_empty() {
            break;
        }
        // Stop if we reached the end of the start tag
        if buf[0] == b'>' {
            consumed += 1;
            break;
        }
        if buf.len() >= 2 && buf[0] == b'/' && buf[1] == b'>' {
            consumed += 2;
            break;
        }

        if out.len() >= max_attrs {
            hit_limit = true;
            break;
        }

        // parse name (copy up to max_name_len but consume the entire original name)
        let (name_len, _delim) = scan_name(buf, max_name_len);
        if name_len == 0 {
            break;
        }
        // If we hit the max_name_len, the real name may be longer; advance to the
        // real delimiter so remaining characters don't become a new attribute.
        let mut full_name_len = name_len;
        if name_len == max_name_len {
            while full_name_len < buf.len() {
                match buf[full_name_len] {
                    b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'=' => break,
                    _ => full_name_len += 1,
                }
            }
            if full_name_len > name_len {
                name_truncated_any = true;
            }
        }
        let name = buf[..name_len].to_vec();
        buf = &buf[full_name_len..];
        consumed += full_name_len;

        // skip whitespace
        let (sk2, rest2) = skip_ws(buf);
        consumed += sk2;
        buf = rest2;

        // expect '='
        if buf.is_empty() || buf[0] != b'=' {
            // no value; treat as empty and continue
            out.push((name, Vec::new()));
            continue;
        }
        // consume '='
        buf = &buf[1..];
        consumed += 1;
        let (sk3, rest3) = skip_ws(buf);
        consumed += sk3;
        buf = rest3;

        if buf.is_empty() {
            out.push((name, Vec::new()));
            break;
        }

        // scan value
        let (vlen, _vdelim) = scan_attr_value(buf, max_value_len);
        let mut value = Vec::new();
        if vlen > 0 {
            // If it's quoted, strip surrounding quotes when returning value
            let mut full_vlen = vlen;
            if buf[0] == b'\'' || buf[0] == b'"' {
                // If truncated, advance until matching closing quote so remainder
                // isn't parsed as further attributes.
                if vlen == max_value_len {
                    while full_vlen < buf.len() {
                        if buf[full_vlen] == buf[0] {
                            full_vlen += 1;
                            break;
                        }
                        full_vlen += 1;
                    }
                    if full_vlen > vlen {
                        value_truncated_any = true;
                    }
                }
                if vlen >= 2 {
                    value.extend_from_slice(&buf[1..vlen - 1]);
                }
            } else {
                // unquoted: if truncated, advance to next whitespace or '>' '/' delimiter
                if vlen == max_value_len {
                    while full_vlen < buf.len() {
                        match buf[full_vlen] {
                            b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/' => break,
                            _ => full_vlen += 1,
                        }
                    }
                    if full_vlen > vlen {
                        value_truncated_any = true;
                    }
                }
                value.extend_from_slice(&buf[..vlen]);
            }
            buf = &buf[full_vlen..];
            consumed += full_vlen;
        } else {
            // empty value
        }

        // append attribute
        out.push((name, value));

        // continue parsing
    }

    (
        out,
        consumed,
        hit_limit,
        name_truncated_any,
        value_truncated_any,
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_data() {
        assert_eq!(State::default(), State::Data);
    }

    #[test]
    fn find_sequence_basic() {
        let h = b"abc--def--ghi";
        assert_eq!(find_sequence(h, b"--"), Some(3));
        assert_eq!(find_sequence(h, b"ghi"), Some(10));
        assert_eq!(find_sequence(h, b"zzz"), None);
    }

    #[test]
    fn find_sequence_single_byte() {
        let h = b"hello";
        assert_eq!(find_sequence(h, b"e"), Some(1));
    }

    #[test]
    fn scan_name_stops_on_delim() {
        let buf = b"tagName attr=...";
        let (len, delim) = scan_name(buf, 100);
        assert_eq!(len, 7);
        assert_eq!(delim, Some(b' '));
    }

    #[test]
    fn scan_name_truncates_to_max() {
        let buf = b"longnamehere>";
        let (len, delim) = scan_name(buf, 4);
        assert_eq!(len, 4);
        assert_eq!(delim, Some(b'n'));
    }

    #[test]
    fn find_comment_end_basic() {
        let h = b"abc-->def";
        assert_eq!(find_comment_end(h), Some(3));
        assert_eq!(find_comment_end(b"no end here"), None);
    }

    #[test]
    fn find_cdata_end_basic() {
        let h = b"xyz]]>more";
        assert_eq!(find_cdata_end(h), Some(3));
    }

    #[test]
    fn find_pi_end_basic() {
        let h = b"pre?>post";
        assert_eq!(find_pi_end(h), Some(3));
    }

    #[test]
    fn scan_attr_value_quoted_and_unquoted() {
        let q = b"\"value\" rest";
        let (len, delim) = scan_attr_value(q, 100);
        assert_eq!(len, 7);
        assert_eq!(delim, Some(b'\"'));

        let s = b"unquoted>tail";
        let (len2, delim2) = scan_attr_value(s, 100);
        assert_eq!(len2, 8);
        assert_eq!(delim2, Some(b'>'));
    }

    #[test]
    fn scan_attr_value_truncates() {
        let q = b"\"abcdefghijkl"; // starts with '"'
        let (len, delim) = scan_attr_value(q, 5);
        assert_eq!(len, 5);
        // delim should be Some(next byte) because we truncated before closing quote
        assert!(delim.is_some());
    }

    #[test]
    fn parse_attributes_basic_and_self_closing() {
        let buf = b" a='one' b=two/>rest";
        let (attrs, consumed, hit, _name_trunc, _val_trunc) = parse_attributes(buf, 100, 100, 10);
        assert!(!hit);
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].0, b"a".to_vec());
        assert_eq!(attrs[0].1, b"one".to_vec());
        assert_eq!(attrs[1].0, b"b".to_vec());
        assert_eq!(attrs[1].1, b"two".to_vec());
        // consumed must be at least up to and including '/>'
        assert!(consumed >= (buf.len() - b"rest".len()));
    }

    #[test]
    fn parse_attributes_truncate_value_and_name() {
        let buf = b" longname=\"abcdefghijklmnop\" >";
        let (attrs, consumed, hit, _name_trunc, _val_trunc) = parse_attributes(buf, 6, 5, 10);
        assert!(!hit);
        assert_eq!(attrs.len(), 1);
        // name must be truncated to at most max 6
        assert!(attrs[0].0.len() <= 6 && attrs[0].0.len() > 0);
        assert!(attrs[0].0.starts_with(b"longn"));
        // value must be <= max_value_len
        assert!(attrs[0].1.len() <= 5);
        assert!(consumed > 0);
    }

    #[test]
    fn parse_attributes_max_attrs_limit() {
        let buf = b" a=1 b=2 c=3 >";
        let (attrs, _consumed, hit, _name_trunc, _val_trunc) = parse_attributes(buf, 100, 100, 2);
        assert!(hit);
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn parse_attributes_name_without_value() {
        let buf = b" a b='c' >";
        let (attrs, _consumed, hit, _name_trunc, _val_trunc) = parse_attributes(buf, 100, 100, 10);
        assert!(!hit);
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].0, b"a".to_vec());
        assert_eq!(attrs[0].1.len(), 0);
        assert_eq!(attrs[1].0, b"b".to_vec());
        assert_eq!(attrs[1].1, b"c".to_vec());
    }

    #[test]
    fn consume_incomplete_sequences_return_none() {
        assert_eq!(consume_comment(b"<!-- abc"), None);
        assert_eq!(consume_cdata(b"<![CDATA[foo"), None);
        assert_eq!(consume_pi(b"<?xml version"), None);
    }
}
