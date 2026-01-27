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
        AttributeTable { entries: Vec::with_capacity(cap), capacity: cap }
    }

    pub fn len(&self) -> usize { self.entries.len() }

    pub fn clear(&mut self) { self.entries.clear(); }
}

pub struct Attributes<'a> {
    // Opaque reference to internal entries and buffers; real implementation will store refs.
    _private: (),
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> Attributes<'a> {
    pub fn len(&self) -> usize { 0 }
    pub fn is_empty(&self) -> bool { true }
    pub fn get(&self, _idx: usize) -> Option<(&'a [u8], &'a [u8])> { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_capacity() {
        let t = AttributeTable::with_capacity(4);
        assert!(t.entries.capacity() >= 4);
    }
}
