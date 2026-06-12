use std::path::Path;
use anyhow::Result;
use crate::error::AppError;

pub struct ParsedDoc {
    pub text: String,
    pub page_count: Option<usize>,
    pub warnings: Vec<String>,
}

pub async fn parse_upload(path: &Path, mime: &str) -> Result<ParsedDoc, AppError> {
    match mime {
        "application/pdf" => parse_pdf(path).await,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => parse_docx(path).await,
        "text/plain" | "text/markdown" | "text/csv" | "application/json" => parse_text(path).await,
        m if m.starts_with("text/") => parse_text(path).await,
        _ => parse_text(path).await,
    }
}

async fn parse_pdf(path: &Path) -> Result<ParsedDoc, AppError> {
    let path_owned = path.to_owned();
    let extracted = tokio::task::spawn_blocking(move || {
        pdf_extract::extract_text(&path_owned)
    }).await.map_err(|e| AppError::Internal(e.to_string()))?
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ParsedDoc {
        text: normalize_whitespace(&extracted),
        page_count: None,
        warnings: vec![],
    })
}

async fn parse_docx(path: &Path) -> Result<ParsedDoc, AppError> {
    let path_owned = path.to_owned();
    let text = tokio::task::spawn_blocking(move || -> Result<String, AppError> {
        let bytes = std::fs::read(&path_owned).map_err(|e| AppError::Internal(e.to_string()))?;
        let docx = docx_rs::read_docx(&bytes).map_err(|e| AppError::Internal(e.to_string()))?;

        let mut text = String::new();
        for child in docx.document.children {
            match child {
                docx_rs::DocumentChild::Paragraph(p) => {
                    for child in p.children {
                        if let docx_rs::ParagraphChild::Run(run) = child {
                            for child in run.children {
                                if let docx_rs::RunChild::Text(t) = child {
                                    text.push_str(&t.text);
                                }
                            }
                        }
                    }
                    text.push('\n');
                }
                _ => {} // Simplified Table extraction for now to avoid complexity
            }
        }
        Ok(text)
    }).await.map_err(|e| AppError::Internal(e.to_string()))??;

    Ok(ParsedDoc {
        text: normalize_whitespace(&text),
        page_count: None,
        warnings: vec![],
    })
}

async fn parse_text(path: &Path) -> Result<ParsedDoc, AppError> {
    let bytes = tokio::fs::read(path).await.map_err(|e| AppError::Internal(e.to_string()))?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(&bytes).0.into_owned(),
    };
    Ok(ParsedDoc {
        text: normalize_whitespace(&text),
        page_count: None,
        warnings: vec![],
    })
}

fn normalize_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_blank = false;
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
        } else {
            out.push_str(trimmed);
            out.push('\n');
            prev_blank = false;
        }
    }
    out
}
