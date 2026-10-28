// Keep the platform USB FFI in this small crate. The companion application
// itself forbids unsafe code and only consumes the typed transport API.
pub mod protocol;

pub mod transport;

pub use protocol::*;
pub use transport::*;
