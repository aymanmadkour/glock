use std::fmt::{ Display, Formatter, Error as FmtError };

/// The type of lock that can be acquired for a `GLock`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LockType {
    /// Before a `Shared` lock is acquired for a child `GLock`, `IntentionShared` locks must be
    /// acquired for each of its ancestors.
    ///
    /// # Compatibility
    ///
    /// `IntentionShared`: Yes
    ///
    /// `IntentionExclusive`: Yes
    ///
    /// `Shared`: Yes
    ///
    /// `SharedIntentionExclusive`: Yes
    ///
    /// `Exclusive`: No
    ///
    IntentionShared,

    /// Before an `Exclusive` or `SharedIntentionExclusive` lock is acquired for a child `GLock`,
    /// `IntentionExclusive` locks must be acquired for each of its ancestors.
    ///
    /// # Compatibility
    ///
    /// `IntentionShared`: Yes
    ///
    /// `IntentionExclusive`: Yes
    ///
    /// `Shared`: No
    ///
    /// `SharedIntentionExclusive`: No
    ///
    /// `Exclusive`: No
    ///
    IntentionExclusive,

    /// A `Shared` lock grants read access to its protected data. Before acquiring a `Shared` lock for a
    /// child `GLock`, `IntentionShared` (or more restrictive) locks must be acquired for all its ancestors.
    ///
    /// # Compatibility
    ///
    /// `IntentionShared`: Yes
    ///
    /// `IntentionExclusive`: No
    ///
    /// `Shared`: Yes
    ///
    /// `SharedIntentionExclusive`: No
    ///
    /// `Exclusive`: No
    ///
    Shared,

    /// A `SharedIntentionExclusive` lock - as the name implies - is similar to holding both a
    /// `Shared` lock and an `IntentionExclusive` lock at the same time. Before acquiring a
    /// `SharedIntentionExclusive` lock for a child `GLock`, `IntentionExclusive` (or more
    /// restrictive) locks must be acquired for all its ancestors.
    ///
    /// # Compatibility
    ///
    /// `IntentionShared`: Yes
    ///
    /// `IntentionExclusive`: No
    ///
    /// `Shared`: No
    ///
    /// `SharedIntentionExclusive`: No
    ///
    /// `Exclusive`: No
    ///
    SharedIntentionExclusive,

    /// An `Exclusive` lock grants write access to its protected data. Before acquiring an
    /// `Exclusive` lock for a child `GLock`, `IntentionExclusive` (or more restrictive) locks
    /// must be acquired for all its ancestors.
    ///
    /// # Compatibility
    ///
    /// `IntentionShared`: No
    ///
    /// `IntentionExclusive`: No
    ///
    /// `Shared`: No
    ///
    /// `SharedIntentionExclusive`: No
    ///
    /// `Exclusive`: No
    ///
    Exclusive,
}

pub const LOCK_TYPE_COUNT: usize = 5;

pub const LOCK_EMPTY_COUNTS: [usize; LOCK_TYPE_COUNT] = [0, 0, 0, 0, 0];

const LOCK_TYPES: [LockType; LOCK_TYPE_COUNT] = [
    LockType::IntentionShared,
    LockType::IntentionExclusive,
    LockType::Shared,
    LockType::SharedIntentionExclusive,
    LockType::Exclusive,
];

const LOCK_TYPE_IMPLICIT_PARENT_TYPE: [LockType; LOCK_TYPE_COUNT] = [
    LockType::IntentionShared,
    LockType::IntentionExclusive,
    LockType::IntentionShared,
    LockType::IntentionExclusive,
    LockType::IntentionExclusive
];

const LOCK_TYPE_COMPATIBLE_WITH: [[bool; LOCK_TYPE_COUNT]; LOCK_TYPE_COUNT] = [
    [true,  true,  true,  true,  false],
    [true,  true,  false, false, false],
    [true,  false, true,  false, false],
    [true,  false, false, false, false],
    [false, false, false, false, false],
];

const LOCK_TYPE_UPGRADABLE_TO: [[bool; LOCK_TYPE_COUNT]; LOCK_TYPE_COUNT] = [
    [true,  true,  true,  true,  true],
    [false, true,  false, true,  true],
    [false, false, true,  true,  true],
    [false, false, false, true,  true],
    [false, false, false, false, true],
];

