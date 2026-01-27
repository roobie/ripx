# ripx

`ripx` aims to be a small, single-binary, very high-performance, non‑validating, non-decoding XML query tool and library focused on streaming single‑pass processing of very large XML files (multi‑GB / TB). It emphasizes low and predictable memory use, simplicity, fault tolerance, and predictable run time.

## Goals
- Stream XML from any BufRead source; no DOM, no strict validation.
- Support simple, fast queries (e.g. `//item`, `/root/items/item[@id=123]`) and callbacks for matched subtrees.
- Minimal XML features initially: start/end elements, attributes, text, comments, CDATA.
- Robust best‑effort fault handling: recover from malformed regions and continue.
- Minimal decoding in the parser - operate on bytes directly - not UTF8

## Task tracking

**IMPORTANT**: This project manages tasks using the beans CLI. All contributors and automated agents are required to run `beans prime` and follow the directives it prints (create and update beans for work, include bean files in commits, and update statuses as work progresses).

## Quickstart (development)
Build and run tests with Rust/Cargo:
```bash
cargo build --release
cargo test
```

Example (hypothetical) CLI usage:
```bash
# query `large-file.xml` for an element named `item` with an attribute `id` equal to 12345 and stop when found.
# the tool should print to stdout the full element that was found
# it could potentially print to stderr info about offset and other stuff
ripx large-file.xml --path "//item" --attr-eq-id="12345" --short-circuit
```

## Future features
- Parallel reading of input file (might require multi-pass or the below indexing)
- Indexing to improve performance, e.g. document structure at offset

## License

> GPLv3
