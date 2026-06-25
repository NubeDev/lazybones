//! Shared tool-argument DTOs.
//!
//! MCP tool inputs are JSON objects whose JSON-Schema rmcp derives from
//! `schemars`-derived argument structs (the `#[tool]` macro reads the struct in the
//! method signature). The cross-tool input shapes (ids, pagination, common
//! filters) live here so the `tools::*` modules share one definition rather than
//! redeclaring them per verb.
//!
//! Empty in this scaffold (task `mcp-crate`): the structs land alongside their
//! tools as the §6 surface is implemented. `schemars` is re-exported by `rmcp`
//! (`rmcp::schemars`), so the DTOs derive `JsonSchema` without a separate dep.
