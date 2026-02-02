[1] **Repository Overview (ripx README)** - ripx is a streaming, non-validating XML query tool focused on single-pass, low-memory processing of very large XML files. See README.md for goals, quickstart, and task-tracking note about beans CLI. [README.md]

[2] **Performance Plan (3-5 GB/s)** - Implementation plan describing phased path to 3-5 GB/s: Phase 1 (parallel), Phase 2 (zero-copy + SIMD), Phase 3 (fast-paths). Contains architecture, files, testing, and benchmarking guidance. [PERFORMANCE_PLAN.md]

[3] **Phase 1 Status** - Progress report for Phase 1 (parallel chunking) including completed components, tests, limitations (attribute parsing incomplete), and next steps. Includes test results and architecture overview. [PHASE1_STATUS.md]

[4] **Phase 2 Results** - Results and metrics after implementing zero-copy and SIMD optimizations: throughput numbers, module summaries (simd_scanner, event, arena_parser), known limitations (overlap counting, simplified attribute parsing), and next steps. [PHASE2_RESULTS.md]

[5] **ParserV2 Design (plan)** - High-level design notes and planning for ParserV2 (plan doc). Useful when implementing slice-based parser and API changes. [ParserV2.plan.md]

[6] **ParserV2 Full Doc** - Extended documentation for ParserV2 ideas, rationale and design alternatives. Read when deep-diving parser rework. [ParserV2.md]

[7] **Implementation Plan (general)** - Project-level PLAN.md with roadmap and higher-level tasks; complement to PERFORMANCE_PLAN.md. [PLAN.md]

[8] **Improvements Checklist (1-2 GB/s)** - The actionable checklist created earlier detailing tasks to reach 1-2 GB/s (phase 2 completion items now marked done). Use this for incremental tracking. [IMPROVEMENTS_FOR_1_2_GBPS.md]

[9] **Mind Map Format Doc** - Describes the mindmap text format and agent guidelines: node syntax, referencing, update rules and scale guidance. Use as primary index format for other conversions. [MINDMAP.md]


---

Usage notes:
- Node IDs are small and intended as navigation hubs. Each node references the original markdown file in brackets for quick lookup.
- To expand a node into sub-nodes, add new numbered nodes and reference them inline (e.g., [2] references can point to [10], [11]).
- This file is a compact index; full content remains in the original .md files. Update this map when you add/remove docs.

Generated from repository .md files on disk. If you want, I can (a) split large docs into multiple nodes with per-section summaries, or (b) convert the full contents of each .md into per-section nodes automatically. Which would you prefer?