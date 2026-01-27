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
        ElementStack { frames: Vec::with_capacity(cap), capacity: cap }
    }

    pub fn len(&self) -> usize { self.frames.len() }

    pub fn push(&mut self, frame: ElementFrame) -> Result<(), ()> {
        if self.frames.len() >= self.capacity {
            return Err(());
        }
        self.frames.push(frame);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<ElementFrame> { self.frames.pop() }

    pub fn top(&self) -> Option<&ElementFrame> { self.frames.last() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_push_pop() {
        let mut s = ElementStack::with_capacity(2);
        assert_eq!(s.len(), 0);
        assert!(s.push(ElementFrame { name_start: 0, name_len: 3 }).is_ok());
        assert!(s.push(ElementFrame { name_start: 3, name_len: 2 }).is_ok());
        assert!(s.push(ElementFrame { name_start: 5, name_len: 1 }).is_err());
        assert_eq!(s.len(), 2);
        s.pop();
        assert_eq!(s.len(), 1);
    }
}
