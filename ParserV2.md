Below is a revised design incorporating your decisions in (1)–(5). I’ll present it as a self-contained updated spec.

---

## 1. Parser Goals and Scope

### 1.1 Intended Use

- Streaming XML processing for very large (multi‑GB / TB) files.
- Security‑sensitive or resource‑constrained systems needing:
  - Predictable, bounded memory use.
  - No entity expansion (XXE safe).
- Applications that want structural events and raw byte content, not full XML validation.

### 1.2 Non-Goals

- No XML validation, DTD/Schema processing.
- No entity or character reference expansion.
- No Unicode handling beyond “opaque byte sequences.”
- No DOM or random access tree building.

---

## 2. Public API Overview

### 2.1 Event Model

The parser exposes a pull interface: caller repeatedly invokes `next_event()` to receive events.

```rust
pub enum EventType {
    StartElement,
    EndElement,
    Text,
    Comment,
    CData,
    ProcessingInstruction,
    Fault, // Structural / limit fault, parser can often continue
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnexpectedEof,
    UnclosedElement,
    OrphanedEndElement,
    MismatchedQuotes,
    TooManyAttributes,
    DepthLimitExceeded,
    TokenTooLong,
    AttributeNameTooLong,
    AttributeValueTooLong,
    NameTooLong,
    TextTooLong,
    CommentTooLong,
    CDataTooLong,
    ProcessingInstructionTooLong,
    InvalidStructure,
    IoError,
}
```

Attributes:

```rust
pub struct Attribute<'a> {
    pub name: &'a [u8],   // Raw bytes (no decoding)
    pub value: &'a [u8],  // Raw bytes (no decoding)
}
```

Instead of exposing a borrowed slice of `Attribute<'a>` directly, attributes are accessed via a lightweight view that is built from internal tables:

```rust
pub struct Attributes<'a> {
    // Opaque wrapper; methods resolve internal indices into live slices
    // over the input buffer or scratch buffers.
    // See "Attribute Storage" section for details.
}

impl<'a> Attributes<'a> {
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn get(&self, index: usize) -> Option<Attribute<'a>>;
    // Optional: iter() etc.
}
```

Event structure:

```rust
pub struct Event<'a> {
    pub event_type: EventType,
    pub data: &'a [u8],        // For name/text/comment/CDATA/PI as appropriate
    pub is_continuation: bool, // For chunked Text/Comment/CDATA/PI if split
    pub error: Option<ErrorCode>, // Set when event_type == Fault
    pub attributes: Attributes<'a>, // Only meaningful for StartElement
}
```

Lifetime guarantee:

- All slices in an `Event<'a>` (including everything reachable via `Attributes<'a>`) remain valid only until the next `next_event()` call.

### 2.2 Parser Construction

```rust
pub struct ParserLimits {
    pub max_depth: usize,
    pub max_attributes: usize,
    pub max_name_len: usize,
    pub max_attr_name_len: usize,
    pub max_attr_value_len: usize,
    pub max_text_chunk_len: usize,
    pub max_text_total_len: Option<usize>, // Optional absolute per-run text limit
    pub max_comment_chunk_len: usize,
    pub max_comment_total_len: Option<usize>,
    pub max_cdata_chunk_len: usize,
    pub max_cdata_total_len: Option<usize>, // Hard per-CDATa section limit
    pub max_pi_chunk_len: usize,
    pub max_pi_total_len: Option<usize>,
}

pub struct Parser<R: std::io::Read> {
    // ...
}

impl<R: std::io::Read> Parser<R> {
    pub fn new(reader: R, limits: ParserLimits) -> Self {
        // Validates limits; allocates internal buffers with fixed capacity
        // based on the configured maxima.
    }

    pub fn next_event<'a>(&'a mut self) -> std::io::Result<Event<'a>> {
        // Advances the internal state machine, returns one event.
        // I/O errors are returned via Err(io::Error).
        // Syntactic/limit faults are surfaced via EventType::Fault.
    }
}
```

Notes:

