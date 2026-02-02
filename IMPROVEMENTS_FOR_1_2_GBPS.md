# Improvements Plan for 1-2 GB/s Parsing Speed

## Overview
This plan outlines the required changes to improve parsing speed from current ~693 MB/s to 1-2 GB/s. Based on the performance plan, this requires completing Phase 2 optimizations and implementing key Phase 3 fast-paths.

**Current Status**: 693 MB/s (Phase 2 ~70% complete)  
**Target**: 1-2 GB/s  
**Estimated Effort**: 2-4 weeks  
**Priority**: High - enables practical large-scale XML processing

## ✅ Phase 2 Completion (900 MB/s - 1.2 GB/s) - COMPLETED

### [x] Complete Attribute Parsing in ArenaParser
- **Location**: `src/arena_parser.rs`
- **Task**: Integrate full `parse_attributes()` from `tokenizer.rs` 
- **Impact**: Enables ignored `filter_elements()` test
- **Lines**: ~50 lines change
- **Status**: ✅ Completed
- **Priority**: Critical

### [x] Expand SIMD Scanner Coverage
- **Location**: `src/simd_scanner.rs` 
- **Tasks**:
  - [x] Add SIMD attribute value scanning
  - [x] Add SIMD quote character detection
  - [x] Add SIMD name validation functions
  - [x] Add SIMD whitespace skipping
- **Impact**: Faster tokenization across all parsers
- **Lines**: ~200 lines addition
- **Status**: ✅ Completed
- **Priority**: High

### [x] Complete Zero-Copy Event Model
- **Location**: `src/event.rs`
- **Tasks**:
  - [x] Implement full `ZeroCopyAttributes` 
  - [x] Handle decoded attributes with `Arc<[u8]>`
  - [x] Add proper entity handling
- **Impact**: Eliminate remaining copies in event emission
- **Lines**: ~100 lines completion
- **Status**: ✅ Completed
- **Priority**: High

## Phase 2 Summary
**Status**: ✅ **PHASE 2 COMPLETE**  
**Performance Target**: 900 MB/s - 1.2 GB/s (from 693 MB/s baseline)  
**Key Achievements**:
- Full attribute parsing with entity decoding in ArenaParser
- SIMD-accelerated scanning operations (whitespace, names, quotes)
- Zero-copy event structures with proper entity handling
- All tests passing (47 passed, 1 ignored)
- Foundation laid for 1-2 GB/s performance

## Phase 3 Fast-Paths (1-2 GB/s)

## Phase 3 Fast-Paths (1-2 GB/s)

### [ ] Implement Filtering Fast-Path Parser
- **Location**: New `src/filter.rs`
- **Tasks**:
  - [ ] Create `FilteringParser` struct
  - [ ] Implement fast element name matching
  - [ ] Add skip-to-end logic for non-matches
  - [ ] Integrate with parallel processing
- **Impact**: 3-5x speedup for selective queries (saves ~90% work)
- **Lines**: ~350 lines new file
- **Status**: Not started
- **Priority**: Critical

### [ ] Adaptive Chunk Sizing
- **Location**: `src/chunk.rs`
- **Tasks**:
  - [ ] Measure element density in first chunk
  - [ ] Adjust chunk sizes based on density
  - [ ] Dense XML: 1-2 MB chunks
  - [ ] Sparse XML: 16-32 MB chunks
- **Impact**: Optimized parallelism for different XML types
- **Lines**: ~50 lines modification
- **Status**: Not started
- **Priority**: Medium

### [ ] Streaming Parallel Output
- **Location**: New `src/output.rs`
- **Tasks**:
  - [ ] Implement lock-free output queue
  - [ ] Add background writer thread
  - [ ] Handle chunk ordering
- **Impact**: Prevents output bottleneck at high speeds
- **Lines**: ~250 lines new file
- **Status**: Not started
- **Priority**: Medium

## Infrastructure Improvements

### [ ] Fix Chunk Overlap Deduplication
- **Location**: `src/parallel.rs`
- **Tasks**:
  - [ ] Implement offset-based deduplication
  - [ ] Handle 8KB overlap regions
  - [ ] Fix ~0.35% over-counting issue
- **Impact**: Accurate parallel result counting
- **Lines**: ~100 lines modification
- **Status**: Not started
- **Priority**: Medium

### [ ] Complete Parallel Integration
- **Location**: `src/parallel.rs`, `src/main.rs`
- **Tasks**:
  - [ ] Switch to `ArenaParser` for parallel chunks
  - [ ] Update CLI to use parallel processing by default
- **Impact**: Leverage all optimizations in main binary
- **Lines**: ~50 lines modification
- **Status**: Partial
- **Priority**: High

### [ ] Add Performance Benchmarks
- **Location**: New `benches/throughput.rs`
- **Tasks**:
  - [ ] Implement criterion throughput benchmarks
  - [ ] Test with various file sizes (100MB-10GB)
  - [ ] Compare parallel vs single-threaded
- **Impact**: Quantitative performance tracking
- **Lines**: ~300 lines new file
- **Status**: Not started
- **Priority**: Medium

## Dependency Updates
### [ ] Update Cargo.toml
- **Tasks**:
  - [ ] Add `crossbeam = "0.8"` for lock-free queues
  - [ ] Add `criterion = "0.5"` for benchmarking
- **Status**: Not started
- **Priority**: Low

## Testing & Verification
### [ ] Performance Testing
- **Tasks**:
  - [ ] Run `cargo bench --bench throughput`
  - [ ] Profile with `perf record -g`
  - [ ] Test on large files (1GB+)
- **Status**: Not started
- **Priority**: Ongoing

### [ ] Correctness Testing  
- **Tasks**:
  - [ ] Ensure parallel output matches single-threaded
  - [ ] Test chunk boundary handling
  - [ ] Validate all existing tests pass
- **Status**: Ongoing
- **Priority**: Critical

## Implementation Timeline
- **Week 1**: Complete Phase 2 (arena parser attributes, SIMD expansion)
- **Week 2**: Implement filtering fast-path
- **Week 3**: Adaptive chunking, output streaming
- **Week 4**: Testing, benchmarking, refinements

## Success Criteria
- [ ] Single-threaded performance: > 200 MB/s
- [ ] Parallel performance: > 1 GB/s  
- [ ] Filtering queries: > 2 GB/s
- [ ] All tests passing
- [ ] Benchmarks added and running

## Notes
- Filtering fast-paths provide biggest speedup potential (3-5x)
- SIMD improvements benefit all parsing operations
- Adaptive chunking prevents under/over-parallelization
- Output streaming becomes critical at >1 GB/s

## Tracking
Use this checklist to track progress. Update status and add details as work progresses. Follow the beans task tracking system for implementation work.