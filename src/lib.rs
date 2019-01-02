//! Granular locking crate for Rust. Instead of using coarse-grained `Mutex` or `RwLock` which can be
//! used to lock an entire structure, `glock` provides more granular locking more granular locking.
//!
//! Code is hosted on github.com:
//!
//! `git clone https://github.com/aymanmadkour/glock`
//!
mod common;
mod locktype;
mod lock;
mod kernel;

pub use self::common::LockError;
pub use self::common::LockResult;

pub use self::locktype::LockType;

pub use self::lock::GLock;
pub use self::lock::GLockBuilder;
pub use self::lock::GLockGuard;
pub use self::lock::GLockGuardMut;
