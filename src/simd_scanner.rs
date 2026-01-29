//! SIMD-accelerated XML scanning operations.
//!
//! This module provides SIMD-optimized variants of common XML scanning operations
//! using AVX2 and SSE4.2 instructions when available, with automatic fallback to
//! scalar implementations.

use memchr;

/// SIMD-accelerated scanner for XML patterns.
pub struct SimdScanner;

impl SimdScanner {
    /// Find the first occurrence of '<', '>', or '&' (XML structural characters).
    ///
    /// This is faster than sequential scanning for long text runs.
    /// Uses SIMD when available, falls back to memchr.
    #[inline]
    pub fn find_xml_structural_char(data: &[u8]) -> Option<usize> {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        unsafe {
            Self::find_xml_structural_char_avx2(data)
        }

        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        {
            // Fallback: use memchr to find any of '<', '>', '&'
            memchr::memchr3(b'<', b'>', b'&', data)
        }
    }

    /// AVX2 implementation for finding XML structural characters.
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    #[target_feature(enable = "avx2")]
    unsafe fn find_xml_structural_char_avx2(data: &[u8]) -> Option<usize> {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::*;

            let len = data.len();
            let mut offset = 0;

            // Process 32 bytes at a time with AVX2
            while offset + 32 <= len {
                let chunk = _mm256_loadu_si256(data.as_ptr().add(offset) as *const __m256i);

                // Compare with '<', '>', '&'
                let lt = _mm256_set1_epi8(b'<' as i8);
                let gt = _mm256_set1_epi8(b'>' as i8);
                let amp = _mm256_set1_epi8(b'&' as i8);

                let cmp_lt = _mm256_cmpeq_epi8(chunk, lt);
                let cmp_gt = _mm256_cmpeq_epi8(chunk, gt);
                let cmp_amp = _mm256_cmpeq_epi8(chunk, amp);

                // OR all comparisons together
                let matches = _mm256_or_si256(_mm256_or_si256(cmp_lt, cmp_gt), cmp_amp);

                // Check if any matches
                let mask = _mm256_movemask_epi8(matches);
                if mask != 0 {
                    // Found a match - return position of first set bit
                    return Some(offset + mask.trailing_zeros() as usize);
                }

                offset += 32;
            }

            // Handle remaining bytes with scalar search
            for i in offset..len {
                match data[i] {
                    b'<' | b'>' | b'&' => return Some(i),
                    _ => {}
                }
            }

            None
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            memchr::memchr(b'<', data)
        }
    }

    /// Check if a byte range contains only valid XML name characters.
    ///
    /// This is useful for validating element/attribute names quickly.
    #[inline]
    pub fn is_valid_name(data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }

        // First character: letter, underscore, or colon
        if !Self::is_name_start_char(data[0]) {
            return false;
        }

        // Remaining characters: name chars
        data[1..].iter().all(|&b| Self::is_name_char(b))
    }

    /// Check if byte is valid as first character of XML name.
    #[inline]
    fn is_name_start_char(b: u8) -> bool {
        matches!(b,
            b'A'..=b'Z' | b'a'..=b'z' | b'_' | b':'
        )
    }

    /// Check if byte is valid in XML name (after first character).
    #[inline]
    fn is_name_char(b: u8) -> bool {
        matches!(b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b':' | b'-' | b'.'
        )
    }

    /// Fast path for finding '>' character (end of tag).
    ///
    /// This is heavily optimized since it's called for every tag.
    #[inline]
    pub fn find_tag_close(data: &[u8]) -> Option<usize> {
        memchr::memchr(b'>', data)
    }

    /// Fast path for finding '=' character (in attributes).
    #[inline]
    pub fn find_equals(data: &[u8]) -> Option<usize> {
        memchr::memchr(b'=', data)
    }

    /// Fast path for finding quote character (attribute value delimiter).
    #[inline]
    pub fn find_quote(data: &[u8], quote_char: u8) -> Option<usize> {
        memchr::memchr(quote_char, data)
    }

    /// Find whitespace character (space, tab, newline, carriage return).
    #[inline]
    pub fn find_whitespace(data: &[u8]) -> Option<usize> {
        data.iter().position(|&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
    }

    /// Skip whitespace and return the number of bytes skipped.
    #[inline]
    pub fn skip_whitespace(data: &[u8]) -> usize {
        data.iter()
            .position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
            .unwrap_or(data.len())
    }
}

/// Lookup table for fast character classification.
///
/// This can be used as an alternative to match statements for better performance.
pub struct CharLookup {
    is_whitespace: [bool; 256],
    is_name_char: [bool; 256],
    is_delimiter: [bool; 256],
}

