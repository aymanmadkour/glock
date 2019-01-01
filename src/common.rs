use std::hash::Hash;
use std::fmt::{ Display, Debug, Formatter, Error as FmtError };
use std::error::Error;

use self::super::locktype::LockType;

#[derive(Debug, PartialEq, Eq)]
pub enum LockError<I: Clone + Eq + Hash + Display + Debug> {
    UnknownError { message: String },
    LockBusy { path: LockPath<I> },
    LockAlreadyUsed { path: LockPath<I> },
    InvalidParentLock { expected_path: LockPath<I>, actual_path: LockPath<I> },
    InvalidParentLockType { path: LockPath<I>, required: LockType, actual: LockType },
    InvalidUpgrade { original: LockType, requested: LockType },
}

impl<I: Clone + Eq + Hash + Display + Debug> Display for LockError<I> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            LockError::UnknownError { message }                         => write!(f, "Unknown error: {}", message),
            LockError::LockBusy { path }                                => write!(f, "Failed to lock/upgrade path: {}; lock is busy", path),
            LockError::LockAlreadyUsed { path }                         => write!(f, "Cannot create lock for path: {}; lock is already used", path),
            LockError::InvalidParentLock { expected_path, actual_path } => write!(f, "Invalid parent lock; expected: {}, actual: {}", expected_path, actual_path),
            LockError::InvalidParentLockType { path, required, actual } => write!(f, "Invalid parent lock type for path {}; required: {}, actual: {}", path, required, actual),
            LockError::InvalidUpgrade { original, requested }           => write!(f, "Lock of type {} is not upgradable to type {}", original, requested),
        }
    }
}

impl<I: Clone + Eq + Hash + Display + Debug> Error for LockError<I> {}


pub fn map_unknown_err<I: Clone + Eq + Hash + Display + Debug, T: Error>(error: T) -> LockError<I> { LockError::UnknownError { message: format!("{}", error) } }


pub type LockResult<I, T> = Result<T, LockError<I>>;


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockPath<I: Clone + Eq + Hash + Display + Debug> {
    items: Vec<I>
}

impl<I: Clone + Eq + Hash + Display + Debug> LockPath<I> {
    pub fn new() -> LockPath<I> { LockPath { items: Vec::new() } }
    pub fn add(&mut self, item: I) { self.items.push(item); }
    pub fn builder() -> LockPathBuilder<I> { LockPathBuilder::new() }
}

impl<I: Clone + Eq + Hash + Display + Debug> Display for LockPath<I> {
    fn fmt(&self, f: & mut Formatter) -> Result<(), FmtError> {
        write!(f, "[")?;
        let mut sep = "";
        for i in self.items.iter() {
            write!(f, "{}{}", sep, i)?;
            sep = ":";
        }
        write!(f, "]")
    }
}


pub struct LockPathBuilder<I: Clone + Eq + Hash + Display + Debug> {
    items: Vec<I>
}

impl<I: Clone + Eq + Hash + Display + Debug> LockPathBuilder<I> {
    pub fn new() -> LockPathBuilder<I> { LockPathBuilder { items: Vec::new() } }

    pub fn add(mut self, item: I) -> LockPathBuilder<I> {
        self.items.push(item);
        return self;
    }

    pub fn build(self) -> LockPath<I> { LockPath { items: self.items } }
}
