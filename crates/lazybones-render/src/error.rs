//! Render failures.
//!
//! The only fallible path is the PDF render (`typst::compile` / `typst_pdf::pdf`
//! can return diagnostics). HTML preview is infallible. The API maps
//! [`RenderError`] onto a `500`.

/// A failure raised while rendering a document to PDF.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    /// `typst::compile` produced fatal diagnostics for the generated template.
    #[error("typst compile failed: {0}")]
    Compile(String),

    /// `typst_pdf::pdf` failed to emit PDF bytes from the laid-out document.
    #[error("typst pdf export failed: {0}")]
    Pdf(String),
}