impl CharLookup {
    /// Create a new character lookup table.
    pub const fn new() -> Self {
        let mut is_whitespace = [false; 256];
        is_whitespace[b' ' as usize] = true;
        is_whitespace[b'\t' as usize] = true;
        is_whitespace[b'\n' as usize] = true;
        is_whitespace[b'\r' as usize] = true;

        let mut is_name_char = [false; 256];
        let mut i = b'A';
        while i <= b'Z' {
            is_name_char[i as usize] = true;
            i += 1;
        }
        i = b'a';
        while i <= b'z' {
            is_name_char[i as usize] = true;
            i += 1;
        }
        i = b'0';
        while i <= b'9' {
            is_name_char[i as usize] = true;
            i += 1;
        }
        is_name_char[b'_' as usize] = true;
        is_name_char[b':' as usize] = true;
        is_name_char[b'-' as usize] = true;
        is_name_char[b'.' as usize] = true;

        let mut is_delimiter = [false; 256];
        is_delimiter[b' ' as usize] = true;
        is_delimiter[b'\t' as usize] = true;
        is_delimiter[b'\n' as usize] = true;
        is_delimiter[b'\r' as usize] = true;
        is_delimiter[b'>' as usize] = true;
        is_delimiter[b'/' as usize] = true;
        is_delimiter[b'=' as usize] = true;

        Self {
            is_whitespace,
            is_name_char,
            is_delimiter,
        }
    }

    /// Check if character is whitespace.
    #[inline]
    pub fn is_whitespace(&self, b: u8) -> bool {
        self.is_whitespace[b as usize]
    }

    /// Check if character is valid in XML name.
    #[inline]
    pub fn is_name_char(&self, b: u8) -> bool {
        self.is_name_char[b as usize]
    }

    /// Check if character is a delimiter (ends a token).
    #[inline]
    pub fn is_delimiter(&self, b: u8) -> bool {
        self.is_delimiter[b as usize]
    }
}

impl Default for CharLookup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_xml_structural_char() {
        let data = b"hello world<tag>";
        assert_eq!(SimdScanner::find_xml_structural_char(data), Some(11));

        let data = b"text>more";
        assert_eq!(SimdScanner::find_xml_structural_char(data), Some(4));

        let data = b"entity&ref";
        assert_eq!(SimdScanner::find_xml_structural_char(data), Some(6));

        let data = b"no special chars";
        assert_eq!(SimdScanner::find_xml_structural_char(data), None);
    }

    #[test]
    fn test_is_valid_name() {
        assert!(SimdScanner::is_valid_name(b"element"));
        assert!(SimdScanner::is_valid_name(b"my-element"));
        assert!(SimdScanner::is_valid_name(b"my_element"));
        assert!(SimdScanner::is_valid_name(b"element123"));
        assert!(SimdScanner::is_valid_name(b"ns:element"));

        assert!(!SimdScanner::is_valid_name(b""));
        assert!(!SimdScanner::is_valid_name(b"123element"));
        assert!(!SimdScanner::is_valid_name(b"-element"));
        assert!(!SimdScanner::is_valid_name(b"ele ment"));
    }

    #[test]
    fn test_find_tag_close() {
        let data = b"<tag attr=\"value\">";
        assert_eq!(SimdScanner::find_tag_close(data), Some(17));

        let data = b"no closing bracket";
        assert_eq!(SimdScanner::find_tag_close(data), None);
    }

    #[test]
    fn test_skip_whitespace() {
        assert_eq!(SimdScanner::skip_whitespace(b"   text"), 3);
        assert_eq!(SimdScanner::skip_whitespace(b"\t\n\r text"), 4);
        assert_eq!(SimdScanner::skip_whitespace(b"text"), 0);
        assert_eq!(SimdScanner::skip_whitespace(b"   "), 3);
    }

    #[test]
    fn test_char_lookup() {
        let lookup = CharLookup::new();

        assert!(lookup.is_whitespace(b' '));
        assert!(lookup.is_whitespace(b'\t'));
        assert!(lookup.is_whitespace(b'\n'));
        assert!(!lookup.is_whitespace(b'a'));

        assert!(lookup.is_name_char(b'a'));
        assert!(lookup.is_name_char(b'Z'));
        assert!(lookup.is_name_char(b'0'));
        assert!(lookup.is_name_char(b'_'));
        assert!(!lookup.is_name_char(b' '));

        assert!(lookup.is_delimiter(b' '));
        assert!(lookup.is_delimiter(b'>'));
        assert!(lookup.is_delimiter(b'/'));
        assert!(!lookup.is_delimiter(b'a'));
    }

    #[test]
    fn test_find_whitespace() {
        let data = b"text more";
        assert_eq!(SimdScanner::find_whitespace(data), Some(4));

        let data = b"nowhitespace";
        assert_eq!(SimdScanner::find_whitespace(data), None);
    }
}
