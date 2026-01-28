# High-Performance XML Parser: 3-5 GB/s Implementation Plan

## Executive Summary

**Current Performance**: ~37.5 MB/s (300MB in 8s)
**Target Performance**: 3-5 GB/s (80-130x improvement)
**Primary Strategy**: Parallelization + Zero-Copy + SIMD optimizations

To achieve 3-5 GB/s, we need three major improvements:

1. **Parallelization** (8-16x speedup): Chunk-based parallel processing with Rayon
2. **Zero-copy design** (2-3x speedup): Memory-mapped I/O, slice-based parsing
3. **SIMD optimizations** (1.5-2x additional): Beyond existing memchr usage

**Estimated Performance Path**:
- Phase 1 (Parallel): 37.5 MB/s → 300-400 MB/s (8-10x)
- Phase 2 (Zero-copy): 400 MB/s → 900 MB/s - 1.2 GB/s (2.5-3x)
- Phase 3 (Optimized): 1.2 GB/s → **3-5 GB/s** (2.5-4x)

---

## Current Architecture Bottlenecks

### Key Issues

1. **Single-threaded**: Parser<R> requires `&mut self`, cannot share across threads
2. **Extensive copying**:
   - InputBuffer uses `copy_within()` for sliding window (input_buffer.rs:75)
   - ScratchBuffers copy data from input (7 separate Vec<u8> buffers)
   - Element names copied multiple times
3. **Owned reader**: Parser owns `Read` source, can't use memory-mapped slices
4. **Event lifetimes**: Events borrow from parser (`Event<'a>`), preventing parallelism
5. **Stateful coupling**: Element stack, scratch buffers, and input buffer are tightly integrated

### Current File Structure

- `/workspace/src/parser.rs`: Main Parser<R: io::Read> (664 lines)
- `/workspace/src/input_buffer.rs`: Sliding window buffer (129 lines)
- `/workspace/src/scratch.rs`: 7 scratch Vec<u8> buffers (152 lines)
- `/workspace/src/element_stack.rs`: Depth tracking (148 lines)
- `/workspace/src/tokenizer.rs`: Stateless tokenization (443 lines)
- `/workspace/src/main.rs`: Single-threaded filtering (237 lines)

---

## Parallel Architecture Design

```
                  ┌─────────────────────────────────┐
                  │   Memory-Mapped Input File      │
                  │   (memmap2 crate)               │
                  └─────────────────────────────────┘
                               │
                               ▼
                  ┌─────────────────────────────────┐
                  │   Chunk Splitter                │
                  │   (find safe split points)      │
                  └─────────────────────────────────┘
                               │
              ┌────────────────┼────────────────┐
              ▼                ▼                ▼
         ┌────────┐       ┌────────┐      ┌────────┐
         │Chunk 1 │       │Chunk 2 │ ...  │Chunk N │
         │Parser  │       │Parser  │      │Parser  │
         └────────┘       └────────┘      └────────┘
              │                │                │
              │    (Rayon Thread Pool)          │
              └────────────────┼────────────────┘
                               ▼
                  ┌─────────────────────────────────┐
                  │   Result Merger / Output Stream │
                  └─────────────────────────────────┘
```

### Design Principles

1. **Chunk-based parallelism**: Split input at safe boundaries (top-level elements)
2. **Zero-copy events**: Events reference mmap'd memory directly
3. **Thread-local parsers**: Each thread has independent parser instance
4. **Stateless tokenization**: Separate stateless ops from stateful parsing
5. **Arena allocation**: Use bump allocators for per-chunk temporary data

---

## Phase 1: Foundation - Enable Parallelization

**Goal**: Make parser capable of parallel chunk processing

**Expected Performance**: 300-400 MB/s (8-10x improvement)

### 1.1 New Core Types

**File**: `/workspace/src/chunk.rs` (NEW ~300 lines)

