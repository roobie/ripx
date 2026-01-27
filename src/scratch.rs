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

    /// Remaining capacity for a vector.
    fn remaining_capacity(vec: &Vec<u8>) -> usize {
        vec.capacity().saturating_sub(vec.len())
    }

    pub fn remaining_name_capacity(&self) -> usize {
        Self::remaining_capacity(&self.name)
    }
    pub fn remaining_attr_name_capacity(&self) -> usize {
        Self::remaining_capacity(&self.attr_name)
    }
    pub fn remaining_attr_value_capacity(&self) -> usize {
        Self::remaining_capacity(&self.attr_value)
    }

    /// Push into `name` buffer, truncating if necessary. Returns (bytes_copied, was_truncated).
    pub fn push_name(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.name);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.name.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }

    pub fn push_attr_name(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.attr_name);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.attr_name.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }

    pub fn push_attr_value(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.attr_value);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.attr_value.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }

    pub fn push_text_chunk(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.text);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.text.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }

    pub fn push_comment_chunk(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.comment);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.comment.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }

    pub fn push_cdata_chunk(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.cdata);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.cdata.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }

    pub fn push_pi_chunk(&mut self, data: &[u8]) -> (usize, bool) {
        let rem = Self::remaining_capacity(&self.pi);
        let to_copy = data.len().min(rem);
        let truncated = to_copy < data.len();
        self.pi.extend_from_slice(&data[..to_copy]);
        (to_copy, truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_truncation() {
        let mut s = ScratchBuffers::with_limits(3, 3, 3, 3, 3, 3, 3);
        let data = b"abcdef";
        let (copied, truncated) = s.push_name(data);
        assert_eq!(copied, 3);
        assert!(truncated);
        assert_eq!(s.name.len(), 3);
    }

    #[test]
    fn push_multiple_buffers() {
        let mut s = ScratchBuffers::with_limits(4, 4, 4, 4, 4, 4, 4);
        let (c1, t1) = s.push_attr_name(b"xy");
        assert_eq!(c1, 2);
        assert!(!t1);
        let (c2, t2) = s.push_attr_value(b"12345");
        assert_eq!(c2, 4);
        assert!(t2);
    }
}

#[cfg(test)]
mod scratch_capacity_test {
    use super::*;

    #[test]
    fn scratch_capacity() {
        let s = ScratchBuffers::with_limits(10, 10, 10, 10, 10, 10, 10);
        assert!(s.name.capacity() >= 10);
    }
}
