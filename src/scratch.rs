/// Scratch buffers with bounded capacity for various token kinds.
pub struct ScratchBuffers {
    pub name: Vec<u8>,
    pub attr_name: Vec<u8>,
    pub attr_value: Vec<u8>,
    pub text: Vec<u8>,
    pub comment: Vec<u8>,
    pub cdata: Vec<u8>,
    pub pi: Vec<u8>,
}

impl ScratchBuffers {
    pub fn with_limits(
        name: usize,
        attr_name: usize,
        attr_value: usize,
        text: usize,
        comment: usize,
        cdata: usize,
        pi: usize,
    ) -> Self {
        ScratchBuffers {
            name: Vec::with_capacity(name),
            attr_name: Vec::with_capacity(attr_name),
            attr_value: Vec::with_capacity(attr_value),
            text: Vec::with_capacity(text),
            comment: Vec::with_capacity(comment),
            cdata: Vec::with_capacity(cdata),
            pi: Vec::with_capacity(pi),
        }
    }

    pub fn clear_all(&mut self) {
        self.name.clear();
        self.attr_name.clear();
        self.attr_value.clear();
        self.text.clear();
        self.comment.clear();
        self.cdata.clear();
        self.pi.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_capacity() {
        let s = ScratchBuffers::with_limits(10, 10, 10, 10, 10, 10, 10);
        assert!(s.name.capacity() >= 10);
    }
}