- `ParserLimits` are validated at construction (e.g., must be > 0 where required, ≤ internal hard maxima).
- All per-parser heap allocations are done once in `new()` and reused.
- `Result` is reserved for I/O failures (e.g., underlying reader error). EOF is represented as an `EventType::Eof` event, not an `Err`.

---

## 3. Internal Structures

### 3.1 Input Buffer

```rust
struct InputBuffer {
    buf: Box<[u8]>,  // Fixed size, e.g., 8–64KB
    start: usize,    // Current cursor start
    end: usize,      // End of valid data
}
```

- On underflow, remaining data is shifted to the front and refilled from the `Read` source.
- Parser cursor is an index in `[start, end)`.
- Sliding the buffer only happens between events, never in a way that invalidates slices exposed in the current `Event<'a>`.

### 3.2 Scratch Buffers

```rust
struct ScratchBuffers {
    name: Vec<u8>,        // Bounded by limits.max_name_len
    attr_name: Vec<u8>,   // Bounded by limits.max_attr_name_len
    attr_value: Vec<u8>,  // Bounded by limits.max_attr_value_len
    text: Vec<u8>,        // Bounded by limits.max_text_chunk_len
    comment: Vec<u8>,     // Bounded by limits.max_comment_chunk_len
    cdata: Vec<u8>,       // Bounded by limits.max_cdata_chunk_len
    pi: Vec<u8>,          // Bounded by limits.max_pi_chunk_len
}
```

- Each `Vec<u8>`:
  - Created once with `with_capacity(limit)`.
  - Never grown beyond that capacity.
  - Cleared and reused between tokens or chunks.

### 3.3 Element Stack

Element names are always copied into scratch to avoid lifetime issues due to sliding the input buffer.

```rust
struct ElementFrame {
    // Element name is always stored in the `name` scratch buffer.
    // The parser manages the offsets or compacts as needed.
    name_start: usize,
    name_len: usize,
}

struct ElementStack {
    frames: Vec<ElementFrame>, // capacity = limits.max_depth
}
```

- On `StartElement`, a frame is pushed containing a name slice into the `name` scratch buffer.
- On `EndElement`, a frame is popped; name is used for comparison and, if desired, exposed through `Event::data`.

The parser may copy the element name for the event’s `data` field into scratch (or reuse offsets if they are still valid), respecting `max_name_len`.

### 3.4 Attribute Storage

Attributes are parsed eagerly for each `StartElement`.

```rust
enum NameBufferKind {
    Input,
    NameScratch,
    AttrNameScratch,
    AttrValueScratch,
}

struct InternalAttribute {
    name_buffer: NameBufferKind,
    name_start: usize,
    name_len: usize,
    value_buffer: NameBufferKind,
    value_start: usize,
    value_len: usize,
}

struct AttributeTable {
    entries: Vec<InternalAttribute>, // capacity = limits.max_attributes
}
```

- `InternalAttribute`s store buffer kind + offset/length into either the input buffer or one of the scratch buffers.
- Per `StartElement`, attributes are parsed into `AttributeTable.entries`.

To expose them without per‑event allocations or unsafe self‑references:

```rust
struct AttributesInner<'a> {
    entries: &'a [InternalAttribute],
    input_buf: &'a [u8],
    scratch: &'a ScratchBuffers,
}

pub struct Attributes<'a> {
    inner: AttributesInner<'a>,
}

impl<'a> Attributes<'a> {
    fn resolve(
        &self,
        kind: NameBufferKind,
        start: usize,
        len: usize,
    ) -> &'a [u8] {
        match kind {
            NameBufferKind::Input => &self.inner.input_buf[start..start+len],
            NameBufferKind::NameScratch => &self.inner.scratch.name[start..start+len],
            NameBufferKind::AttrNameScratch => {
                &self.inner.scratch.attr_name[start..start+len]
            }
            NameBufferKind::AttrValueScratch => {
                &self.inner.scratch.attr_value[start..start+len]
            }
        }
    }

    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<Attribute<'a>> {
        let ia = self.inner.entries.get(idx)?;
        Some(Attribute {
            name: self.resolve(ia.name_buffer, ia.name_start, ia.name_len),
            value: self.resolve(ia.value_buffer, ia.value_start, ia.value_len),
        })
    }
}
```

