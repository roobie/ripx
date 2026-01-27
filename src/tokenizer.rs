// Tokenizer primitives and state machine helpers will live here.

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

impl Default for State {
    fn default() -> Self { State::Data }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_data() {
        let s = State::default();
        assert_eq!(s, State::Data);
    }
}
