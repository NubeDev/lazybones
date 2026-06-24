//! Host capability implementations.
//!
//! Host-side impls of the WIT interfaces guests may `import`. Default-deny: a
//! guest only gets a capability its manifest requests *and* an admin grants at
//! install time (`log`, `store-read`, `http-fetch`, `secrets-read`, `kv`,
//! `emit-event` — design §3.3). No raw FS, no raw sockets, no clock-as-entropy.
//!
//! SCAFFOLD: filled in by a later task.
