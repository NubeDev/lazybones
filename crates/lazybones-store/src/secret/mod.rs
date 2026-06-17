//! The secret store: agent CLI credentials, encrypted at rest.
//!
//! lazybones runs an agent CLI per task (claude, codex, …); each reads its key
//! from the environment. The user registers those keys through the app, the
//! daemon seals them with AES-256-GCM under a master key, and stores only the
//! ciphertext in SurrealDB. Two read shapes: safe [`SecretMeta`] for listing
//! (no plaintext) and decrypted [`SecretEnv`] pairs for the trusted loop to
//! export at agent spawn.

mod cipher;
mod delete;
mod env;
mod list;
mod model;
mod put;
mod row;

pub(crate) use cipher::Cipher;
pub use delete::delete_secret;
pub use env::secret_env;
pub use list::list_secrets;
pub use model::{SecretEnv, SecretMeta};
pub use put::put_secret;
