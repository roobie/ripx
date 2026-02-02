# Mind Map Format - Self-Documentation

> **For AI Agents:** This mind map is your primary knowledge index. Read overview nodes [1-5] first, then follow links [N] to find what you need. Always reference node IDs. When you encounter bugs, document your attempts in relevant nodes. When you make changes, update outdated nodes immediately—especially overview nodes since they're your springboard. Add new nodes only for genuinely new concepts. Keep it compact (20-50 nodes typical). The mind map wraps every task: consult it, rely on it, update it.

[1] **Mind Map Format Overview** - A graph-based documentation format stored as plain text files where each node is a single line containing an ID, title, and inline references [2]. The format leverages LLM familiarity with citation-style references from academic papers, making it natural to generate and edit [3]. It serves as a superset structure that can represent trees, lists, or any graph topology [4], scaling from small projects (<50 nodes) to complex systems (500+ nodes) [5]. The methodology is fully detailed in PROJECT_MIND_MAPPING.md with bootstrapping tools available.

[2] **Node Syntax Structure** - Each node follows the format: `[N] **Node Title** - node text with [N] references inlined` [1]. Nodes are line-oriented, allowing line-by-line loading and editing by AI models [3]. The inline reference syntax `[N]` creates bidirectional navigation between concepts, with links embedded naturally within descriptive text rather than as separate metadata [1][4]. This structure is both machine-parseable and human-readable, supporting grep-based lookups for quick node retrieval [3].

[3] **Technical Advantages** - The format enables line-by-line overwriting of nodes without complex parsing [2], making incremental updates efficient for both humans and AI agents [1]. Grep operations allow instant node lookup by ID or keyword without loading the entire file [2]. The text-based storage ensures version control compatibility, diff-friendly editing, and zero tooling dependencies [4]. LLMs generate this format naturally because citation syntax `[N]` mirrors academic paper references they've seen extensively during training [1][5].

[4] **Graph Topology Benefits** - Unlike hierarchical trees or linear lists, the graph structure allows many-to-many relationships between concepts [1]. Any node can reference any other node, creating knowledge clusters around related topics [2][3]. The format accommodates cyclic references for concepts that mutually depend on each other, captures cross-cutting concerns that span multiple subsystems, and supports progressive refinement where nodes are added to densify understanding [5]. This flexibility makes it suitable as a universal knowledge representation format [1].

[5] **Scalability and Usage Patterns** - Small projects typically need fewer than 50 nodes to capture core architecture, data flow, and key implementations [1]. Complex topics or large codebases can scale to 500+ nodes by adding specialized deep-dive nodes for algorithms, optimizations, and subsystems [4]. The methodology includes a bootstrap prompt (linked gist) for generating initial mind maps from existing codebases automatically [1]. Scale is managed through overview nodes [1-5] that serve as navigation hubs, with detail nodes forming clusters around major concepts [3][4]. The format remains navigable at any scale due to inline linking and grep-based search [2][3].


---

# Repository Knowledge Cluster

[10] **Repository: ripx (README summary)** - ripx is a streaming, non-validating XML query tool focused on single-pass, low-memory processing of very large XML files. Includes goals, quickstart, and beans CLI task-tracking note. See README.md for full details. [README.md]

[11] **Performance Plan (3-5 GB/s)** - Phased implementation plan for reaching 3-5 GB/s: Phase 1 parallelization, Phase 2 zero-copy+SIMD, Phase 3 fast-paths. Contains architecture diagrams, file lists, testing, benchmarking and risk mitigation. Use for strategic work and benchmarking. [PERFORMANCE_PLAN.md][2]

[12] **Phase 1 Status** - Progress report for Phase 1 parallel chunking: chunk splitting, slice parser, parallel processor, memmap + rayon integration, tests passing, known limitations (attribute parsing incomplete). Useful to track parallel foundation. [PHASE1_STATUS.md][11]

[13] **Phase 2 Results** - Outcomes of zero-copy and SIMD optimizations: performance numbers (single-thread and parallel), modules added (simd_scanner, event, arena_parser), known limitations (chunk overlap counting, simplified attribute parsing before this work). Use to verify Phase 2 objectives and as a baseline for Phase 3. [PHASE2_RESULTS.md][11]

[14] **ParserV2 Plan** - Planning document for ParserV2 redesign (slice-based parser, API changes, zero-copy events). Reference when modifying parser interfaces or adding slice-based parsers. [ParserV2.plan.md][11]

[15] **ParserV2 Full Doc** - Expanded design notes, rationale, and alternatives for ParserV2. Deep dive resource for parser implementation and trade-offs. [ParserV2.md][14]

[16] **Project PLAN** - High-level roadmap and milestones complementing the Performance Plan. Use for scheduling, bean creation, and coordinating phases. [PLAN.md][11]

[17] **Improvements Checklist (1-2 GB/s)** - Actionable checklist created during Phase 2 completion work; tracks tasks to reach 1-2 GB/s. Items updated as work progressed (many Phase 2 items now checked). Use for incremental progress and task assignment. [IMPROVEMENTS_FOR_1_2_GBPS.md][13]

[18] **Mind Map Format Doc** - This file (MINDMAP.md) defines the mindmap format and agent rules. All mindmap data must be stored here per repo policy. Read this before editing the map. [MINDMAP.md]

[19] **Documentation Conversion Policy** - All repository .md files are consolidated into this MINDMAP.md. When docs change, update corresponding node(s) here and keep original files for full content. Use node references to point to original files when preserving full text is desired. [IMPROVEMENTS_FOR_1_2_GBPS.md][10]


---

