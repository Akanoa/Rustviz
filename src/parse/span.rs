//! Span, FileId, SourceMap, SourceFile.
//!
//! Spans are byte-offset half-open ranges `[start, end)` into a file registered
//! in a [`SourceMap`]. Multi-file ready from day one (CLAUDE.md locked-in decision).

use std::collections::BTreeMap;

/// Identifier for a source file registered in a [`SourceMap`].
///
/// Allocated by [`SourceMap::add`]. `FileId(0)` is reserved as a sentinel for
/// "no file" and is never returned by `add`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// A half-open byte range `[start, end)` into a source file.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive start byte offset.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
    /// File this span refers to.
    pub file: FileId,
}

impl Span {
    /// Construct a span. Debug-asserts `start <= end`.
    pub fn new(start: u32, end: u32, file: FileId) -> Self {
        debug_assert!(start <= end, "span: start {start} > end {end}");
        Self { start, end, file }
    }

    /// Zero-length span at the given byte offset (used for EOF errors).
    pub fn point(at: u32, file: FileId) -> Self {
        Self { start: at, end: at, file }
    }

    /// Smallest span covering both `self` and `other`. Debug-asserts same file.
    pub fn merge(self, other: Self) -> Self {
        debug_assert_eq!(
            self.file, other.file,
            "cannot merge spans from different files"
        );
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            file: self.file,
        }
    }
}

/// A registered source file: name + bytes + precomputed line starts.
#[derive(Debug)]
pub struct SourceFile {
    /// Human-readable file name (e.g. `"m01_arithmetic.rs"`).
    pub name: String,
    /// Source bytes as a UTF-8 string.
    pub src: String,
    /// Byte offset of each line's first character. `line_starts[0] == 0`,
    /// strictly increasing. Used for `(line, col)` lookup.
    line_starts: Vec<u32>,
}

impl SourceFile {
    fn new(name: String, src: String) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in src.as_bytes().iter().enumerate() {
            if *b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self { name, src, line_starts }
    }
}

/// A registry of source files indexed by [`FileId`]. Uses [`BTreeMap`] for
/// deterministic iteration order so snapshot tests are stable (SC-004).
#[derive(Debug, Default)]
pub struct SourceMap {
    files: BTreeMap<FileId, SourceFile>,
    next_id: u32,
}

impl SourceMap {
    /// Create an empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a file. Returns its newly-allocated [`FileId`].
    pub fn add(&mut self, name: String, src: String) -> FileId {
        self.next_id += 1;
        let id = FileId(self.next_id);
        self.files.insert(id, SourceFile::new(name, src));
        id
    }

    /// Look up a registered file.
    pub fn get(&self, file: FileId) -> Option<&SourceFile> {
        self.files.get(&file)
    }

    /// Derive 1-based `(line, col)` from a span's start byte.
    /// Returns `None` if `span.file` is not registered.
    pub fn line_col(&self, span: Span) -> Option<(u32, u32)> {
        let file = self.get(span.file)?;
        // Largest index `i` with line_starts[i] <= span.start.
        let idx = file.line_starts.partition_point(|&s| s <= span.start);
        if idx == 0 {
            return None;
        }
        let line_idx = (idx - 1) as u32;
        let line_start = file.line_starts[idx - 1];
        let col = span.start.saturating_sub(line_start);
        Some((line_idx + 1, col + 1))
    }
}
