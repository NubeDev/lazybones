//! Host probing for the orchestration engine + agent CLIs.
//!
//! Read-only detection that powers `GET /engine` (is hcom available?) and
//! `GET /agents` (which agent CLIs are installed + set up?). Nothing here mutates
//! state; it shells out to `command -v` / `--version` and reads the environment.

mod catalog;
mod detect;
mod report;

pub use report::{
    AgentReport, AgentTestResult, EngineReport, agent_reports, engine_report, env_var_for,
    test_agent,
};
