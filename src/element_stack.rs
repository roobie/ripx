use crate::scratch::ScratchBuffers;

pub struct ElementFrame {
    pub name_start: usize,
    pub name_len: usize,
}

pub struct ElementStack {
    frames: Vec<ElementFrame>,
    capacity: usize,
}

impl ElementStack {
    pub fn with_capacity(cap: usize) -> Self {
        ElementStack {
            frames: Vec::with_capacity(cap),
            capacity: cap,
        }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Push a name by copying into `scratch.name`. The name will be truncated to
    /// `max_name_len` and to the remaining scratch capacity. Returns Err(()) if
    /// the depth limit would be exceeded.
    pub fn push_name(
        &mut self,
        scratch: &mut ScratchBuffers,
        name: &[u8],
        max_name_len: usize,
    ) -> Result<(), ()> {
        if self.frames.len() >= self.capacity {
            return Err(());
        }
        let to_copy = name
            .len()
            .min(max_name_len)
            .min(scratch.remaining_name_capacity());
        let start = scratch.name.len();
        scratch.name.extend_from_slice(&name[..to_copy]);
        let frame = ElementFrame {
            name_start: start,
            name_len: to_copy,
        };
        self.frames.push(frame);
        Ok(())
    }

    pub fn push(&mut self, frame: ElementFrame) -> Result<(), ()> {
        if self.frames.len() >= self.capacity {
            return Err(());
        }
        self.frames.push(frame);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<ElementFrame> {
        self.frames.pop()
    }

    pub fn top(&self) -> Option<&ElementFrame> {
        self.frames.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::ScratchBuffers;

    #[test]
    fn stack_push_pop() {
        let mut s = ElementStack::with_capacity(2);
        assert_eq!(s.len(), 0);
        assert!(
            s.push(ElementFrame {
                name_start: 0,
                name_len: 3
            })
            .is_ok()
        );
        assert!(
            s.push(ElementFrame {
                name_start: 3,
                name_len: 2
            })
            .is_ok()
        );
        assert!(
            s.push(ElementFrame {
                name_start: 5,
                name_len: 1
            })
            .is_err()
        );
        assert_eq!(s.len(), 2);
        s.pop();
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn push_name_into_scratch_and_truncate() {
        let mut scratch = ScratchBuffers::with_limits(3, 3, 3, 3, 3, 3, 3);
        let mut stack = ElementStack::with_capacity(4);
        let name = b"abcdef";
        let res = stack.push_name(&mut scratch, name, 10);
        assert!(res.is_ok());
        let top = stack.top().unwrap();
        assert_eq!(top.name_len, 3);
        assert_eq!(&scratch.name[..], &name[..3]);
    }
}
