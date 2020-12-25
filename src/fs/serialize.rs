#[cfg(feature = "binc")]
pub use bincode::{deserialize, serialize};

#[cfg(feature = "binc")]
pub const ENCODING: &str = "bincode";

#[cfg(feature = "json")]
pub use serde_json::{from_slice as deserialize, to_vec as serialize};

#[cfg(feature = "json")]
pub const ENCODING: &str = "json";