- `Attributes<'a>` is constructed freshly inside `next_event()` from internal references and passed by value in the `Event<'a>`.
- No additional heap allocation is required, only references and small structs.

---

## 4. Algorithmic Overview

### 4.1 Parsing State Machine

```rust
enum State {
    Data,                // Outside tags, collecting text
    TagOpen,             // Just saw '<'
    StartTagName,        // Reading element name
    EndTagName,          // Reading end tag name
    InsideStartTag,      // After name, before '>' or '/>'
    AttrName,            // Reading attribute name
    AttrBeforeValue,     // After '=', before attribute value
    AttrValue,           // Reading quoted attribute value
    CommentStart,        // After '<!', matching '--'
    CommentBody,         // Inside <!-- ... -->
    CDataStart,          // Matching <![CDATA[
    CDataBody,           // Inside CDATA
    PiStart,             // After '<?'
    PiBody,              // Inside processing instruction
    SkipDoctype,         // Inside <!DOCTYPE ... >, with bracket depth tracking
    ErrorRecovery,       // Scanning forward after fault
    Eof,
}
```

`next_event()`:

1. Reads bytes from the input buffer.
2. Drives the state machine.
3. Once a complete logical event (including chunked pieces) is formed, returns it.

---

## 5. Behavior by Construct

### 5.1 General Rules

- Raw bytes only:
  - No UTF‑8 validation.
  - Names and values are arbitrary byte sequences (subject to length limits).
  - No entity expansion (`&amp;` remains `&amp;`).
- No validation:
  - The parser does not enforce XML name rules, character classes, or well‑formedness beyond simple structure and configured limits.
- Fault handling:
  - On malformed structures or limit violations, emit an `EventType::Fault` with `error = Some(code)` and an appropriate `data` payload where applicable.
  - Attempt to recover where possible and continue producing events.

### 5.2 Text Content (`Text` Events)

- Bytes outside tags (`<...>`) are returned as `Text`.
- Accumulation:
  - Bytes are copied into the `text` scratch buffer until:
    - `limits.max_text_chunk_len` reached, or
    - Optional absolute text limit (`max_text_total_len`) is reached (see below), or
    - `<` encountered (start of next tag), or
    - EOF.
- When `max_text_chunk_len` is reached:
  - Emit `Text` with `is_continuation = true`.
  - Continue reading text and emitting more `Text` events until tag/EOF.

Optional absolute text limit:

- If `limits.max_text_total_len` is `Some(N)`, the parser maintains a per‑run (or per contiguous text region, depending on desired semantics) counter.
- If the cumulative text bytes exceed `N`:
  - Emit `Fault` with `error = Some(TextTooLong)`.
  - Enter `ErrorRecovery` to find the next structural `<` or EOF, thus avoiding reading arbitrarily until EOF in pathological streams.
- If `max_text_total_len` is `None`, no absolute limit is enforced beyond chunk size.

### 5.3 Start Elements (`<name ...>`)

- On `<` in `Data` state → `TagOpen`.
- If next char is not `/`, `!`, or `?`, treat as start tag:
  - Enter `StartTagName`, accumulate element name into `name` scratch buffer (always copied).
  - Enforce `limits.max_name_len`; on overflow:
    - Emit `Fault(NameTooLong)`.
    - Element name is truncated to the first `max_name_len` bytes for further use (stack, events, comparisons), or the tag is skipped entirely (implementation choice must be documented).
  - On whitespace, `/`, or `>`:
    - Finalize name.
    - Push to element stack if within depth limit.
    - Move to `InsideStartTag` or emit `StartElement` if immediate `>`.

Depth limit:

