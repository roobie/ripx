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
}
