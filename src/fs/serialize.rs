#[cfg(feature = "binc")]
pub use bincode::{deserialize, serialize};

#[cfg(feature = "binc")]
pub const ENCODING: &str = "bincode";

#[cfg(all(feature = "json", not(feature = "binc")))]
pub use serde_json::{from_slice as deserialize, to_vec as serialize};

#[cfg(all(feature = "json", not(feature = "binc")))]
pub const ENCODING: &str = "json";
