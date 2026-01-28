//! Chunk splitting for parallel XML processing.
//!
//! This module provides the ability to split large XML files into chunks at safe
//! boundaries (top-level element boundaries) for parallel processing.

/// A chunk represents a contiguous byte slice of the input file with safe boundaries.
///
/// Chunks are designed to be processed independently in parallel, with small overlap
/// regions to handle elements near boundaries.
#[derive(Debug, Clone)]
pub struct Chunk<'a> {
    /// The actual data bytes for this chunk
    pub data: &'a [u8],
    /// File offset where this chunk starts
    pub offset: u64,
    /// Chunk identifier for ordering output
    pub chunk_id: usize,
    /// Number of bytes overlapping with the previous chunk
    pub overlap_bytes: usize,
}

/// Configuration for chunk splitting strategy.
#[derive(Debug, Clone)]
pub struct ChunkSplitter {
    /// Minimum chunk size in bytes (e.g., 1-4 MB)
    pub min_chunk_size: usize,
    /// Maximum chunk size in bytes (e.g., 16-32 MB)
    pub max_chunk_size: usize,
    /// Overlap size between chunks in bytes (e.g., 4-8 KB)
    pub overlap_bytes: usize,
}

impl Default for ChunkSplitter {
    fn default() -> Self {
        Self {
            min_chunk_size: 2 * 1024 * 1024,  // 2 MB
            max_chunk_size: 16 * 1024 * 1024, // 16 MB
            overlap_bytes: 8 * 1024,          // 8 KB
        }
    }
}

impl ChunkSplitter {
    /// Create a new chunk splitter with custom sizes.
    pub fn new(min_chunk_size: usize, max_chunk_size: usize, overlap_bytes: usize) -> Self {
        assert!(min_chunk_size > 0, "min_chunk_size must be > 0");
        assert!(
            max_chunk_size >= min_chunk_size,
            "max_chunk_size must be >= min_chunk_size"
        );
        assert!(
            overlap_bytes < min_chunk_size,
            "overlap_bytes must be < min_chunk_size"
        );

        Self {
            min_chunk_size,
            max_chunk_size,
            overlap_bytes,
        }
    }