const LOCK_TYPE_SUPPORTS_CHILDREN: [[bool; LOCK_TYPE_COUNT]; LOCK_TYPE_COUNT] = [
    [true,  false, true,  false, false],
    [true,  true,  true,  true,  true],
    [true,  false, true,  false, false],
    [true,  true,  true,  true,  true],
    [true,  true,  true,  true,  true],
];

impl LockType {

    pub fn lock_types() -> &'static [LockType] { &LOCK_TYPES }

    /// Returns the numeric index corresponding to this lock type.
    pub fn index(self) -> usize {
        match self {
            LockType::IntentionShared           => 0,
            LockType::IntentionExclusive        => 1,
            LockType::Shared                    => 2,
            LockType::SharedIntentionExclusive  => 3,
            LockType::Exclusive                 => 4,
        }
    }

    /// Returns the implicit parent lock type for this lock type. This means that, before acquiring
    /// this type of lock for a child `GLock`, locks of the implicit parent type must be acquired
    /// for all its ancestor `GLock`s.
    pub fn implicit_parent_type(self) -> LockType { LOCK_TYPE_IMPLICIT_PARENT_TYPE[self.index()] }

    /// Returns `true` if the lock type is compatible with the specified lock type, `false` otherwise.
    pub fn compatible_with(self, other_type: LockType) -> bool { LOCK_TYPE_COMPATIBLE_WITH[self.index()][other_type.index()] }

    /// Returns `true` if the lock type is upgradable to the specified lock type, `false` otherwise.
    pub fn upgradable_to(self, other_type: LockType) -> bool { LOCK_TYPE_UPGRADABLE_TO[self.index()][other_type.index()] }

    /// Returns `true` if the lock type can support child locks of the specified type, `false` otherwise.
    /// If `true`, this means that if a lock of this type is acquired for a parent `GLock`, a lock
    /// of the specified type can be acquired for a child `GLock`.
    pub fn supports_children(self, other_type: LockType) -> bool { LOCK_TYPE_SUPPORTS_CHILDREN[self.index()][other_type.index()] }

    /// Returns the least restrictive lock type that this lock type can be upgraded to, that is at
    /// least as restrictive as the specified type.
    pub fn min_upgradable(self, other_type: LockType) -> LockType {
        for lt in LOCK_TYPES.iter() {
            if self.upgradable_to(*lt) && other_type.upgradable_to(*lt) {
                return *lt;
            }
        }

        return LockType::Exclusive;
    }
}

impl Display for LockType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match *self {
            LockType::IntentionShared           => { write!(f, "IntentionShared") },
            LockType::IntentionExclusive        => { write!(f, "IntentionExclusive") },
            LockType::Shared                    => { write!(f, "Shared") },
            LockType::SharedIntentionExclusive  => { write!(f, "SharedIntentionExclusive") },
            LockType::Exclusive                 => { write!(f, "Exclusive") },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn implicit_parent_type() {
        assert_eq!(LockType::IntentionShared.implicit_parent_type(), LockType::IntentionShared);
        assert_eq!(LockType::IntentionExclusive.implicit_parent_type(), LockType::IntentionExclusive);
        assert_eq!(LockType::Shared.implicit_parent_type(), LockType::IntentionShared);
        assert_eq!(LockType::SharedIntentionExclusive.implicit_parent_type(), LockType::IntentionExclusive);
        assert_eq!(LockType::Exclusive.implicit_parent_type(), LockType::IntentionExclusive);
    }

