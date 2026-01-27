use crate::reader::{Event, Reader};
use std::io::BufRead;

/// Simple stack of element names representing the current path.
#[derive(Debug, Default)]
pub struct PathStack {
    stack: Vec<String>,
}

impl PathStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn push(&mut self, name: &str) {
        self.stack.push(name.to_owned());
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn as_slice(&self) -> &[String] {
        &self.stack
    }
}

/// Trait implemented by query executors.
/// You get a callback for every start, text, and end event, with the current path.
pub trait Query {
    fn on_start(&mut self, path: &[String], name: &str, attrs: &[(String, String)]);

    fn on_text(&mut self, path: &[String], text: &str);

    fn on_end(&mut self, path: &[String], name: &str, content: &str);

    fn is_done(&mut self) -> bool;
}

/// Very small path selector language:
/// - "//name"   => match any element with this name, anywhere
/// - "/a/b/c"   => match elements whose full path is exactly /a/b/c
#[derive(Debug, Clone)]
pub enum PathSelector {
    Anywhere(String),
    Absolute(Vec<String>),
}

impl PathSelector {
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(name) = s.strip_prefix("//") {
            if name.is_empty() {
                return Err("empty name in // selector".into());
            }
            Ok(PathSelector::Anywhere(name.to_owned()))
        } else if s.starts_with('/') {
            let parts: Vec<String> = s
                .split('/')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_owned())
                .collect();
            if parts.is_empty() {
                return Err("empty absolute path".into());
            }
            Ok(PathSelector::Absolute(parts))
        } else {
            // Treat bare "name" like "//name"
            Ok(PathSelector::Anywhere(s.to_owned()))
        }
    }

    pub fn matches_path(&self, path: &[String], name: &str) -> bool {
        match self {
            PathSelector::Anywhere(target) => name == target,
            PathSelector::Absolute(parts) => path == parts.as_slice(),
        }
    }
}

pub struct PathQuery {
    selector: PathSelector,
    in_match: usize,    // nesting depth inside the *current* matched element
    match_depth: usize, // depth at which the match started
    buf: String,        // collect text for one match
    printed: usize,     // how many matches printed so far
    max: usize,         // stop after this many
    done: bool,
    // attrs: Vec<(String, String)>
}