```rust
/// A chunk represents a contiguous byte slice with safe boundaries
pub struct Chunk<'a> {
    data: &'a [u8],
    offset: u64,          // File offset
    chunk_id: usize,      // For ordering
    overlap_bytes: usize, // Overlap with previous chunk
}

/// Finds safe split points at top-level element boundaries
pub struct ChunkSplitter {
    min_chunk_size: usize,  // e.g., 1-4 MB
    max_chunk_size: usize,  // e.g., 16-32 MB
}

impl ChunkSplitter {
    pub fn split<'a>(&self, data: &'a [u8]) -> Vec<Chunk<'a>> {
        // Algorithm:
        // 1. Start at min_chunk_size intervals
        // 2. Scan forward tracking nesting depth
        // 3. When depth=0 after '>', that's a safe split
        // 4. Include 4-8KB overlap for elements near boundaries
    }
}
```

### 1.2 Slice-Based Parser

**File**: `/workspace/src/slice_parser.rs` (NEW ~500 lines)

```rust
/// Parser that operates on byte slice instead of io::Read
pub struct SliceParser<'data> {
    input: &'data [u8],
    pos: usize,
    scratch: ScratchBuffers,
    elem_stack: ElementStack,
    attr_table: AttributeTable,
    limits: ParserLimits,
}

impl<'data> SliceParser<'data> {
    pub fn new(data: &'data [u8], limits: ParserLimits) -> Self {
        // Same initialization as Parser::new
        // but without io::Read dependency
    }

    pub fn next_event<'a>(&'a mut self) -> Result<Event<'a>, ParseError> {
        // Similar to current next_event but:
        // - No io::Read, just advance pos through slice
        // - Events reference slice directly (zero-copy)
    }
}
```

**Key Changes**:
- Remove `io::Read` dependency
- Remove `InputBuffer` (use slice directly)
- Eliminate sliding window logic

### 1.3 Parallel Processing Framework

**File**: `/workspace/src/parallel.rs` (NEW ~200 lines)

```rust
use rayon::prelude::*;
use memmap2::Mmap;

pub struct ParallelProcessor {
    num_threads: usize,
    limits: ParserLimits,
    splitter: ChunkSplitter,
}

impl ParallelProcessor {
    pub fn process_file<F, T>(&self, path: &Path, chunk_fn: F) -> io::Result<Vec<T>>
    where
        F: Fn(Chunk) -> T + Send + Sync,
        T: Send,
    {
        // 1. Memory-map the file
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // 2. Split into chunks
        let chunks = self.splitter.split(&mmap[..]);

        // 3. Process in parallel with Rayon
        let results: Vec<T> = chunks
            .into_par_iter()
            .map(chunk_fn)
            .collect();

        Ok(results)
    }
}
```

### 1.4 Backwards Compatibility

Keep existing `Parser<R: io::Read>` implementation unchanged for backwards compatibility.

### Phase 1 Testing

- Unit tests for ChunkSplitter with various XML structures
- Correctness tests comparing SliceParser vs Parser<R>
- Chunk boundary tests for elements near splits
- Parallel correctness vs single-threaded

---

## Phase 2: Zero-Copy Optimization

**Goal**: Eliminate unnecessary data copying

**Expected Performance**: 900 MB/s - 1.2 GB/s (additional 2.5-3x)

### 2.1 Zero-Copy Event Model

**File**: `/workspace/src/event.rs` (NEW ~200 lines)

```rust
/// Zero-copy event that references mmap'd data
pub struct ZeroCopyEvent<'data> {
    pub event_type: EventType,
    pub data: &'data [u8],  // Direct slice into mmap
    pub is_continuation: bool,
    pub error: Option<ErrorCode>,
    pub attributes: ZeroCopyAttributes<'data>,
}

pub struct ZeroCopyAttributes<'data> {
    attrs: SmallVec<[(AttrSlice<'data>, AttrSlice<'data>); 8]>,
}

enum AttrSlice<'data> {
    Direct(&'data [u8]),     // Direct reference to mmap
    Decoded(Arc<[u8]>),      // Shared decoded data (for entities)
}
```

