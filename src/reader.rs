use std::io::{self, BufRead};
use std::mem;

#[derive(Debug, PartialEq, Clone)]
pub enum Event {
    StartElement {
        name: Vec<u8>,
        attributes: Vec<(Vec<u8>, Vec<u8>)>,
    },
    EndElement {
        name: Vec<u8>,
        accumulated: Vec<u8>,
    },
    Text(Vec<u8>),
    Comment(Vec<u8>),
    CData(Vec<u8>),
    Eof,
}

enum State {
    OutsideTag,
    InsideTag,
}

pub struct Reader<R: BufRead> {
    source: R,
    buf: Vec<u8>,
    pos: usize,
    end: usize,
    state: State,
    finished: bool,
    // NEW: synthetic end-tags for empty elements (<tag/>)
    pending_end: Vec<Vec<u8>>,
    accumulator: Vec<u8>,
}

impl<R: BufRead> Reader<R> {
    pub fn from_reader(source: R) -> Self {
        Reader {
            source,
            buf: vec![0; 8192],
            pos: 0,
            end: 0,
            state: State::OutsideTag,
            finished: false,
            pending_end: Vec::new(),
            accumulator: Vec::new(),
        }
    }

    fn get_accumulated(&mut self) -> Vec<u8> {
        mem::take(&mut self.accumulator)
    }

    /// Try to resynchronize after an error.
    /// Skips forward in the stream until the next plausible markup start
    /// (`<tag`, `</`, `<!--`, `<![CDATA[`, `<?`), or EOF.
    pub fn advance(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }

