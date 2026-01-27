## High‑level Implementation Plan (Rust)

Assumptions:
- Input: UTF‑8 only.
- Features: start/end elements, attributes, text, minimal comments/CDATA.
- Error handling: best effort, “skip ahead and continue”.
- Goal: streaming queries on multi‑GB files (single‑pass).

---

## 1. Crate Structure

- `src/lib.rs`
  - Re‑exports and top‑level types.
- `src/reader.rs` (modified to emit raw bytes for text, attributes, and accumulated markup)
  - Streaming XML reader + iterator over events.
- `src/tokenizer.rs`
  - Low‑level byte scanning and tokenization.
- `src/query.rs` (adapted to convert bytes to strings for legacy Query trait usage)
  - Simple query predicates + path tracking.
- `src/bin/xmlq.rs` (optional)
  - CLI: `xmlq --path "//item" file.xml`.

---

## 2. Core Data Types

### Event API

```rust
pub enum Event {
    StartElement {
        name: String,
        attributes: Vec<(String, String)>,
    },
    EndElement {
        name: String,
    },
    Text(String),
    Comment(String), // minimal
    CData(String),   // minimal
    Eof,
}
```

- Use `String` for simplicity; optimize later if needed.
- Text can be chunked if desired (not mandatory initially).

### Reader

```rust
pub struct Reader<R: std::io::BufRead> {
    source: R,
    buf: Vec<u8>,
    pos: usize,
    end: usize,
    // small state for inside-tag / outside-tag, etc.
}
```

- Maintain a refillable buffer and indices (`pos`, `end`).
- Expose:

```rust
impl<R: BufRead> Reader<R> {
    pub fn from_reader(source: R) -> Self { /* ... */ }

    pub fn next_event(&mut self) -> std::io::Result<Event> { /* ... */ }
}
```

---

## 3. Tokenization Strategy

### States

Implement a tiny state machine:

1. **OutsideTag** (text):
   - Scan until `<`.
   - Emit `Text` (if non‑empty, trim or preserve as needed).
2. **TagOpen**:
   - After `<`, peek:
     - `/` → end element.
     - `!` → comment or CDATA.
     - `?` → processing instruction (can skip).
     - otherwise → start element.
3. **StartElement**:
   - Parse name: `[A-Za-z_:][A-Za-z0-9_\-.:]*`.
   - Then loop parsing attributes until `>` or `/>`.
4. **EndElement**:
   - Parse name, then expect `>`, emit `EndElement`.
5. **Comment / CDATA (minimal)**:
   - Comment: assume `<!-- ... -->`. Find next `-->` or EOF and treat as one token.
   - CDATA: assume `<![CDATA[ ... ]]>`. Find next `]]>`.

### Coarse Error Handling

- If you hit unexpected bytes or an unterminated construct:
  - Skip forward to next `<` and return a generic `io::ErrorKind::InvalidData` once.
  - Continue parsing from new position.
- If attributes are malformed:
  - Try to find next `>` and bail on that tag, emit nothing or a partial `StartElement`.

---

## 4. Attribute Parsing

Inside `StartElement`:

1. After the element name, repeatedly:
   - Skip whitespace.
   - If `>` or `/>`, stop.
   - Parse attribute name (same lexical rules as element name).
   - Expect `=`, skip possible whitespace.
   - Expect quote `" or '`.
   - Read until matching quote (no complicated entity handling except basic `&lt;`, etc.).
2. Replace basic entities with a simple function:

```rust
fn decode_basic_entities(s: &str) -> String {
    s.replace("&lt;", "<")
     .replace("&gt;", ">")
     .replace("&amp;", "&")
     .replace("&apos;", "'")
     .replace("&quot;", "\"")
}
```

This is slow but simple; optimize later.

---

## 5. Query Layer

### Path Tracking

Maintain a stack of element names:

```rust
pub struct PathStack {
    stack: Vec<String>,
}

impl PathStack {
    pub fn push(&mut self, name: &str) { self.stack.push(name.to_owned()); }
    pub fn pop(&mut self) { self.stack.pop(); }

    pub fn as_slice(&self) -> &[String] {
        &self.stack
    }
}
```

In your main stream loop:

```rust
let mut path = PathStack::new();
while let Ok(event) = reader.next_event() {
    match event {
        Event::StartElement { name, attributes } => {
            path.push(&name);
            query.on_start(path.as_slice(), &name, &attributes);
        }
        Event::EndElement { name } => {
            query.on_end(path.as_slice(), &name);
            path.pop();
        }
        Event::Text(text) => {
            query.on_text(path.as_slice(), &text);
        }
        Event::Eof => break,
        _ => {}
    }
}
```

### Query Predicate Trait

```rust
pub trait Query {
    fn on_start(
        &mut self,
        path: &[String],
        name: &str,
        attrs: &[(String, String)],
    );
    fn on_text(&mut self, path: &[String], text: &str);
    fn on_end(&mut self, path: &[String], name: &str);
}
```

You can implement:

- **Path filter query**: match `path == ["root", "items", "item"]`.
- **Attr filter**: check `attrs.iter().any(|(k, v)| k == "id" && v == "42")`.

---

## 6. Tiny Query “Language” (MVP)

Support a minimal syntax like:

- `//item` → match any `item` element.
- `/root/items/item` → exact path.

Implementation:

1. Parse query string into:
   ```rust
   enum PathSelector {
       Anywhere(String),      // //name
       Absolute(Vec<String>), // /a/b/c
   }
   ```
2. `match_start(path, name)`:
   - For `Anywhere(n)`: `if name == n`.
   - For `Absolute(p)`: `if path == p`.

Output could be:

- Stream of matching elements’ text or raw XML snippets.
- Or callbacks invoked when inside a matching subtree.

---

## 7. CLI Skeleton (Optional)

```rust
fn main() -> std::io::Result<()> {
    let query_str = std::env::args().nth(1).expect("need query");
    let file = std::fs::File::open(std::env::args().nth(2).unwrap())?;
    let reader = std::io::BufReader::new(file);

    let mut xml_reader = Reader::from_reader(reader);
    let selector = PathSelector::parse(&query_str).unwrap();
    let mut query = PathQuery::new(selector);

    run_query(&mut xml_reader, &mut query)?;
    Ok(())
}
```

---

## 8. Implementation Order

1. `Reader` + `next_event` for:
   - Text + simple start/end elements without attrs.
2. Add attributes.
3. Add minimal comments/CDATA.
4. Add `PathStack` and `Query` trait.
5. Add `PathSelector` and a simple `PathQuery` implementation.
6. Add CLI (if needed) and test on big files.

If you’d like, I can next write a more concrete `Reader::next_event` skeleton showing the main state machine in Rust.