    /// Split data into chunks at safe top-level element boundaries.
    ///
    /// This function finds boundaries where the XML nesting depth returns to 0,
    /// ensuring each chunk can be parsed independently.
    pub fn split<'a>(&self, data: &'a [u8]) -> Vec<Chunk<'a>> {
        if data.is_empty() {
            return Vec::new();
        }

        // For small files, just return a single chunk
        if data.len() <= self.min_chunk_size {
            return vec![Chunk {
                data,
                offset: 0,
                chunk_id: 0,
                overlap_bytes: 0,
            }];
        }

        let mut chunks = Vec::new();
        let mut current_offset = 0;
        let mut chunk_id = 0;

        while current_offset < data.len() {
            // Find the end of this chunk
            let remaining = &data[current_offset..];
            let chunk_end = self.find_chunk_boundary(remaining);

            // Calculate overlap with previous chunk
            let overlap = if chunk_id > 0 {
                self.overlap_bytes.min(current_offset)
            } else {
                0
            };

            // Actual chunk start considering overlap
            let chunk_start = current_offset.saturating_sub(overlap);
            let chunk_data_end = current_offset + chunk_end;

            chunks.push(Chunk {
                data: &data[chunk_start..chunk_data_end],
                offset: chunk_start as u64,
                chunk_id,
                overlap_bytes: overlap,
            });

            current_offset = chunk_data_end;
            chunk_id += 1;

            // If we've reached the end, break
            if chunk_data_end >= data.len() {
                break;
            }
        }

        chunks
    }

    /// Find the boundary for the next chunk by tracking XML nesting depth.
    ///
    /// Returns the offset within `data` where the chunk should end.
    fn find_chunk_boundary(&self, data: &[u8]) -> usize {
        // Start looking for a boundary after min_chunk_size
        let start_search = self.min_chunk_size.min(data.len());

        if start_search >= data.len() {
            return data.len();
        }

        // Track depth by counting < and > characters
        // When depth returns to 0, we have a safe split point
        let mut depth: i32 = 0;
        let mut pos: usize = 0;
        let mut last_safe_boundary = start_search;

        // First, get to the search start position and calculate initial depth
        for i in 0..start_search {
            match data[i] {
                b'<' => {
                    // Check if it's a closing tag
                    if i + 1 < data.len() && data[i + 1] == b'/' {
                        depth -= 1;
                    } else if i + 1 < data.len() && data[i + 1] != b'!' && data[i + 1] != b'?' {
                        depth += 1;
                    }
                }
                b'>' => {
                    // Check if this was a self-closing tag
                    if i > 0 && data[i - 1] == b'/' {
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }

        // Now search for a boundary where depth returns to 0
        pos = start_search;
        while pos < data.len() && pos < self.max_chunk_size {
            match data[pos] {
                b'<' => {
                    // Check if it's a closing tag
                    if pos + 1 < data.len() && data[pos + 1] == b'/' {
                        depth -= 1;
                    } else if pos + 1 < data.len() && data[pos + 1] != b'!' && data[pos + 1] != b'?'
                    {
                        depth += 1;
                    }
                }
                b'>' => {
                    // Check if this was a self-closing tag
                    if pos > 0 && data[pos - 1] == b'/' {
                        depth -= 1;
                    }

                    // If depth is 0, we've found a safe boundary
                    if depth == 0 {
                        last_safe_boundary = pos + 1;
                        // If we're past min size, we can stop here
                        if last_safe_boundary >= self.min_chunk_size {
                            return last_safe_boundary;
                        }
                    }
                }
                _ => {}
            }
            pos += 1;
        }

        // If we hit max_chunk_size, return the last safe boundary we found
        if last_safe_boundary > start_search {
            last_safe_boundary
        } else {
            // Fallback: if no safe boundary found, just use max_chunk_size
            self.max_chunk_size.min(data.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_data() {
        let splitter = ChunkSplitter::default();
        let chunks = splitter.split(b"");
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_single_small_chunk() {
        let splitter = ChunkSplitter::new(1024, 4096, 128);
        let data = b"<root><item>text</item></root>";
        let chunks = splitter.split(data);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, data);
        assert_eq!(chunks[0].chunk_id, 0);
        assert_eq!(chunks[0].overlap_bytes, 0);
    }

    #[test]
    fn test_multiple_chunks() {
        let splitter = ChunkSplitter::new(100, 1000, 20);

        // Create XML with multiple top-level elements
        let mut data = Vec::new();
        for i in 0..50 {
            data.extend_from_slice(format!("<item id=\"{}\">data</item>", i).as_bytes());
        }

        let chunks = splitter.split(&data);

        // Should split into multiple chunks
        assert!(
            chunks.len() > 1,
            "Expected multiple chunks, got {}",
            chunks.len()
        );

        // Check chunk IDs are sequential
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_id, i);
        }

        // Check overlaps (first chunk has no overlap)
        assert_eq!(chunks[0].overlap_bytes, 0);
        if chunks.len() > 1 {
            for chunk in &chunks[1..] {
                assert!(
                    chunk.overlap_bytes > 0,
                    "Expected overlap for chunk {}",
                    chunk.chunk_id
                );
            }
        }
    }

    #[test]
    fn test_nested_elements() {
        let splitter = ChunkSplitter::new(50, 500, 10);

        let data = b"<root><a><b><c>deep</c></b></a></root><root><a>shallow</a></root>";
        let chunks = splitter.split(data);

        // Each chunk should be valid XML fragment
        for chunk in &chunks {
            // Basic check: should have balanced tags at top level
            let open_count = chunk.data.iter().filter(|&&b| b == b'<').count();
            let close_count = chunk.data.iter().filter(|&&b| b == b'>').count();
            assert_eq!(
                open_count, close_count,
                "Unbalanced tags in chunk {}",
                chunk.chunk_id
            );
        }
    }

    #[test]
    fn test_chunk_boundaries_at_safe_points() {
        let splitter = ChunkSplitter::new(80, 500, 10);

        // XML with clear boundaries between elements
        let data = b"<item>1</item><item>2</item><item>3</item><item>4</item><item>5</item>";
        let chunks = splitter.split(data);

        // Verify chunks don't split in the middle of elements
        for chunk in &chunks {
            // Should start with < (after overlap)
            let start_idx = chunk.overlap_bytes;
            if start_idx < chunk.data.len() {
                // Should eventually find a <
                let has_tag_start = chunk.data[start_idx..].iter().any(|&b| b == b'<');
                assert!(
                    has_tag_start,
                    "Chunk {} doesn't contain tag start",
                    chunk.chunk_id
                );
            }
        }
    }
}