        loop {
            self.fill_buf()?;
            if self.finished {
                return Ok(());
            }

            while self.pos < self.end {
                let b = self.buf[self.pos];
                if b != b'<' {
                    self.pos += 1;
                    continue;
                }

                // We are at a '<'. Try to look ahead without committing.
                let start = self.pos;

                // Ensure we have a few lookahead bytes (but don't require, just best-effort)
                self.fill_buf()?; // may not add more, but that's fine

                // safe bounds
                let available = self.end - start;
                let s = &self.buf[start..self.end];

                let looks_like_start_tag = available >= 2 && {
                    let c = s[1];
                    is_name_char(c) // includes letters, digits, underscore, etc.
                };

                let looks_like_end_tag = available >= 2 && s[1] == b'/';
                let looks_like_comment = available >= 4 && &s[1..4] == b"!--";
                let looks_like_cdata = available >= 8 && &s[1..8] == b"![CDATA"; // enough to disambiguate
                let looks_like_pi = available >= 2 && s[1] == b'?';

                if looks_like_start_tag
                    || looks_like_end_tag
                    || looks_like_comment
                    || looks_like_cdata
                    || looks_like_pi
                {
                    // Leave cursor at '<' and reset state so next_event can continue.
                    self.state = State::OutsideTag;
                    return Ok(());
                }

                // Not a recognizable construct, skip this '<' and continue scanning.
                self.pos += 1;
            }
        }
    }

    pub fn next_event(&mut self) -> io::Result<Event> {
        // If we have a synthetic end element to emit, do that first.
        if let Some(name) = self.pending_end.pop() {
            return Ok(Event::EndElement {
                name,
                accumulated: self.get_accumulated(),
            });
        }

        if self.finished {
            return Ok(Event::Eof);
        }

        loop {
            match self.state {
                State::OutsideTag => {
                    let text = self.read_text_until_lt()?;
                    self.accumulator.extend_from_slice(&text);
                    if !text.is_empty() {
                        return Ok(Event::Text(text));
                    }
                    if self.finished {
                        return Ok(Event::Eof);
                    }
                    self.pos += 1; // consume '<'
                    self.accumulator.push(b'<');
                    self.state = State::InsideTag;
                }
                State::InsideTag => {
                    let c = self.peek_byte()?;
                    match c {
                        b'/' => {
                            self.pos += 1; // consume '/'
                            self.accumulator.push(c);
                            let name = self.read_name()?;
                            self.accumulator.extend_from_slice(&name);
                            self.skip_spaces()?;
                            self.expect_byte(b'>')?;
                            self.accumulator.push(b'>');
                            self.state = State::OutsideTag;
                            return Ok(Event::EndElement {
                                name,
                                accumulated: self.get_accumulated(),
                            });
                        }
                        b'!' => {
                            self.pos += 1; // consume '!'
                            self.accumulator.push(c);
                            if self.try_consume(b"--")? {
                                let comment = self.read_until_bytes(b"-->")?;
                                self.accumulator.extend_from_slice(&comment);
                                self.state = State::OutsideTag;
                                return Ok(Event::Comment(comment));
                            } else if self.try_consume(b"[CDATA[")? {
                                let cdata = self.read_until_bytes(b"]]>")?;
                                self.accumulator.extend_from_slice(&cdata);
                                self.state = State::OutsideTag;
                                return Ok(Event::CData(cdata));
                            } else {
                                self.skip_until_byte(b'>')?;
                                self.accumulator.push(b'>');
                                self.state = State::OutsideTag;
                                continue;
                            }
                        }
                        b'?' => {
                            self.pos += 1; // consume '?'
                            self.accumulator.push(c);
                            self.skip_until_bytes(b"?>")?;
                            self.accumulator.push(b'>');
                            self.state = State::OutsideTag;
                            continue;
                        }
                        _ => {
                            let name = self.read_name()?;
                            self.accumulator.extend_from_slice(&name);
                            let mut attrs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
                            loop {
                                self.skip_spaces()?;
                                let b = self.peek_byte()?;
                                match b {
                                    b'/' => {
                                        // empty element: <tag .../>
                                        self.pos += 1;
                                        self.expect_byte(b'>')?;
                                        self.accumulator.push(b'>');
                                        self.state = State::OutsideTag;

                                        // Emit Start now, schedule End for next call
                                        self.pending_end.push(name.clone());
                                        return Ok(Event::StartElement {
                                            name,
                                            attributes: attrs,
                                        });
                                    }
                                    b'>' => {
                                        self.pos += 1;
                                        self.state = State::OutsideTag;
                                        self.accumulator.push(b'>');
                                        return Ok(Event::StartElement {
                                            name,
                                            attributes: attrs,
                                        });
                                    }
                                    _ => {
                                        let (k, v) = self.read_attribute()?;
                                        attrs.push((k, v));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // === low-level helpers ===

    fn fill_buf(&mut self) -> io::Result<()> {
        if self.pos < self.end {
            return Ok(());
        }
        if self.buf.is_empty() {
            self.buf.resize(8192, 0);
        }
        let n = self.source.read(&mut self.buf[..])?;
        if n == 0 {
            self.finished = true;
            return Ok(());
        }
        self.pos = 0;
        self.end = n;
        Ok(())
    }

    fn peek_byte(&mut self) -> io::Result<u8> {
        self.fill_buf()?;
        if self.finished || self.pos >= self.end {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF"));
        }
        Ok(self.buf[self.pos])
    }

    fn read_text_until_lt(&mut self) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            self.fill_buf()?;
            if self.finished {
                break;
            }
            while self.pos < self.end {
                let b = self.buf[self.pos];
                if b == b'<' {
                    return Ok(out);
                }
                out.push(b);
                self.pos += 1;
            }
        }
        Ok(out)
    }

    fn read_name(&mut self) -> io::Result<Vec<u8>> {
        self.skip_spaces()?;
        let mut out = Vec::new();
        loop {
            self.fill_buf()?;
            if self.finished {
                break;
            }
            while self.pos < self.end {
                let b = self.buf[self.pos];
                if is_name_char(b) {
                    out.push(b);
                    self.pos += 1;
                } else {
                    if out.is_empty() {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected name"));
                    }
                    return Ok(out);
                }
            }
        }
        if out.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "EOF while reading name",
            ));
        }
        Ok(out)
    }

    fn skip_spaces(&mut self) -> io::Result<()> {
        loop {
            self.fill_buf()?;
            if self.finished {
                return Ok(());
            }
            let mut advanced = false;
            while self.pos < self.end {
                let b = self.buf[self.pos];
                if matches!(b, b' ' | b'\n' | b'\r' | b'\t') {
                    self.accumulator.push(b);
                    self.pos += 1;
                    advanced = true;
                } else {
                    return Ok(());
                }
            }
            if !advanced {
                return Ok(());
            }
        }
    }

    fn expect_byte(&mut self, expected: u8) -> io::Result<()> {
        let b = self.peek_byte()?;
        if b == expected {
            self.pos += 1;
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "expected byte {:?}, found {:?}",
                    expected as char, b as char
                ),
            ))
        }
    }

    fn read_attribute(&mut self) -> io::Result<(Vec<u8>, Vec<u8>)> {
        let name = self.read_name()?;
        self.accumulator.extend_from_slice(&name);
        self.skip_spaces()?;
        self.expect_byte(b'=')?;
        self.accumulator.push(b'=');
        self.skip_spaces()?;

        let quote = self.peek_byte()?;
        if quote != b'"' && quote != b'\'' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected quote for attribute value",
            ));
        }
        self.pos += 1; // consume quote
        self.accumulator.push(quote);

        let mut out = Vec::new();
        loop {
            self.fill_buf()?;
            if self.finished {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF in attribute value",
                ));
            }
            while self.pos < self.end {
                let b = self.buf[self.pos];
                if b == quote {
                    self.pos += 1;
                    self.accumulator.push(quote);
                    return Ok((name, out));
                } else {
                    out.push(b);
                    self.accumulator.push(b);
                    self.pos += 1;
                }
            }
        }
    }

    /// Streaming search for a pattern that may cross buffer boundaries.
    /// Returns everything up to (but not including) the pattern.
    fn read_until_bytes(&mut self, pat: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut matched = 0usize;

        if pat.is_empty() {
            return Ok(Vec::new());
        }

        loop {
            self.fill_buf()?;
            if self.finished {
                break;
            }

            while self.pos < self.end {
                let b = self.buf[self.pos];
                self.pos += 1;

                if b == pat[matched] {
                    matched += 1;
                    if matched == pat.len() {
                        return Ok(out);
                    }
                } else {
                    if matched > 0 {
                        // flush previously matched bytes
                        out.extend_from_slice(&pat[..matched]);
                        matched = 0;
                        // reprocess current byte as potential start of pattern
                        if b == pat[0] {
                            matched = 1;
                        } else {
                            out.push(b);
                        }
                    } else {
                        out.push(b);
                    }
                }
            }
        }

        // pattern not found; return collected text
        Ok(out)
    }

    fn skip_until_byte(&mut self, target: u8) -> io::Result<()> {
        loop {
            self.fill_buf()?;
            if self.finished {
                return Ok(());
            }
            while self.pos < self.end {
                let b = self.buf[self.pos];
                self.pos += 1;
                if b == target {
                    return Ok(());
                }
            }
        }
    }

    /// Streaming skip for a multi-byte pattern that may cross buffer boundaries.
    fn skip_until_bytes(&mut self, pat: &[u8]) -> io::Result<()> {
        if pat.is_empty() {
            return Ok(());
        }

        let mut matched = 0usize;

        loop {
            self.fill_buf()?;
            if self.finished {
                return Ok(());
            }

            while self.pos < self.end {
                let b = self.buf[self.pos];
                self.pos += 1;

                if b == pat[matched] {
                    matched += 1;
                    if matched == pat.len() {
                        return Ok(());
                    }
                } else {
                    if matched > 0 {
                        matched = 0;
                        if b == pat[0] {
                            matched = 1;
                        }
                    }
                }
            }
        }
    }

    /// Try to consume `pat` starting at current position without crossing logical
    /// stream position on failure (no bytes consumed if it doesn't match).
    fn try_consume(&mut self, pat: &[u8]) -> io::Result<bool> {
        if pat.is_empty() {
            return Ok(true);
        }

        // We never call fill_buf here in a way that discards existing bytes.
        // Just ensure we have enough bytes in the current buffer; if not, we
        // give up (returns false) instead of trying to span refill boundaries.
        self.fill_buf()?;
        if self.finished {
            return Ok(false);
        }

        if self.end - self.pos < pat.len() {
            return Ok(false);
        }

        if &self.buf[self.pos..self.pos + pat.len()] == pat {
            self.pos += pat.len();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn is_name_char(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b':' | b'.' | b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn events_from(s: &str) -> Vec<Event> {
        let cursor = Cursor::new(s.as_bytes());
        let mut reader = Reader::from_reader(cursor);
        let mut out = Vec::new();
        loop {
            let ev = match reader.next_event() {
                Ok(event) => event,
                Err(error) => {
                    eprintln!("Parse error: {error}");
                    let _ = reader.advance();
                    continue;
                }
            };
            match ev {
                Event::Eof => break,
                _ => out.push(ev),
            }
        }
        out
    }

    fn strip_ws_text(events: Vec<Event>) -> Vec<Event> {
        events
            .into_iter()
            .filter_map(|e| match e {
                Event::Text(t) => {
                    if t.iter()
                        .all(|&b| b == b' ' || b == b'\n' || b == b'\r' || b == b'\t')
                    {
                        None
                    } else {
                        Some(Event::Text(t))
                    }
                }
                other => Some(other),
            })
            .collect()
    }

    #[test]
    fn simple_accumulator() {
        let xml = "<root></root>";
        let ev = events_from(xml);
        assert_eq!(ev.len(), 2);
        match &ev[1] {
            Event::EndElement { accumulated, .. } => {
                assert_eq!(accumulated.as_slice(), xml.as_bytes())
            }
            _ => panic!("expected EndElement"),
        }
    }

    fn test_broken(xml: &str) {
        let ev = events_from(xml);
        assert_eq!(ev.len(), 2);
        match &ev[1] {
            Event::EndElement { accumulated, .. } => {
                assert_eq!(
                    String::from_utf8_lossy(accumulated),
                    String::from_utf8_lossy(xml.as_bytes())
                )
            }
            _ => panic!("expected EndElement"),
        }
    }

    #[test]
    fn broken_tag_recovery_1() {
        let xml = "<root><broken</root>";
        test_broken(xml);
    }

    #[test]
    fn broken_tag_recovery_2() {
        let xml = "<root><broken a</root>";
        test_broken(xml);
    }
    #[test]
    fn broken_tag_recovery_3() {
        let xml = "<root><broken a=</root>";
        test_broken(xml);
    }
    #[test]
    fn broken_tag_recovery_4() {
        let xml = "<root><broken a=\"</root>";
        test_broken(xml);
    }

    #[test]
    fn single_empty_element_no_attrs() {
        let xml = "<root></root>";
        let ev = events_from(xml);
        assert_eq!(ev.len(), 2);
        match &ev[0] {
            Event::StartElement { name, attributes } => {
                assert_eq!(name.as_slice(), b"root");
                assert!(attributes.is_empty());
            }
            _ => panic!("expected StartElement"),
        }
        match &ev[1] {
            Event::EndElement { name, .. } => assert_eq!(name.as_slice(), b"root"),
            _ => panic!("expected EndElement"),
        }
    }

    #[test]
    fn single_empty_element_self_closing() {
        let xml = "<root/>";
        let ev = events_from(xml);
        // StartElement() and EndElement()
        assert_eq!(ev.len(), 2);
        match &ev[0] {
            Event::StartElement { name, attributes } => {
                assert_eq!(name.as_slice(), b"root");
                assert!(attributes.is_empty());
            }
            _ => panic!("expected StartElement"),
        }
    }

    #[test]
    fn nested_elements_and_text() {
        let xml = "<root><child>hello</child><child>world</child></root>";
        let ev = strip_ws_text(events_from(xml));

        // Expected sequence:
        // Start(root)
        // Start(child), Text("hello"), End(child)
        // Start(child), Text("world"), End(child)
        // End(root)
        assert_eq!(ev.len(), 8);

        match &ev[0] {
            Event::StartElement { name, .. } => assert_eq!(name.as_slice(), b"root"),
            _ => panic!("expected StartElement(root)"),
        }
        match &ev[1] {
            Event::StartElement { name, .. } => assert_eq!(name.as_slice(), b"child"),
            _ => panic!("expected StartElement(child)"),
        }
        match &ev[2] {
            Event::Text(t) => assert_eq!(t.as_slice(), b"hello"),
            _ => panic!("expected Text(hello)"),
        }
        match &ev[3] {
            Event::EndElement { name, .. } => assert_eq!(name.as_slice(), b"child"),
            _ => panic!("expected EndElement(child)"),
        }
        match &ev[4] {
            Event::StartElement { name, .. } => assert_eq!(name.as_slice(), b"child"),
            _ => panic!("expected StartElement(child)"),
        }
        match &ev[5] {
            Event::Text(t) => assert_eq!(t.as_slice(), b"world"),
            _ => panic!("expected Text(world)"),
        }
        match &ev[6] {
            Event::EndElement { name, .. } => {
                assert!(name.as_slice() == b"child" || name.as_slice() == b"root")
            }
            _ => panic!("expected Text(world)"),
        }
    }

    #[test]
    fn attributes_simple() {
        let xml = r#"<root a="1" b="2" c="3"></root>"#;
        let ev = events_from(xml);
        assert_eq!(ev.len(), 2);
        match &ev[0] {
            Event::StartElement { name, attributes } => {
                assert_eq!(name.as_slice(), b"root");
                assert_eq!(attributes.len(), 3);
                assert_eq!(attributes[0].0.as_slice(), b"a");
                assert_eq!(attributes[0].1.as_slice(), b"1");
                assert_eq!(attributes[1].0.as_slice(), b"b");
                assert_eq!(attributes[1].1.as_slice(), b"2");
                assert_eq!(attributes[2].0.as_slice(), b"c");
                assert_eq!(attributes[2].1.as_slice(), b"3");
            }
            _ => panic!("expected StartElement(root)"),
        }
    }

    #[test]
    fn attributes_single_quoted_and_spaces() {
        let xml = "<root  a = '1'   b= \"2\" ></root>";
        let ev = events_from(xml);
        assert_eq!(ev.len(), 2);
        match &ev[0] {
            Event::StartElement { name, attributes } => {
                assert_eq!(name.as_slice(), b"root");
                // Order is preserved by parser
                assert_eq!(attributes.len(), 2);
                assert_eq!(attributes[0].0.as_slice(), b"a");
                assert_eq!(attributes[0].1.as_slice(), b"1");
                assert_eq!(attributes[1].0.as_slice(), b"b");
                assert_eq!(attributes[1].1.as_slice(), b"2");
            }
            _ => panic!("expected StartElement(root)"),
        }
    }

    #[test]
    fn attribute_entity_decoding() {
        let xml = r#"<root a="&lt;&gt;&amp;&quot;&apos;"></root>"#;
        let ev = events_from(xml);
        assert_eq!(ev.len(), 2);
        match &ev[0] {
            Event::StartElement { attributes, .. } => {
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].0.as_slice(), b"a");
                assert_eq!(attributes[0].1.as_slice(), b"&lt;&gt;&amp;&quot;&apos;");
            }
            _ => panic!("expected StartElement(root)"),
        }
    }

    #[test]
    fn text_between_tags() {
        let xml = "<root>hello world</root>";
        let ev = events_from(xml);
        assert_eq!(ev.len(), 3);
        match &ev[1] {
            Event::Text(t) => assert_eq!(t.as_slice(), b"hello world"),
            _ => panic!("expected Text(hello world)"),
        }
    }

    #[test]
    fn leading_text() {
        let xml = "  pre<root>inside</root>";
        let ev = events_from(xml);
        // We expect 4 text nodes + 2 element events, but whitespace is included.
        assert_eq!(ev.len(), 4);
        assert!(matches!(ev[0], Event::Text(_)));
        assert!(matches!(ev[1], Event::StartElement { .. }));
        assert!(matches!(ev[2], Event::Text(_)));
        assert!(matches!(ev[3], Event::EndElement { .. }));
    }

    #[test]
    fn leading_and_trailing_text() {
        let xml = "  pre<root>inside</root>post  ";
        let ev = events_from(xml);
        // We expect 4 text nodes + 2 element events, but whitespace is included.
        assert_eq!(ev.len(), 5);
        assert!(matches!(ev[0], Event::Text(_)));
        assert!(matches!(ev[1], Event::StartElement { .. }));
        assert!(matches!(ev[2], Event::Text(_)));
        assert!(matches!(ev[3], Event::EndElement { .. }));
        assert!(matches!(ev[4], Event::Text(_)));
        // Note: text after closing root may be lost depending on implementation
        // (our MVP stops at EOF after last element); adjust if you want post-text.
    }

    #[test]
    fn comment_basic() {
        let xml = "<root><!-- this is a comment --><child/></root>";
        let ev = strip_ws_text(events_from(xml));
        // Start(root), Comment, Start(child), End(child), End(root)
        assert_eq!(ev.len(), 5);
        matches!(&ev[1], Event::Comment(_));
    }

    #[test]
    fn cdata_basic() {
        let xml = "<root><![CDATA[some <weird> & text]]></root>";
        let ev = strip_ws_text(events_from(xml));
        // Start(root), CData, End(root)
        assert_eq!(ev.len(), 3);
        match &ev[1] {
            Event::CData(t) => assert_eq!(t.as_slice(), b"some <weird> & text"),
            _ => panic!("expected CData"),
        }
    }

    #[test]
    fn processing_instruction_is_skipped() {
        let xml = r#"<?xml version="1.0"?><root><child>t</child></root>"#;
        let ev = strip_ws_text(events_from(xml));
        // We should see only root/child/text events, PI skipped.
        assert_eq!(ev.len(), 5);
        match &ev[0] {
            Event::StartElement { name, .. } => assert_eq!(name.as_slice(), b"root"),
            _ => panic!("expected StartElement(root)"),
        }
    }

    #[test]
    fn complex_mixed() {
        let xml = r#"
            <root a="1">
                text1
                <!-- comment -->
                <child><![CDATA[cdata text]]>inner</child>
                text2
            </root>
        "#;
        let ev = strip_ws_text(events_from(xml));
        // A rough check: we should see:
        // Start(root), Text("text1\n..."), Comment, Start(child),
        // CData("cdata text"), Text("inner"), End(child), Text("text2\n..."),
        // End(root)
        assert!(matches!(ev[0], Event::StartElement { .. }));
        assert!(matches!(
            ev.iter().find(|e| matches!(e, Event::Comment(_))),
            Some(_)
        ));
        assert!(matches!(
            ev.iter().find(|e| matches!(e, Event::CData(_))),
            Some(_)
        ));
    }

    #[test]
    fn malformed_tag_coarse_recovery() {
        // Missing closing '>' on child; our coarse handler should skip ahead and still see end root.
        let xml = "<root><child attr='1'<broken>xxx</root>";
        let ev = events_from(xml);

        // At minimum, we expect a Start(root) and an End(root) (parser may or may not emit child).
        assert!(
            matches!(ev.first(), Some(Event::StartElement { name, .. }) if name.as_slice() == b"root")
        );
        assert!(
            matches!(ev.last(), Some(Event::EndElement { name, .. }) if name.as_slice() == b"root")
        );
    }

    #[test]
    fn empty_input() {
        let xml = "";
        let ev = events_from(xml);
        assert!(ev.is_empty());
    }

    #[test]
    fn whitespace_only_input() {
        let xml = "   \n\t  ";
        let ev = events_from(xml);
        // Could be a single Text or none depending on implementation;
        // in this MVP, read_text_until_lt will return a Text then EOF.
        assert_eq!(ev.len(), 1);
        match &ev[0] {
            Event::Text(t) => assert_eq!(t.as_slice(), b"   \n\t  "),
            _ => panic!("expected single Text event"),
        }
    }

    #[test]
    fn fill_buf_reads_into_internal_buffer() {
        use std::io::Cursor;
        let cursor = Cursor::new(b"<root></root>");
        let mut reader = Reader::from_reader(cursor);
        // sanity: Reader::from_reader should allocate the large buffer
        assert_eq!(reader.buf.len(), 8192);
        // force a refill
        reader.pos = reader.end;
        reader.fill_buf().unwrap();
        // Expect fill_buf to have read data from the source
        assert!(
            !reader.finished,
            "fill_buf marked finished prematurely (read 0 bytes)"
        );
        assert!(reader.end > 0, "fill_buf did not fill buffer (end == 0)");
    }
}