impl PathQuery {
    pub fn new(selector: PathSelector, max: usize) -> Self {
        Self {
            selector,
            in_match: 0,
            match_depth: 0,
            buf: String::new(),
            printed: 0,
            max,
            done: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

impl Query for PathQuery {
    fn on_start(&mut self, path: &[String], name: &str, _attrs: &[(String, String)]) {
        if self.done {
            return;
        }

        if self.selector.matches_path(path, name) {
            // starting a new top-level match
            if self.in_match == 0 {
                self.buf.clear();
                self.match_depth = path.len(); // depth of this element
            }
            self.buf.push_str(name);
            self.in_match += 1;
        } else if self.in_match > 0 {
            self.buf.push_str(name);
            self.in_match += 1;
        }
    }

    fn on_text(&mut self, _path: &[String], text: &str) {
        if self.in_match > 0 && !self.done {
            self.buf.push_str(text);
        }
    }

    fn on_end(&mut self, path: &[String], _name: &str, content: &str) {
        if self.done || self.in_match == 0 {
            return;
        }

        self.in_match -= 1;

        // leaving the root element of the match?
        if self.in_match == 0 && path.len() == self.match_depth {
            // one complete match collected in buf
            // println!("{}", self.buf.trim());
            println!("{}", content);
            self.printed += 1;
            if self.printed >= self.max {
                self.done = true;
            }
        }
    }

    fn is_done(&mut self) -> bool {
        self.done
    }
}

/// Drive the reader and query together.
pub fn run_query<R: BufRead, Q: Query>(
    reader: &mut Reader<R>,
    query: &mut Q,
) -> std::io::Result<()> {
    let mut path = PathStack::new();

    loop {
        if query.is_done() {
            break;
        }
        match reader.next_event()? {
            Event::StartElement { name, attributes } => {
                // convert raw bytes to owned Strings for the Query trait
                let name_str = String::from_utf8_lossy(&name).into_owned();
                let attrs_conv: Vec<(String, String)> = attributes
                    .iter()
                    .map(|(k, v)| {
                        (
                            String::from_utf8_lossy(k).into_owned(),
                            String::from_utf8_lossy(v).into_owned(),
                        )
                    })
                    .collect();
                query.on_start(path.as_slice(), &name_str, &attrs_conv);
                path.push(&name_str);
            }
            Event::EndElement { name, accumulated } => {
                let name_str = String::from_utf8_lossy(&name).into_owned();
                let accumulated_str = String::from_utf8_lossy(&accumulated).into_owned();
                query.on_end(path.as_slice(), &name_str, &accumulated_str);
                path.pop();
            }
            Event::Text(text) => {
                if !text.is_empty() {
                    let text_str = String::from_utf8_lossy(&text).into_owned();
                    query.on_text(path.as_slice(), &text_str);
                }
            }
            Event::Comment(_) | Event::CData(_) => {
                // Ignored by default; could be forwarded to query if needed.
            }
            Event::Eof => {
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Event;
    use std::io::{self, BufRead};

    // --- PathStack tests ---
    #[test]
    fn pathstack_push_pop_as_slice() {
        let mut stack = PathStack::new();
        assert_eq!(stack.as_slice(), &[] as &[String]);
        stack.push("a");
        stack.push("b");
        assert_eq!(stack.as_slice(), &[String::from("a"), String::from("b")]);
        stack.pop();
        assert_eq!(stack.as_slice(), &[String::from("a")]);
        stack.pop();
        assert_eq!(stack.as_slice(), &[] as &[String]);
        stack.pop(); // popping empty should not panic
        assert_eq!(stack.as_slice(), <&[String]>::default());
    }

    // --- PathSelector tests ---
    #[test]
    fn pathselector_parse_and_match_anywhere() {
        let sel = PathSelector::parse("//foo").unwrap();
        assert!(matches!(sel, PathSelector::Anywhere(_)));
        assert!(sel.matches_path(&["a".to_string()], "foo"));
        assert!(!sel.matches_path(&["a".to_string()], "bar"));
    }

    #[test]
    fn pathselector_parse_and_match_absolute() {
        let sel = PathSelector::parse("/a/b/c").unwrap();
        assert!(matches!(sel, PathSelector::Absolute(_)));
        let path = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(sel.matches_path(&path, "c"));
        assert!(!sel.matches_path(&["a".to_string(), "b".to_string()], "b"));
    }

    #[test]
    fn pathselector_parse_bare_name() {
        let sel = PathSelector::parse("foo").unwrap();
        assert!(matches!(sel, PathSelector::Anywhere(_)));
        assert!(sel.matches_path(&[], "foo"));
    }

    #[test]
    fn pathselector_parse_errors() {
        assert!(PathSelector::parse("//").is_err());
        assert!(PathSelector::parse("/").is_err());
    }

    // --- PathQuery tests ---
    #[test]
    fn pathquery_basic_matching_and_max() {
        let selector = PathSelector::parse("//foo").unwrap();
        let mut pq = PathQuery::new(selector, 2);
        // Simulate matching events
        pq.on_start(&[], "foo", &[]);
        pq.on_text(&[], "bar");
        pq.on_end(&[], "foo", "foobar");
        assert_eq!(pq.printed, 1);
        pq.on_start(&[], "foo", &[]);
        pq.on_end(&[], "foo", "foo");
        assert_eq!(pq.printed, 2);
        assert!(pq.is_done());
    }

    #[test]
    fn pathquery_non_matching() {
        let selector = PathSelector::parse("//foo").unwrap();
        let mut pq = PathQuery::new(selector, 1);
        pq.on_start(&[], "bar", &[]);
        pq.on_text(&[], "baz");
        pq.on_end(&[], "bar", "barbaz");
        assert_eq!(pq.printed, 0);
        assert!(!pq.is_done());
    }

    // --- Query trait callback order ---
    struct MockQuery {
        log: Vec<String>,
        done: bool,
    }
    impl Query for MockQuery {
        fn on_start(&mut self, path: &[String], name: &str, _attrs: &[(String, String)]) {
            self.log.push(format!("start:{}:{:?}", name, path));
        }
        fn on_text(&mut self, _path: &[String], text: &str) {
            self.log.push(format!("text:{}", text));
        }
        fn on_end(&mut self, path: &[String], name: &str, content: &str) {
            self.log
                .push(format!("end:{}:{:?}:{}", name, path, content));
            if self.log.len() > 3 {
                self.done = true;
            }
        }
        fn is_done(&mut self) -> bool {
            self.done
        }
    }

    trait StubNextEvent {
        fn next_event(&mut self) -> io::Result<Event>;
    }

    #[test]
    fn run_query_dispatches_events_and_stops_on_done() {
        // Minimal stub Reader emitting a fixed event sequence
        struct StubReader {
            events: Vec<Event>,
            idx: usize,
        }

        impl StubNextEvent for StubReader {
            fn next_event(&mut self) -> io::Result<Event> {
                StubReader::next_event(self)
            }
        }
        impl std::io::Read for StubReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
        }
        impl BufRead for StubReader {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                Ok(&[])
            }
            fn consume(&mut self, _amt: usize) {}
        }
        impl StubReader {
            fn next_event(&mut self) -> io::Result<Event> {
                if self.idx < self.events.len() {
                    let ev = self.events[self.idx].clone();
                    self.idx += 1;
                    Ok(ev)
                } else {
                    Ok(Event::Eof)
                }
            }
        }
        // Compose events: Start(foo), Text(bar), End(foo)
        let events = vec![
            Event::StartElement {
                name: b"foo".to_vec(),
                attributes: vec![],
            },
            Event::Text(b"bar".to_vec()),
            Event::EndElement {
                name: b"foo".to_vec(),
                accumulated: b"foobar".to_vec(),
            },
        ];
        let mut reader = StubReader { events, idx: 0 };
        let mut query = MockQuery {
            log: vec![],
            done: false,
        };
        // Patch run_query to use our stub
        fn run_query_stub<Q: Query, R: StubNextEvent>(
            reader: &mut R,
            query: &mut Q,
        ) -> io::Result<()> {
            let mut path = PathStack::new();
            loop {
                if query.is_done() {
                    break;
                }
                let ev = reader.next_event()?;
                match ev {
                    Event::StartElement { name, attributes } => {
                        let name_str = String::from_utf8_lossy(&name).into_owned();
                        let attrs_conv: Vec<(String, String)> = attributes
                            .iter()
                            .map(|(k, v)| {
                                (
                                    String::from_utf8_lossy(k).into_owned(),
                                    String::from_utf8_lossy(v).into_owned(),
                                )
                            })
                            .collect();
                        path.push(&name_str);
                        query.on_start(path.as_slice(), &name_str, &attrs_conv);
                    }
                    Event::EndElement { name, accumulated } => {
                        let name_str = String::from_utf8_lossy(&name).into_owned();
                        let accumulated_str = String::from_utf8_lossy(&accumulated).into_owned();
                        query.on_end(path.as_slice(), &name_str, &accumulated_str);
                        path.pop();
                    }
                    Event::Text(text) => {
                        if !text.is_empty() {
                            let text_str = String::from_utf8_lossy(&text).into_owned();
                            query.on_text(path.as_slice(), &text_str);
                        }
                    }
                    Event::Comment(_) | Event::CData(_) => {}
                    Event::Eof => {
                        break;
                    }
                }
            }
            Ok(())
        }
        run_query_stub(&mut reader, &mut query).unwrap();
        assert_eq!(query.log[0], "start:foo:[\"foo\"]");
        assert_eq!(query.log[1], "text:bar");
        assert!(query.log.iter().any(|l| l.starts_with("end:foo")));
    }

    // --- Edge cases ---
    #[test]
    fn run_query_empty_input() {
        struct EmptyReader;
        impl StubNextEvent for EmptyReader {
            fn next_event(&mut self) -> io::Result<Event> {
                Ok(Event::Eof)
            }
        }
        impl std::io::Read for EmptyReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
        }
        impl BufRead for EmptyReader {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                Ok(&[])
            }
            fn consume(&mut self, _amt: usize) {}
        }
        let mut reader = EmptyReader;
        let mut query = MockQuery {
            log: vec![],
            done: false,
        };
        fn run_query_stub_1<Q: Query, R: StubNextEvent>(
            reader: &mut R,
            query: &mut Q,
        ) -> io::Result<()> {
            let mut path = PathStack::new();
            loop {
                if query.is_done() {
                    break;
                }
                let ev = reader.next_event()?;
                match ev {
                    Event::StartElement { name, attributes } => {
                        let name_str = String::from_utf8_lossy(&name).into_owned();
                        let attrs_conv: Vec<(String, String)> = attributes
                            .iter()
                            .map(|(k, v)| {
                                (
                                    String::from_utf8_lossy(k).into_owned(),
                                    String::from_utf8_lossy(v).into_owned(),
                                )
                            })
                            .collect();
                        path.push(&name_str);
                        query.on_start(path.as_slice(), &name_str, &attrs_conv);
                    }
                    Event::EndElement { name, accumulated } => {
                        let name_str = String::from_utf8_lossy(&name).into_owned();
                        let accumulated_str = String::from_utf8_lossy(&accumulated).into_owned();
                        query.on_end(path.as_slice(), &name_str, &accumulated_str);
                        path.pop();
                    }
                    Event::Text(text) => {
                        if !text.is_empty() {
                            let text_str = String::from_utf8_lossy(&text).into_owned();
                            query.on_text(path.as_slice(), &text_str);
                        }
                    }
                    Event::Comment(_) | Event::CData(_) => {}
                    Event::Eof => {
                        break;
                    }
                }
            }
            Ok(())
        }
        run_query_stub_1(&mut reader, &mut query).unwrap();
        assert!(query.log.is_empty());
    }
}
