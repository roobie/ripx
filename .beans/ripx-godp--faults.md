---
# ripx-godp
title: faults
status: draft
type: task
priority: normal
created_at: 2026-01-27T20:07:29Z
updated_at: 2026-01-27T20:07:54Z
---

- **UnexpectedEof**: EOF reached mid-construct (e.g. unterminated attribute value `"<a x=\"1"` or unterminated comment/PI/CDATA).
- **OrphanedEndElement**: end-tag with no matching open element when you want to treat it as a structural fault (e.g. `"</x>"` when stack empty) — currently present in enum but not emitted.
- **MismatchedQuotes**: attribute value quotes mismatch or stray quote handling (e.g. `a="v'`).
- **TokenTooLong**: generic token length overflow not covered by specific Name/Attr/Value limits.
- **TextTooLong**: text total-length limit exceeded (when `max_text_total_len` is set).
- **CommentTooLong**: comment chunk/total length exceeded or comment truncated by limits.
- **CDataTooLong**: CDATA chunk/total length exceeded.
- **ProcessingInstructionTooLong**: PI chunk/total length exceeded.
- **InvalidStructure**: structural errors not covered by other codes (e.g. malformed markup sequences).
- **IoError**: propagate underlying I/O errors as Fault events (wrap underlying `io::Error`).

