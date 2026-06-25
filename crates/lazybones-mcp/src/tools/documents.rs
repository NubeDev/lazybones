//! Document tools — documents, pages, references, sources, branding, asset
//! metadata, render, and publish (design §6.2).
//!
//! All mutators check `Capability::Document`, the same guard the `document_*`
//! routes use. Asset *bytes* are not an MCP concern — uploads stay on REST
//! raw-body `POST /assets`; these tools carry only metadata + reference-by-id and
//! return the `/assets/:id` URL for the agent to fetch out of band.
//!
//! Scaffold: no tools yet (task `mcp-crate`); the §6.2 set lands in `mcp-documents`.