- On pushing a new `ElementFrame`:
  - If `stack.len() == limits.max_depth`:
    - Emit `Fault(DepthLimitExceeded)`.
    - **Do not** push the frame.
    - Treat this element as syntactically skipped:
      - Skip until the matching `>` (accounting for attributes, `/` before `>`, etc.).
      - Ignore its matching end tags (treat any `</name>` as `OrphanedEndElement` if still unmatched).
    - Resume parsing after the skipped start tag.

Attributes (eager):

- In `InsideStartTag`:
  - Skip whitespace.
  - If next is `>` → emit `StartElement`, go to `Data`.
  - If next is `/` then `>` → self-closing:
    - Emit `StartElement`.
    - Immediately emit matching synthetic `EndElement` on a subsequent `next_event()` (or inline as two sequential events).
  - Else interpret as attribute name → `AttrName`.

- `AttrName`:
  - Accumulate into `attr_name` scratch up to `max_attr_name_len`.
  - On `=` (after optional whitespace) move to `AttrBeforeValue`.
  - If limit exceeded:
    - Emit `Fault(AttributeNameTooLong)`.
    - Skip this attribute (consume until matching value end) and do not add it to the table.

- `AttrBeforeValue`:
  - Expect `'` or `"`; record which quote.
  - Enter `AttrValue`.

- `AttrValue`:
  - Accumulate into `attr_value` scratch up to `max_attr_value_len`.
  - On closing quote:
    - Add attribute to `AttributeTable` if `entries.len() < limits.max_attributes`.
    - Else:
      - Emit `Fault(TooManyAttributes)`.
      - Skip this and any subsequent attributes for this start tag until `>` or `/>`.

Overflow cases:

- Too many attributes:
  - Emit `Fault(TooManyAttributes)`.
  - Ignore additional attributes syntactically (consume them but don’t store).
- Attribute name/value too long:
  - Emit appropriate `Fault(AttributeNameTooLong / AttributeValueTooLong)`.
  - Skip that attribute; do not add it.

On completing `StartElement`:

- Build an `Attributes<'a>` view from:
  - A slice of `AttributeTable.entries`.
  - References to `InputBuffer` and `ScratchBuffers`.
- Clear `AttributeTable` for the next `StartElement`.

### 5.4 End Elements (`</name>`)

Simple, precise strategy:

1. On `<` followed by `/`:
   - Enter `EndTagName`, accumulate into `name` scratch (or re-use if small).
   - Enforce `max_name_len` as for `StartTagName`, potentially emitting `Fault(NameTooLong)` and truncating.

2. On `>`:
   - If stack is empty:
     - Emit `Fault(OrphanedEndElement)`; ignore this end tag structurally.
   - Else:
     - Compare parsed name to the top of the stack’s name:
       - If equal:
         - Pop stack.
         - Emit `EndElement` event.
       - If not equal:
         - Emit `Fault(InvalidStructure)` (mismatched end).
         - Pop frames while names do not match:
           - For each popped frame, emit a **synthetic** `EndElement` corresponding to that frame.
           - If a matching name is eventually found:
             - Pop it and **do not** emit a further event for the original mismatching end tag (it has been “consumed” for recovery).
           - If the stack becomes empty without finding a match:
             - Treat the original end tag as `OrphanedEndElement`; emit `Fault(OrphanedEndElement)` (if not already emitted) and skip it.

This strategy preserves a consistent nesting model for consumers after recovery.

EOF with non-empty stack is handled in section 6.2.

### 5.5 Comments: `<!-- ... -->`

- On `<` then `!` and `--`:
  - Enter `CommentBody`.

- `CommentBody`:
  - Accumulate into `comment` scratch until:
    - `-->` is found → emit `Comment` (maybe chunked).
    - `limits.max_comment_chunk_len` reached → emit `Comment` with `is_continuation = true`, clear buffer, and keep accumulating.
    - Optional absolute comment limit (`max_comment_total_len`) is exceeded:
      - Emit `Fault(CommentTooLong)`.
      - Enter `ErrorRecovery` (skip until next `<` or EOF).
    - EOF before `-->`:
      - Emit `Fault(UnexpectedEof)`.
      - Enter `Eof` state.