**Key Insight**: For most XML, attributes and text reference mmap directly. Only entity decoding requires copying.

### 2.2 Eliminate InputBuffer Copying

**Current Problem** (input_buffer.rs:71-77):
```rust
self.buf.copy_within(self.start..self.end, 0);  // EXPENSIVE
```

**Solution**: With mmap and slice parsing, InputBuffer is eliminated entirely.

### 2.3 Eliminate Scratch Buffer Copying

**Current Problem** (scratch.rs:63):
```rust
self.name.extend_from_slice(&data[..to_copy]);  // Copy
```

**Solution**: Use slice references with careful lifetime management:

```rust
pub struct ZeroCopyScratch<'data> {
    element_names: Arena<u8>,  // Bump allocator for names that must be copied
    // For attributes: use direct slices (no copy needed)
}
```

### 2.4 SIMD-Optimized Scanning

**File**: `/workspace/src/simd_scanner.rs` (NEW ~400 lines)

```rust
use std::arch::x86_64::*;

pub struct SimdScanner;

impl SimdScanner {
    /// Find any of: '<', '>', '&' using AVX2
    #[target_feature(enable = "avx2")]
    pub unsafe fn find_xml_delimiters(data: &[u8]) -> Option<usize> {
        // Scan 32 bytes at a time with AVX2
        // 2-3x faster than byte-by-byte
    }

    /// Validate name characters using SSE4.2
    #[target_feature(enable = "sse4.2")]
    pub unsafe fn validate_name_fast(data: &[u8]) -> bool {
        // Use PCMPESTRI for fast validation
    }
}
```

### 2.5 Arena Allocation

**File**: `/workspace/src/arena_parser.rs` (NEW ~400 lines)

```rust
use bumpalo::Bump;

pub struct ArenaParser<'data> {
    data: &'data [u8],
    arena: Bump,  // Fast allocation, freed when chunk done
}

impl<'data> ArenaParser<'data> {
    pub fn new_in_arena(data: &'data [u8], limits: ParserLimits) -> Self {
        let arena = Bump::with_capacity(1024 * 1024); // 1MB arena
        // Allocate scratch buffers from arena
    }
}
```

### Phase 2 Testing

- Memory validation with Miri (no invalid slice references)
- Performance benchmarks measuring copy elimination
- SIMD correctness tests
- Memory usage verification

---

## Phase 3: Advanced Optimizations

**Goal**: Push to 3-5 GB/s through specialized fast-paths

**Expected Performance**: 3-5 GB/s (additional 2.5-4x)

### 3.1 Filtering Fast-Path

**File**: `/workspace/src/filter.rs` (NEW ~350 lines)

```rust
/// Specialized parser for element filtering
pub struct FilteringParser<'data> {
    data: &'data [u8],
    target_name: &'data [u8],
    skip_depth: usize,
}

impl<'data> FilteringParser<'data> {
    pub fn find_next_match(&mut self) -> Option<ElementMatch<'data>> {
        // Fast-path:
        // 1. memchr for '<'
        // 2. Quick check if tag name matches
        // 3. If no match: skip to '>' without parsing attributes
        // 4. If match: parse attributes only for this element
        // Saves 90% of work for filtering!
    }
}
```

### 3.2 Specialized Matchers

**File**: `/workspace/src/matcher.rs` (NEW ~200 lines)

```rust
pub trait ElementMatcher {
    fn matches(&self, name: &[u8], attrs: &RawAttributes) -> bool;
}

pub struct NameMatcher {
    target: &'static [u8],
}

pub struct NameAttrMatcher {
    name: &'static [u8],
    attr_key: &'static [u8],
    attr_value: &'static [u8],
}
```

