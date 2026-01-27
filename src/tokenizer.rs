// Tokenizer primitives and state machine helpers.

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

impl Default for State { fn default() -> Self { State::Data } }

/// Find the first occurrence of `needle` inside `haystack` and return the
/// byte index, or `None` if not found. Simple non-optimized implementation
/// suitable for tests and initial tokenizer logic.
pub fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() { return Some(0); }
    if needle.len() == 1 {
        return haystack.iter().position(|&b| b == needle[0]);
    }
    if needle.len() > haystack.len() { return None; }
    haystack.windows(needle.len()).position(|w| w == needle)
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
            _ => { len += 1; }
        }
    }
    let delim = if len < buf.len() { Some(buf[len]) } else { None };
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
    if buf.is_empty() { return (0, None); }
    let first = buf[0];
    let mut len = 0usize;
    if first == b'\'' || first == b'\"' {
        // quoted value: include anything until matching quote
        len = 1; // start after opening quote
        while len < buf.len() && len < max_len {
            if buf[len] == first { return (len + 1, Some(first)); }
            len += 1;
        }
        let delim = if len < buf.len() { Some(buf[len]) } else { None };
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
        let delim = if len < buf.len() { Some(buf[len]) } else { None };
        (len, delim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_data() { assert_eq!(State::default(), State::Data); }

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
}