- If `<!` is not followed by `--` and not a valid CDATA/DOCTYPE start:
  - Emit `Fault(InvalidStructure)`.
  - Enter `ErrorRecovery`.

…continuing from CDATA behavior…

---

### 5.6 CDATA: `<![CDATA[ ... ]]>` (continued)

- `CDataBody`:
  - Accumulate into `cdata` scratch until:
    - `]]>` is seen → emit `CData`.
    - `limits.max_cdata_chunk_len` reached:
      - Emit `CData` with `is_continuation = true`.
      - Clear buffer and continue accumulating.
    - Optional absolute per‑section limit (`max_cdata_total_len`) is exceeded:
      - Emit `Fault(CDataTooLong)`.
      - Enter `ErrorRecovery` to avoid reading arbitrarily to EOF:
        - Discard bytes until next `<` or EOF.
        - Then resume normal parsing from `TagOpen` or `Eof`.
    - EOF before `]]>`:
      - Emit `Fault(UnexpectedEof)`.
      - Enter `Eof` state.

- CDATA content is verbatim bytes; the caller decides how to interpret or decode them.

### 5.7 Processing Instructions: `<? ... ?>`

- On `<` then `?`:
  - Enter `PiStart`/`PiBody` (the parser does not interpret target vs content; `data` is the PI payload minus `<?` and `?>`).

- `PiBody`:
  - Accumulate into `pi` scratch until:
    - `?>` is seen → emit `ProcessingInstruction`.
    - `limits.max_pi_chunk_len` reached:
      - Emit `ProcessingInstruction` with `is_continuation = true`.
      - Clear buffer and continue.
    - Optional absolute PI limit (`max_pi_total_len`) is exceeded:
      - Emit `Fault(ProcessingInstructionTooLong)`.
      - Enter `ErrorRecovery` (skip until next `<` or EOF).
    - EOF before `?>`:
      - Emit `Fault(UnexpectedEof)`.
      - Enter `Eof` state.

### 5.8 DOCTYPE / DTD: `<!DOCTYPE ...>`

If `<!DOCTYPE` is detected:

- Emit `Fault(InvalidStructure)` **once** for that declaration (the parser does not support DTDs).
- Enter `SkipDoctype` state with a `bracket_depth` counter:

  1. Initialize `bracket_depth = 0`.
  2. For each byte:
     - If `[`, increment `bracket_depth` (saturating at some small max or `usize::MAX`).
     - If `]`, decrement `bracket_depth` if > 0.
     - If `>` and `bracket_depth == 0`, leave `SkipDoctype`, return to `Data`, and continue normal parsing.
  3. If EOF is reached before completing:
     - Emit `Fault(UnexpectedEof)`.
     - Enter `Eof`.

- No DTD parsing, no entity or external subset handling is performed.

---

## 6. Fault Handling and Recovery

### 6.1 Fault Conditions and `Fault` Events

Representative faults:

- Structural:
  - `UnexpectedEof` (mid‑tag, mid‑comment, etc.).
  - `UnclosedElement` (EOF with open stack).
  - `OrphanedEndElement`.
  - `MismatchedQuotes`.
  - `InvalidStructure` (bad `<!`, malformed tags, etc.).
- Limits:
  - `DepthLimitExceeded`.
  - `TooManyAttributes`.
  - `NameTooLong`, `AttributeNameTooLong`, `AttributeValueTooLong`.
  - `TextTooLong`, `CommentTooLong`, `CDataTooLong`, `ProcessingInstructionTooLong`.
  - `TokenTooLong` (for generic token overruns that don’t fit a more specific code).
- I/O:
  - `IoError` is represented in `ErrorCode`, but actual underlying I/O failures are returned as `Err(io::Error)` from `next_event()`. `Fault(IoError)` is reserved for situations where the parser chooses to surface I/O problems as recoverable conditions (if ever used).

On encountering a fault:

