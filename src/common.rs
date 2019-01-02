use std::fmt::{ Display, Formatter, Error as FmtError };
use std::error::Error;

use self::super::locktype::LockType;

/// Error enum for `glock` crate.
#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    /// This error indicates an unhandled error, usually an internal `Mutex` poison error.
    UnknownError {
        /// The error message from the original `Error`.
        message: String
    },

    /// This error is returned when calling any of the `try_lock` variants, if the target lock is busy.
    LockBusy,

    /// This error is returned when calling any of the `lock_using_parent` variants and passing an
    /// incorrect parent `GLockGuard` (i.e. a `GLockGuard` that does not belong to the parent `GLock`)
    InvalidParentLock,

    /// This error occurs when upgrading a `GLockGuard` to a type not supported by its parent `GLockGuard`,
    /// if auto upgrade is disabled. Currently, auto upgrade is enabled by default, so this error
    /// should never occur.
    InvalidParentLockType {
        /// The required parent lock type.
        required: LockType,

        /// The actual parent lock type.
        actual: LockType
    },

    /// This error occurs when trying to upgrade a `GLockGuard` to a type to which it is not upgradable.
    InvalidUpgrade {
        /// The original lock type.
        original: LockType,

        /// The target lock type of the upgrade.
        requested: LockType
    },
}

impl Display for LockError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            LockError::UnknownError { message }                     => write!(f, "Unknown error: {}", message),
            LockError::LockBusy                                     => write!(f, "Failed to acquire/upgrade lock; lock is busy"),
            LockError::InvalidParentLock                            => write!(f, "Invalid parent lock"),
            LockError::InvalidParentLockType { required, actual }   => write!(f, "Invalid parent lock type; required: {}, actual: {}", required, actual),
            LockError::InvalidUpgrade { original, requested }       => write!(f, "Lock of type {} is not upgradable to type {}", original, requested),
        }
    }
}

impl Error for LockError {}


pub fn map_unknown_err<T: Error>(error: T) -> LockError { LockError::UnknownError { message: format!("{}", error) } }


/// A type alias for `Result<T, LockError>`.
pub type LockResult<T> = Result<T, LockError>;