    #[test]
    fn compatible_with() {
        assert_eq!(LockType::IntentionShared.compatible_with(LockType::IntentionShared), true);
        assert_eq!(LockType::IntentionShared.compatible_with(LockType::IntentionExclusive), true);
        assert_eq!(LockType::IntentionShared.compatible_with(LockType::Shared), true);
        assert_eq!(LockType::IntentionShared.compatible_with(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::IntentionShared.compatible_with(LockType::Exclusive), false);

        assert_eq!(LockType::IntentionExclusive.compatible_with(LockType::IntentionShared), true);
        assert_eq!(LockType::IntentionExclusive.compatible_with(LockType::IntentionExclusive), true);
        assert_eq!(LockType::IntentionExclusive.compatible_with(LockType::Shared), false);
        assert_eq!(LockType::IntentionExclusive.compatible_with(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::IntentionExclusive.compatible_with(LockType::Exclusive), false);

        assert_eq!(LockType::Shared.compatible_with(LockType::IntentionShared), true);
        assert_eq!(LockType::Shared.compatible_with(LockType::IntentionExclusive), false);
        assert_eq!(LockType::Shared.compatible_with(LockType::Shared), true);
        assert_eq!(LockType::Shared.compatible_with(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::Shared.compatible_with(LockType::Exclusive), false);

        assert_eq!(LockType::SharedIntentionExclusive.compatible_with(LockType::IntentionShared), true);
        assert_eq!(LockType::SharedIntentionExclusive.compatible_with(LockType::IntentionExclusive), false);
        assert_eq!(LockType::SharedIntentionExclusive.compatible_with(LockType::Shared), false);
        assert_eq!(LockType::SharedIntentionExclusive.compatible_with(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::SharedIntentionExclusive.compatible_with(LockType::Exclusive), false);

        assert_eq!(LockType::Exclusive.compatible_with(LockType::IntentionShared), false);
        assert_eq!(LockType::Exclusive.compatible_with(LockType::IntentionExclusive), false);
        assert_eq!(LockType::Exclusive.compatible_with(LockType::Shared), false);
        assert_eq!(LockType::Exclusive.compatible_with(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::Exclusive.compatible_with(LockType::Exclusive), false);
    }

    #[test]
    fn upgradable_to() {
        assert_eq!(LockType::IntentionShared.upgradable_to(LockType::IntentionShared), true);
        assert_eq!(LockType::IntentionShared.upgradable_to(LockType::IntentionExclusive), true);
        assert_eq!(LockType::IntentionShared.upgradable_to(LockType::Shared), true);
        assert_eq!(LockType::IntentionShared.upgradable_to(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::IntentionShared.upgradable_to(LockType::Exclusive), true);

        assert_eq!(LockType::IntentionExclusive.upgradable_to(LockType::IntentionShared), false);
        assert_eq!(LockType::IntentionExclusive.upgradable_to(LockType::IntentionExclusive), true);
        assert_eq!(LockType::IntentionExclusive.upgradable_to(LockType::Shared), false);
        assert_eq!(LockType::IntentionExclusive.upgradable_to(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::IntentionExclusive.upgradable_to(LockType::Exclusive), true);

        assert_eq!(LockType::Shared.upgradable_to(LockType::IntentionShared), false);
        assert_eq!(LockType::Shared.upgradable_to(LockType::IntentionExclusive), false);
        assert_eq!(LockType::Shared.upgradable_to(LockType::Shared), true);
        assert_eq!(LockType::Shared.upgradable_to(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::Shared.upgradable_to(LockType::Exclusive), true);

        assert_eq!(LockType::SharedIntentionExclusive.upgradable_to(LockType::IntentionShared), false);
        assert_eq!(LockType::SharedIntentionExclusive.upgradable_to(LockType::IntentionExclusive), false);
        assert_eq!(LockType::SharedIntentionExclusive.upgradable_to(LockType::Shared), false);
        assert_eq!(LockType::SharedIntentionExclusive.upgradable_to(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::SharedIntentionExclusive.upgradable_to(LockType::Exclusive), true);

        assert_eq!(LockType::Exclusive.upgradable_to(LockType::IntentionShared), false);
        assert_eq!(LockType::Exclusive.upgradable_to(LockType::IntentionExclusive), false);
        assert_eq!(LockType::Exclusive.upgradable_to(LockType::Shared), false);
        assert_eq!(LockType::Exclusive.upgradable_to(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::Exclusive.upgradable_to(LockType::Exclusive), true);
    }

    #[test]
    fn supports_children() {
        assert_eq!(LockType::IntentionShared.supports_children(LockType::IntentionShared), true);
        assert_eq!(LockType::IntentionShared.supports_children(LockType::IntentionExclusive), false);
        assert_eq!(LockType::IntentionShared.supports_children(LockType::Shared), true);
        assert_eq!(LockType::IntentionShared.supports_children(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::IntentionShared.supports_children(LockType::Exclusive), false);

        assert_eq!(LockType::IntentionExclusive.supports_children(LockType::IntentionShared), true);
        assert_eq!(LockType::IntentionExclusive.supports_children(LockType::IntentionExclusive), true);
        assert_eq!(LockType::IntentionExclusive.supports_children(LockType::Shared), true);
        assert_eq!(LockType::IntentionExclusive.supports_children(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::IntentionExclusive.supports_children(LockType::Exclusive), true);

        assert_eq!(LockType::Shared.supports_children(LockType::IntentionShared), true);
        assert_eq!(LockType::Shared.supports_children(LockType::IntentionExclusive), false);
        assert_eq!(LockType::Shared.supports_children(LockType::Shared), true);
        assert_eq!(LockType::Shared.supports_children(LockType::SharedIntentionExclusive), false);
        assert_eq!(LockType::Shared.supports_children(LockType::Exclusive), false);

        assert_eq!(LockType::SharedIntentionExclusive.supports_children(LockType::IntentionShared), true);
        assert_eq!(LockType::SharedIntentionExclusive.supports_children(LockType::IntentionExclusive), true);
        assert_eq!(LockType::SharedIntentionExclusive.supports_children(LockType::Shared), true);
        assert_eq!(LockType::SharedIntentionExclusive.supports_children(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::SharedIntentionExclusive.supports_children(LockType::Exclusive), true);

        assert_eq!(LockType::Exclusive.supports_children(LockType::IntentionShared), true);
        assert_eq!(LockType::Exclusive.supports_children(LockType::IntentionExclusive), true);
        assert_eq!(LockType::Exclusive.supports_children(LockType::Shared), true);
        assert_eq!(LockType::Exclusive.supports_children(LockType::SharedIntentionExclusive), true);
        assert_eq!(LockType::Exclusive.supports_children(LockType::Exclusive), true);
    }

    #[test]
    fn min_upgradable() {
        assert_eq!(LockType::IntentionShared.min_upgradable(LockType::IntentionShared), LockType::IntentionShared);
        assert_eq!(LockType::IntentionShared.min_upgradable(LockType::IntentionExclusive), LockType::IntentionExclusive);
        assert_eq!(LockType::IntentionShared.min_upgradable(LockType::Shared), LockType::Shared);
        assert_eq!(LockType::IntentionShared.min_upgradable(LockType::SharedIntentionExclusive), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::IntentionShared.min_upgradable(LockType::Exclusive), LockType::Exclusive);

        assert_eq!(LockType::IntentionExclusive.min_upgradable(LockType::IntentionShared), LockType::IntentionExclusive);
        assert_eq!(LockType::IntentionExclusive.min_upgradable(LockType::IntentionExclusive), LockType::IntentionExclusive);
        assert_eq!(LockType::IntentionExclusive.min_upgradable(LockType::Shared), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::IntentionExclusive.min_upgradable(LockType::SharedIntentionExclusive), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::IntentionExclusive.min_upgradable(LockType::Exclusive), LockType::Exclusive);

        assert_eq!(LockType::Shared.min_upgradable(LockType::IntentionShared), LockType::Shared);
        assert_eq!(LockType::Shared.min_upgradable(LockType::IntentionExclusive), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::Shared.min_upgradable(LockType::Shared), LockType::Shared);
        assert_eq!(LockType::Shared.min_upgradable(LockType::SharedIntentionExclusive), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::Shared.min_upgradable(LockType::Exclusive), LockType::Exclusive);

        assert_eq!(LockType::SharedIntentionExclusive.min_upgradable(LockType::IntentionShared), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::SharedIntentionExclusive.min_upgradable(LockType::IntentionExclusive), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::SharedIntentionExclusive.min_upgradable(LockType::Shared), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::SharedIntentionExclusive.min_upgradable(LockType::SharedIntentionExclusive), LockType::SharedIntentionExclusive);
        assert_eq!(LockType::SharedIntentionExclusive.min_upgradable(LockType::Exclusive), LockType::Exclusive);

        assert_eq!(LockType::Exclusive.min_upgradable(LockType::IntentionShared), LockType::Exclusive);
        assert_eq!(LockType::Exclusive.min_upgradable(LockType::IntentionExclusive), LockType::Exclusive);
        assert_eq!(LockType::Exclusive.min_upgradable(LockType::Shared), LockType::Exclusive);
        assert_eq!(LockType::Exclusive.min_upgradable(LockType::SharedIntentionExclusive), LockType::Exclusive);
        assert_eq!(LockType::Exclusive.min_upgradable(LockType::Exclusive), LockType::Exclusive);
    }
}