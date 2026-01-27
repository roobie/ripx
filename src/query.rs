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

    fn on_end(&mut self, path: &[String], name: &str);

    fn is_full(&mut self) -> bool;
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
    count: usize,       // count accumulated matches
    done: bool,
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
            count: 0,
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
                self.count += 1;
            }
            self.in_match += 1;
        } else if self.in_match > 0 {
            self.in_match += 1;
        }
    }

    fn on_text(&mut self, _path: &[String], text: &str) {
        self.count += 1;
        if self.in_match > 0 && !self.done {
            self.buf.push_str(text);
        }
    }

    fn on_end(&mut self, path: &[String], _name: &str) {
        if self.done || self.in_match == 0 {
            return;
        }

        self.in_match -= 1;

        // leaving the root element of the match?
        if self.in_match == 0 && path.len() == self.match_depth {
            // one complete match collected in buf
            println!("{}", self.buf.trim());
            self.printed += 1;
            if self.printed >= self.max {
                self.done = true;
            }
        }
    }

    fn is_full(&mut self) -> bool {
        self.count >= self.max
    }
}

/// Drive the reader and query together.
pub fn run_query<R: BufRead, Q: Query>(
    reader: &mut Reader<R>,
    query: &mut Q,
) -> std::io::Result<()> {
    let mut path = PathStack::new();

    loop {
        if query.is_full() {
            break;
        }
        println!("{:?}", path);
        match reader.next_event()? {
            Event::StartElement { name, attributes } => {
                path.push(&name);
                query.on_start(path.as_slice(), &name, &attributes);
            }
            Event::EndElement { name } => {
                println!("{:?}", path);
                query.on_end(path.as_slice(), &name);
                path.pop();
            }
            Event::Text(text) => {
                println!("{:?}", path);
                // You might want to trim whitespace here; for MVP, pass through.
                if !text.is_empty() {
                    query.on_text(path.as_slice(), &text);
                }
            }
            Event::Comment(_) | Event::CData(_) => {
                println!("{:?}", path);
                // Ignored by default; could be forwarded to query if needed.
            }
            Event::Eof => {
                println!("{:?}", path);
                break;
            }
        }
    }

    Ok(())
}
