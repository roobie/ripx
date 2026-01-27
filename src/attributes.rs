use crate::scratch::ScratchBuffers;

#[derive(Clone, Copy, Debug)]
pub enum NameBufferKind {
    Input,
    NameScratch,
    AttrNameScratch,
    AttrValueScratch,
}

#[derive(Clone, Copy, Debug)]
pub struct InternalAttribute {
    pub name_buffer: NameBufferKind,
    pub name_start: usize,
    pub name_len: usize,
    pub value_buffer: NameBufferKind,
    pub value_start: usize,
    pub value_len: usize,
}

pub struct AttributeTable {
    pub entries: Vec<InternalAttribute>,
    pub capacity: usize,
}

impl AttributeTable {
    pub fn with_capacity(cap: usize) -> Self {
        AttributeTable {
            entries: Vec::with_capacity(cap),
            capacity: cap,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Push an internal attribute; returns Err(()) if capacity exceeded.
    pub fn push_entry(&mut self, ia: InternalAttribute) -> Result<(), ()> {
        if self.entries.len() >= self.capacity {
            return Err(());
        }
        self.entries.push(ia);
        Ok(())
    }

    pub fn as_slice(&self) -> &[InternalAttribute] {
        &self.entries
    }
}

pub struct AttributesInner<'a> {
    pub entries: &'a [InternalAttribute],
    pub input_buf: &'a [u8],
    pub scratch: &'a ScratchBuffers,
}

pub struct Attributes<'a> {
    inner: AttributesInner<'a>,
}

impl<'a> Attributes<'a> {
    pub fn from_parts(
        entries: &'a [InternalAttribute],
        input: &'a [u8],
        scratch: &'a ScratchBuffers,
    ) -> Self {
        Attributes {
            inner: AttributesInner {
                entries,
                input_buf: input,
                scratch,
            },
        }
    }

    fn resolve(&self, kind: NameBufferKind, start: usize, len: usize) -> &'a [u8] {
        match kind {
            NameBufferKind::Input => &self.inner.input_buf[start..start + len],
            NameBufferKind::NameScratch => &self.inner.scratch.name[start..start + len],
            NameBufferKind::AttrNameScratch => &self.inner.scratch.attr_name[start..start + len],
            NameBufferKind::AttrValueScratch => &self.inner.scratch.attr_value[start..start + len],
        }
    }

    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<(&'a [u8], &'a [u8])> {
        let ia = self.inner.entries.get(idx)?;
        Some((
            self.resolve(ia.name_buffer, ia.name_start, ia.name_len),
            self.resolve(ia.value_buffer, ia.value_start, ia.value_len),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::ScratchBuffers;

    #[test]
    fn table_capacity() {
        let t = AttributeTable::with_capacity(4);
        assert!(t.entries.capacity() >= 4);
    }

    #[test]
    fn attributes_resolution_input_and_scratch() {
        let mut table = AttributeTable::with_capacity(2);
        let ia1 = InternalAttribute {
            name_buffer: NameBufferKind::Input,
            name_start: 0,
            name_len: 3,
            value_buffer: NameBufferKind::Input,
            value_start: 3,
            value_len: 2,
        };
        let ia2 = InternalAttribute {
            name_buffer: NameBufferKind::AttrNameScratch,
            name_start: 0,
            name_len: 2,
            value_buffer: NameBufferKind::AttrValueScratch,
            value_start: 0,
            value_len: 3,
        };
        assert!(table.push_entry(ia1).is_ok());
        assert!(table.push_entry(ia2).is_ok());

        let input = b"namvalxx"; // name(0..3)=nam, value(3..5)=va
        let mut scratch = ScratchBuffers::with_limits(4, 4, 4, 4, 4, 4, 4);
        scratch.attr_name.extend_from_slice(b"nm");
        scratch.attr_value.extend_from_slice(b"123");

        let attrs = Attributes::from_parts(table.as_slice(), input, &scratch);
        assert_eq!(attrs.len(), 2);
        let (n1, v1) = attrs.get(0).unwrap();
        assert_eq!(n1, &input[0..3]);
        assert_eq!(v1, &input[3..5]);
        let (n2, v2) = attrs.get(1).unwrap();
        assert_eq!(n2, &scratch.attr_name[..2]);
        assert_eq!(v2, &scratch.attr_value[..3]);
    }
}
