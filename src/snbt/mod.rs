//! Implements stringified NBT (SNBT) support. This is the human-readable NBT
//! format that is often used in Minecraft commands.

mod de;
mod ser;

pub use de::{Deserializer, from_string};
pub use ser::{Serializer, to_string};
