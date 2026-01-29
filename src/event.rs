//! Zero-copy event structures for high-performance parsing.
//!
//! These event types reference memory-mapped data directly, avoiding the need
//! to copy element names, attribute names/values, and text content into owned buffers.

use crate::parser::{ErrorCode, EventType};
use smallvec::SmallVec;
use std::sync::Arc;

/// Zero-copy event that references memory-mapped data.
///
/// Unlike the regular Event which may copy data into scratch buffers,
/// ZeroCopyEvent maintains direct references to the original memory-mapped data
/// whenever possible. This eliminates the majority of allocations and copies.
#[derive(Debug, Clone)]
pub struct ZeroCopyEvent<'data> {
    /// Type of event (StartElement, EndElement, Text, etc.)
    pub event_type: EventType,

    /// Event data slice (element name, text content, etc.)
    ///
    /// For StartElement/EndElement: element name
    /// For Text: text content
    /// For Comment: comment content
    /// For CData: CDATA content
    /// For ProcessingInstruction: PI target and data
    pub data: AttrSlice<'data>,

    /// True if this event is a continuation of previous event (chunked data)
    pub is_continuation: bool,

    /// Error code if event_type is Fault
    pub error: Option<ErrorCode>,

    /// Attributes for StartElement events
    pub attributes: ZeroCopyAttributes<'data>,
}

/// Zero-copy attribute collection.
///
/// Stores attributes with direct references to mmap'd data where possible.
/// Only allocates when entity decoding is required.
#[derive(Debug, Clone, Default)]
pub struct ZeroCopyAttributes<'data> {
    /// Attribute pairs stored inline for common case (≤8 attributes)
    ///
    /// SmallVec avoids heap allocation for most elements which have few attributes.
    attrs: SmallVec<[(AttrSlice<'data>, AttrSlice<'data>); 8]>,
}

/// Attribute slice that may reference mmap'd data directly or decoded data.
///
/// This enum allows us to avoid copying most attribute values while still
/// supporting entity decoding when necessary.
#[derive(Debug, Clone)]
pub enum AttrSlice<'data> {
    /// Direct reference to memory-mapped data (zero-copy)
    Direct(&'data [u8]),

    /// Shared decoded data (e.g., after entity reference expansion)
    ///
    /// Arc allows cheap cloning without copying the actual data.
    Decoded(Arc<[u8]>),
}

impl<'data> ZeroCopyEvent<'data> {
    /// Create a new zero-copy event.
    pub fn new(
        event_type: EventType,
        data: AttrSlice<'data>,
        is_continuation: bool,
        error: Option<ErrorCode>,
        attributes: ZeroCopyAttributes<'data>,
    ) -> Self {
        Self {
            event_type,
            data,
            is_continuation,
            error,
            attributes,
        }
    }

    /// Get the event data as a byte slice.
    ///
    /// This provides a uniform interface regardless of whether data is
    /// direct or decoded.
    pub fn data_bytes(&self) -> &[u8] {
        match &self.data {
            AttrSlice::Direct(slice) => slice,
            AttrSlice::Decoded(arc) => &arc[..],
        }
    }
}

impl<'data> ZeroCopyAttributes<'data> {
    /// Create an empty attribute collection.
    pub fn new() -> Self {
        Self {
            attrs: SmallVec::new(),
        }
    }

