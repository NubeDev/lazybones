//! Plain-text extraction from uploaded PDF source bytes.
//!
//! On a PDF upload, the API extracts text once and stores it on the
//! [`Source.extracted_text`](super::Source) field — it powers preview/keyword
//! search now and is the exact substrate the later RAG phase chunks + embeds.
//! Extraction is best-effort: anything that isn't a parseable PDF (or a PDF the
//! extractor can't read) yields `None` rather than an error, so a non-PDF or a
//! malformed upload never blocks creating the source row.

/// Extract plain text from PDF `bytes`, or `None` if it isn't a readable PDF.
///
/// Best-effort and panic-safe: a malformed PDF returns `None` rather than
/// propagating an error or unwinding.
#[must_use]
pub fn extract_pdf_text(bytes: &[u8]) -> Option<String> {
    // `pdf-extract` can panic on some malformed inputs; isolate it so a bad
    // upload degrades to "no extracted text" instead of taking down the caller.
    let bytes = bytes.to_vec();
    let result =
        std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&bytes).ok()).ok()?;
    match result {
        Some(text) if !text.trim().is_empty() => Some(text),
        _ => None,
    }
}