### 3.3 Streaming Output

**File**: `/workspace/src/output.rs` (NEW ~250 lines)

```rust
/// Lock-free multi-producer output stream
pub struct OutputSink {
    chunks: crossbeam::queue::SegQueue<OutputChunk>,
    writer_thread: JoinHandle<()>,
}

impl OutputSink {
    pub fn new(output: impl Write + Send + 'static) -> Self {
        // Background thread:
        // 1. Dequeues chunks
        // 2. Sorts by chunk_id
        // 3. Writes in order
    }

    pub fn submit(&self, chunk: OutputChunk) {
        self.chunks.push(chunk);  // Lock-free!
    }
}
```

### 3.4 Adaptive Chunk Sizing

Modify `/workspace/src/chunk.rs`:

```rust
impl ChunkSplitter {
    pub fn split_adaptive<'a>(&self, data: &'a [u8]) -> Vec<Chunk<'a>> {
        // Measure element density in first chunk
        let element_density = measure_element_density(&data[..self.min_chunk_size]);

        // Dense XML: smaller chunks (1-2 MB)
        // Sparse XML: larger chunks (16-32 MB)
        let optimal_size = if element_density > 0.1 {
            self.min_chunk_size
        } else {
            self.max_chunk_size
        };
    }
}
```

### Phase 3 Testing

- Fast-path correctness vs full parsing
- Output ordering verification
- Adaptive sizing performance across XML types
- Stress testing with 10GB+ files

---

## Implementation Summary

| Phase | Changes | Throughput | Speedup |
|-------|---------|-----------|---------|
| Baseline | Current | 37.5 MB/s | 1x |
| Phase 1 | Parallel + mmap | 300-400 MB/s | 8-10x |
| Phase 2 | Zero-copy + SIMD | 900 MB/s - 1.2 GB/s | 24-32x |
| Phase 3 | Fast-paths | **3-5 GB/s** | **80-130x** |

---

## Critical Files

### New Files (10 total)

1. `/workspace/src/chunk.rs` - Chunk splitting logic (~300 lines)
2. `/workspace/src/slice_parser.rs` - Slice-based parser (~500 lines)
3. `/workspace/src/parallel.rs` - Parallel framework (~200 lines)
4. `/workspace/src/event.rs` - Zero-copy events (~200 lines)
5. `/workspace/src/simd_scanner.rs` - SIMD optimizations (~400 lines)
6. `/workspace/src/arena_parser.rs` - Arena allocation (~400 lines)
7. `/workspace/src/filter.rs` - Filtering fast-path (~350 lines)
8. `/workspace/src/output.rs` - Parallel output (~250 lines)
9. `/workspace/src/matcher.rs` - Element matchers (~200 lines)
10. `/workspace/benches/throughput.rs` - Benchmarks (~300 lines)

### Modified Files (3 total)

1. `/workspace/Cargo.toml` - Add dependencies
2. `/workspace/src/lib.rs` - Export new modules
3. `/workspace/src/main.rs` - Use parallel processor

### Unchanged Files (backwards compatibility)

- `/workspace/src/parser.rs` - Keep existing Parser<R>
- `/workspace/src/input_buffer.rs`
- `/workspace/src/scratch.rs`
- `/workspace/src/element_stack.rs`
- `/workspace/src/tokenizer.rs`
- `/workspace/src/attributes.rs`

---

## Dependency Changes

Add to `/workspace/Cargo.toml`:

```toml
[dependencies]
memchr = "2.7.6"        # Already present

# Phase 1: Parallelization
rayon = "1.8"           # Data parallelism
memmap2 = "0.9"         # Memory-mapped I/O
crossbeam = "0.8"       # Lock-free structures

# Phase 2: Zero-Copy
bumpalo = "3.14"        # Bump allocator
smallvec = "1.11"       # Stack-allocated vectors

[dev-dependencies]
proptest = "1.9.0"      # Already present
criterion = "0.5"       # Benchmarking
```

