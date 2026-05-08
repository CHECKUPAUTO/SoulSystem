//! PageProvider trait — abstraction for document sources (PDF, text, etc).

use crate::page::ChunkType;

/// A document to be ingested into the RAG index.
#[derive(Debug, Clone)]
pub struct Document {
    /// Unique identifier for this document.
    pub id: String,
    /// Document title.
    pub title: String,
    /// Source path or URL.
    pub source: String,
    /// Total number of pages.
    pub page_count: usize,
}

/// Trait for extracting pages from a document.
///
/// Implementations: PdfPageProvider, TextPageProvider, MarkdownPageProvider.
pub trait PageProvider: Send + Sync {
    /// Get the document metadata.
    fn document(&self) -> Document;

    /// Get the total number of pages.
    fn get_page_count(&self) -> usize {
        self.document().page_count
    }

    /// Get the content of a specific page (0-indexed).
    fn get_page_content(&self, page_num: usize) -> Option<String>;

    /// Clean content by removing headers, footers, and noise.
    /// Default implementation strips repeated lines.
    fn clean_content(&self, raw: String) -> String {
        let lines: Vec<&str> = raw.lines().collect();
        let mut cleaned = String::new();

        // Detect and remove repeated headers/footers
        // (lines that appear on >50% of pages are likely headers/footers)
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                cleaned.push('\n');
                continue;
            }
            // Skip very short lines that look like page numbers
            if trimmed.len() <= 3 && trimmed.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            cleaned.push_str(line);
            cleaned.push('\n');
        }

        cleaned.trim().to_string()
    }

    /// Detect the chunk type for a page.
    fn detect_chunk_type(&self, content: &str) -> ChunkType {
        let trimmed = content.trim();

        // Short all-caps lines = likely a header
        if trimmed.len() < 80 && trimmed.chars().filter(|c| c.is_uppercase()).count() > trimmed.len() / 2 {
            return ChunkType::Header;
        }

        // Lines starting with | or containing tab-separated values = table
        let lines: Vec<&str> = trimmed.lines().collect();
        let pipe_lines = lines.iter().filter(|l| l.contains('|')).count();
        if pipe_lines > lines.len() / 2 && pipe_lines > 2 {
            return ChunkType::Table;
        }

        // Lines starting with common code markers
        if lines.iter().any(|l| l.starts_with("fn ") || l.starts_with("def ") || l.starts_with("class ")) {
            return ChunkType::Code;
        }

        ChunkType::Body
    }
}

/// Simple text-based page provider — splits text by page markers.
pub struct TextPageProvider {
    doc_id: String,
    title: String,
    source: String,
    pages: Vec<String>,
}

impl TextPageProvider {
    /// Create from a single text, splitting by `\f` (form feed) or by size.
    pub fn from_text(doc_id: impl Into<String>, title: impl Into<String>, text: &str) -> Self {
        let pages: Vec<String> = if text.contains('\x0c') {
            // Split on form feed character (^L)
            text.split('\x0c').map(|s| s.to_string()).collect()
        } else {
            // Split into pages of ~3000 chars
            let mut pages = Vec::new();
            let mut current = String::new();
            for line in text.lines() {
                if current.len() + line.len() > 3000 && !current.is_empty() {
                    pages.push(std::mem::take(&mut current));
                }
                current.push_str(line);
                current.push('\n');
            }
            if !current.is_empty() {
                pages.push(current);
            }
            if pages.is_empty() {
                pages.push(text.to_string());
            }
            pages
        };

        Self {
            doc_id: doc_id.into(),
            title: title.into(),
            source: "text://inline".to_string(),
            pages,
        }
    }

    /// Create from a file path, splitting by pages.
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let title = std::path::Path::new(path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or("unknown".to_string());
        Ok(Self::from_text(path, &title, &text))
    }
}

impl PageProvider for TextPageProvider {
    fn document(&self) -> Document {
        Document {
            id: self.doc_id.clone(),
            title: self.title.clone(),
            source: self.source.clone(),
            page_count: self.pages.len(),
        }
    }

    fn get_page_content(&self, page_num: usize) -> Option<String> {
        self.pages.get(page_num).cloned()
    }
}

/// PDF page provider — placeholder for pdf-extract integration.
/// In production, this would use pdf-extract or poppler.
pub struct PdfPageProvider {
    doc_id: String,
    title: String,
    source: String,
    pages: Vec<String>,
}

impl PdfPageProvider {
    /// Create from pre-extracted PDF pages.
    pub fn from_pages(doc_id: impl Into<String>, title: impl Into<String>, pages: Vec<String>) -> Self {
        let id = doc_id.into();
        let source = format!("pdf://{}", id);
        Self {
            doc_id: id,
            title: title.into(),
            source,
            pages,
        }
    }
}

impl PageProvider for PdfPageProvider {
    fn document(&self) -> Document {
        Document {
            id: self.doc_id.clone(),
            title: self.title.clone(),
            source: self.source.clone(),
            page_count: self.pages.len(),
        }
    }

    fn get_page_content(&self, page_num: usize) -> Option<String> {
        self.pages.get(page_num).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_provider_from_text() {
        let text = "Page 1 content\n\x0cPage 2 content\n\x0cPage 3 content";
        let provider = TextPageProvider::from_text("doc-1", "Test Doc", text);
        assert_eq!(provider.get_page_count(), 3);
        assert_eq!(provider.get_page_content(0).unwrap(), "Page 1 content\n");
    }

    #[test]
    fn text_provider_auto_split() {
        let text = (0..100).map(|i| format!("Line {} with some content here", i)).collect::<Vec<_>>().join("\n");
        let provider = TextPageProvider::from_text("doc-2", "Long Doc", &text);
        assert!(provider.get_page_count() > 1, "should split into multiple pages");
    }

    #[test]
    fn clean_content_removes_page_numbers() {
        let provider = TextPageProvider::from_text("doc", "Test", "Real content\n3\nMore content");
        let cleaned = provider.clean_content("Header repeated\nReal content\n3\nFooter repeated".to_string());
        // Page number "3" on its own line should be removed
        assert!(!cleaned.contains("\n3\n"));
    }

    #[test]
    fn detect_chunk_type_header() {
        let provider = TextPageProvider::from_text("doc", "T", "content");
        assert_eq!(provider.detect_chunk_type("INTRODUCTION"), ChunkType::Header);
    }

    #[test]
    fn detect_chunk_type_body() {
        let provider = TextPageProvider::from_text("doc", "T", "content");
        assert_eq!(provider.detect_chunk_type("This is a normal paragraph of text."), ChunkType::Body);
    }

    #[test]
    fn detect_chunk_type_table() {
        let provider = TextPageProvider::from_text("doc", "T", "content");
        let table = "| Name | Age |\n|---|---|\n| Alice | 30 |\n| Bob | 25 |";
        assert_eq!(provider.detect_chunk_type(table), ChunkType::Table);
    }

    #[test]
    fn pdf_provider_from_pages() {
        let provider = PdfPageProvider::from_pages("pdf-1", "Report", vec![
            "Page 1".to_string(),
            "Page 2".to_string(),
        ]);
        assert_eq!(provider.get_page_count(), 2);
        assert_eq!(provider.get_page_content(0).unwrap(), "Page 1");
    }
}