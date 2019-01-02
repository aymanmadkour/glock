use std::fmt::{ Display, Formatter, Error as FmtError };
use std::error::Error;

use self::super::locktype::LockType;

#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    UnknownError { message: String },
    LockBusy,
    LockAlreadyUsed,
    InvalidParentLock,
    InvalidParentLockType { required: LockType, actual: LockType },
    InvalidUpgrade { original: LockType, requested: LockType },
}

impl Display for LockError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            LockError::UnknownError { message }                     => write!(f, "Unknown error: {}", message),
            LockError::LockBusy                                     => write!(f, "Failed to acquire/upgrade lock; lock is busy"),
            LockError::LockAlreadyUsed                              => write!(f, "Cannot create lock; lock is already used"),
            LockError::InvalidParentLock                            => write!(f, "Invalid parent lock"),
            LockError::InvalidParentLockType { required, actual }   => write!(f, "Invalid parent lock type; required: {}, actual: {}", required, actual),
            LockError::InvalidUpgrade { original, requested }       => write!(f, "Lock of type {} is not upgradable to type {}", original, requested),
        }
    }
}

impl Error for LockError {}


pub fn map_unknown_err<T: Error>(error: T) -> LockError { LockError::UnknownError { message: format!("{}", error) } }


pub type LockResult<T> = Result<T, LockError>;