---

## Verification Strategy

### Correctness Testing

1. **Property-based testing**:
   - Generate random XML
   - Compare parallel vs single-threaded output
   - Verify identical results

2. **Chunk boundary tests**:
   - Elements at chunk boundaries
   - Overlap handling
   - Duplicate detection

3. **Regression tests**:
   - Keep all existing tests passing
   - Add parallel-specific tests

### Performance Benchmarking

**File**: `/workspace/benches/throughput.rs`

```rust
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn benchmark_throughput(c: &mut Criterion) {
    let sizes = [100_000_000, 1_000_000_000, 10_000_000_000];

    for size in sizes {
        let mut group = c.benchmark_group("throughput");
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_function(format!("{}MB", size / 1_000_000), |b| {
            b.iter(|| /* parallel processing */);
        });
    }
}

criterion_group!(benches, benchmark_throughput);
criterion_main!(benches);
```

Run with:
```bash
cargo bench --bench throughput
```

### Profiling

```bash
# CPU profiling
perf record -g ./target/release/ripx large.xml relation
perf report

# Memory profiling
valgrind --tool=massif ./target/release/ripx large.xml

# Cache analysis
perf stat -e cache-misses,cache-references ./target/release/ripx large.xml
```

---

## Risk Mitigation

### Risk 1: Chunk Boundary Handling

**Risk**: Elements spanning boundaries might be missed

**Mitigation**:
- 4-8KB overlap between chunks
- Duplicate detection markers
- Comprehensive boundary tests

### Risk 2: Memory Pressure

**Risk**: N threads × memory could exhaust RAM

**Mitigation**:
- Adaptive thread count based on available memory
- Bounded chunk queue
- Back-pressure mechanism

### Risk 3: SIMD Portability

**Risk**: SIMD may not work on all CPUs

**Mitigation**:
- Runtime CPU feature detection
- Scalar fallbacks
- Compile-time feature flags

### Risk 4: Output Ordering

**Risk**: Parallel processing complicates ordering

**Mitigation**:
- Chunk IDs for ordering
- crossbeam mpsc for ordered collection
- Option for unordered output

---

## Performance Tuning Parameters

```rust
pub struct ParallelConfig {
    pub num_threads: Option<usize>,   // None = auto-detect
    pub min_chunk_size: usize,         // Default: 1MB
    pub max_chunk_size: usize,         // Default: 16MB
    pub overlap_bytes: usize,          // Default: 4KB
    pub enable_simd: bool,             // Default: true
}
```

---

## Storing Plan in Repo

After approval, this plan will be saved to the repository root as:

**File**: `/workspace/PERFORMANCE_PLAN.md`

This will serve as the master implementation guide for achieving 3-5 GB/s throughput.

---

## Next Steps

1. **Phase 1 Implementation** (~2-3 weeks):
   - Implement chunk.rs, slice_parser.rs, parallel.rs
   - Add rayon, memmap2 dependencies
   - Test parallel correctness
   - Target: 300-400 MB/s

2. **Phase 2 Implementation** (~2-3 weeks):
   - Zero-copy events
   - SIMD scanning
   - Arena allocation
   - Target: 900 MB/s - 1.2 GB/s

3. **Phase 3 Implementation** (~2-3 weeks):
   - Filtering fast-paths
   - Adaptive chunking
   - Output streaming
   - Target: **3-5 GB/s**

**Total estimated effort**: 6-9 weeks for complete implementation

---

## Conclusion

This plan provides a clear path from 37.5 MB/s to 3-5 GB/s through three incremental, testable phases. Each phase delivers measurable improvements while maintaining backwards compatibility with the existing single-threaded parser.

The architecture leverages proven technologies (Rayon, memmap2, SIMD) and focuses on the highest-ROI optimizations: parallelization, zero-copy parsing, and specialized filtering fast-paths.