- Emit an `Event` with:
  - `event_type = EventType::Fault`.
  - `error = Some(code)`.
  - `data`:
    - Either empty `&[]`, or (where appropriate) a slice of the offending token truncated to fit within configured scratch limits.
- Then either:
  - Continue parsing from a defined structural state (for recoverable faults), or
  - Transition to `Eof` (for unrecoverable cases like EOF mid‑construct).

By convention:

- `event_type == Fault` ⇒ `error.is_some()` is guaranteed.
- Other `event_type`s (`StartElement`, `Text`, etc.) have `error == None`.

### 6.2 EOF Handling and Open Elements

At EOF:

- If the input ends cleanly while in `Data` or similar neutral states and the element stack is empty:
  - Emit a single `Event` with `event_type = Eof`.
  - Subsequent `next_event()` calls may either:
    - Keep returning `Eof`, or
    - Return `Ok(Event { event_type: Eof, … })` once and then `Err` akin to “already finished” (implementation-specific but should be documented).

- If the element stack is **not empty** at EOF:
  - Emit a single `Fault` event with:
    - `event_type = Fault`.
    - `error = Some(UnclosedElement)`.
  - Do **not** emit synthetic `EndElement` events.
  - After that fault, emit a final `Eof` event.
  - No further events are produced.

This keeps behavior simple and predictable: consumers know the document ended with unclosed elements but don’t have to deal with synthetic closures.

### 6.3 Error Recovery Strategy

When the parser enters `ErrorRecovery` after a fault:

1. Switch to `ErrorRecovery` state.
2. Consume and discard bytes until:
   - The next `<` is seen (potential start of a structural construct), or
   - EOF is reached.
3. On finding `<`:
   - Set state to `TagOpen` and resume normal parsing on the next `next_event()` call.
4. On EOF:
   - Emit `Fault(UnexpectedEof)` if not already emitted for this episode (optional).
   - Enter `Eof`.

In `ErrorRecovery`:

- Only a **single** `Fault` event is emitted for the episode that triggered entry into this state (the original one).
- The skipped region is not preserved; `Event::data` is typically an empty slice for such faults to avoid large memory use.

---

## 7. Security and Safety Considerations

- **Bounded heap use:**
  - All `Vec`s have fixed capacities derived from `ParserLimits`.
  - No runtime growth; buffers are reused with `.clear()`.
- **No unbounded logical reads:**
  - All token accumulations are bounded by:
    - Chunk limits (e.g., `max_cdata_chunk_len`).
    - Optional absolute per‑construct limits (e.g., `max_cdata_total_len`), which trigger a `*TooLong` fault and `ErrorRecovery` instead of reading to EOF in vain.
- **Slice safety:**
  - Element names that survive across events are always copied into scratch buffers so they are not invalidated by sliding the input buffer.
  - Attributes are resolved lazily through `Attributes<'a>` from stable internal indices.
- **Indexing safety:**
  - Use normal Rust indexing and internal bound checks; no `IntegerOverflow` error code (removed from API).
  - Out‑of‑range indices would indicate a logic bug and can safely panic in debug builds; production code should be thoroughly tested.
- **No entity expansion or external fetches:**
  - No DTD processing or external entity loading.
  - No entity or character reference expansion; all content is raw bytes.
- **Depth and attribute limits:**
  - Prevent excessive nesting and attribute floods from stressing downstream systems or parser internals.

---

## 8. Summary

Revised design highlights:

- `EventType::Fault` clearly separates parser faults from normal events.
- Attributes are exposed via `Attributes<'a>` accessors, avoiding lifetime issues.
- Element names are always copied into scratch, making buffer sliding safe.
- Text, Comment, CDATA, and PI use a **uniform chunking model** with optional absolute per‑construct limits (`*TooLong` + `ErrorRecovery`) to avoid reading to EOF in pathological cases (e.g., unbounded CDATA in a TB file).
- End‑tag mismatch and EOF with open elements are handled with simple, precise rules, and DOCTYPE is consistently skipped with a bracket‑depth heuristic.