    /// Create with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            attrs: SmallVec::with_capacity(capacity),
        }
    }

    /// Add an attribute (name, value) pair.
    pub fn push(&mut self, name: AttrSlice<'data>, value: AttrSlice<'data>) {
        self.attrs.push((name, value));
    }

    /// Get the number of attributes.
    pub fn len(&self) -> usize {
        self.attrs.len()
    }

    /// Check if there are no attributes.
    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }

    /// Get an attribute by index.
    pub fn get(&self, index: usize) -> Option<(&[u8], &[u8])> {
        self.attrs.get(index).map(|(name, value)| {
            let name_bytes = match name {
                AttrSlice::Direct(s) => *s,
                AttrSlice::Decoded(arc) => &arc[..],
            };
            let value_bytes = match value {
                AttrSlice::Direct(s) => *s,
                AttrSlice::Decoded(arc) => &arc[..],
            };
            (name_bytes, value_bytes)
        })
    }

    /// Iterator over attribute (name, value) pairs as byte slices.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> + '_ {
        self.attrs.iter().map(|(name, value)| {
            let name_bytes = match name {
                AttrSlice::Direct(s) => *s,
                AttrSlice::Decoded(arc) => &arc[..],
            };
            let value_bytes = match value {
                AttrSlice::Direct(s) => *s,
                AttrSlice::Decoded(arc) => &arc[..],
            };
            (name_bytes, value_bytes)
        })
    }

    /// Find an attribute by name.
    pub fn find(&self, target_name: &[u8]) -> Option<&[u8]> {
        self.iter().find_map(|(name, value)| {
            if name == target_name {
                Some(value)
            } else {
                None
            }
        })
    }

    /// Clear all attributes.
    pub fn clear(&mut self) {
        self.attrs.clear();
    }
}

impl<'data> AttrSlice<'data> {
    /// Create a direct reference to mmap'd data.
    pub fn direct(slice: &'data [u8]) -> Self {
        AttrSlice::Direct(slice)
    }

    /// Create from decoded data.
    pub fn decoded(data: Vec<u8>) -> Self {
        AttrSlice::Decoded(Arc::from(data.into_boxed_slice()))
    }

    /// Get as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            AttrSlice::Direct(slice) => slice,
            AttrSlice::Decoded(arc) => &arc[..],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attr_slice_direct() {
        let data = b"hello";
        let slice = AttrSlice::direct(data);
        assert_eq!(slice.as_bytes(), b"hello");
    }

    #[test]
    fn test_attr_slice_decoded() {
        let data = vec![b'w', b'o', b'r', b'l', b'd'];
        let slice = AttrSlice::decoded(data);
        assert_eq!(slice.as_bytes(), b"world");
    }

    #[test]
    fn test_zero_copy_attributes() {
        let mut attrs = ZeroCopyAttributes::new();
        assert_eq!(attrs.len(), 0);
        assert!(attrs.is_empty());

        attrs.push(AttrSlice::direct(b"id"), AttrSlice::direct(b"123"));
        attrs.push(AttrSlice::direct(b"name"), AttrSlice::direct(b"test"));

        assert_eq!(attrs.len(), 2);
        assert!(!attrs.is_empty());

        assert_eq!(attrs.get(0), Some((&b"id"[..], &b"123"[..])));
        assert_eq!(attrs.get(1), Some((&b"name"[..], &b"test"[..])));

        assert_eq!(attrs.find(b"id"), Some(&b"123"[..]));
        assert_eq!(attrs.find(b"name"), Some(&b"test"[..]));
        assert_eq!(attrs.find(b"missing"), None);
    }

    #[test]
    fn test_zero_copy_event() {
        let data = AttrSlice::direct(b"element");
        let mut attrs = ZeroCopyAttributes::new();
        attrs.push(AttrSlice::direct(b"key"), AttrSlice::direct(b"value"));

        let event = ZeroCopyEvent::new(
            EventType::StartElement,
            data,
            false,
            None,
            attrs,
        );

        assert_eq!(event.event_type, EventType::StartElement);
        assert_eq!(event.data_bytes(), b"element");
        assert!(!event.is_continuation);
        assert_eq!(event.error, None);
        assert_eq!(event.attributes.len(), 1);
    }

    #[test]
    fn test_attributes_iterator() {
        let mut attrs = ZeroCopyAttributes::new();
        attrs.push(AttrSlice::direct(b"a"), AttrSlice::direct(b"1"));
        attrs.push(AttrSlice::direct(b"b"), AttrSlice::direct(b"2"));
        attrs.push(AttrSlice::direct(b"c"), AttrSlice::direct(b"3"));

        let pairs: Vec<_> = attrs.iter().collect();
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0], (&b"a"[..], &b"1"[..]));
        assert_eq!(pairs[1], (&b"b"[..], &b"2"[..]));
        assert_eq!(pairs[2], (&b"c"[..], &b"3"[..]));
    }
}
