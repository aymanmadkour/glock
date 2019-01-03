use std::ops::{ Deref, DerefMut };
use std::sync::Arc;

use self::super::common::*;
use self::super::locktype::*;
use self::super::kernel::*;


/// A `GLockBuilder` can be used to construct nested `GLock`s. In Rust, inner `struct`s are
/// initialized before their outer container `struct`s. In `glock`, parent lock kernels must be
/// initialized before child lock kernels. To resolve this, a `GLockBuilder` can be used to
/// initialize the parent lock kernel first, then accept the containing `struct` in `build()`.
///
/// # Example
///
///
/// ```
/// use glock::{ GLock, GLockBuilder };
///
/// struct Parent {
///     child1: GLock<u32>,
///     child2: GLock<u32>,
/// }
///
/// let parent_lock = {
///
///     let parent_lock_builder = GLockBuilder::new_root_builder();
///
///     let parent = Parent {
///         child1: parent_lock_builder.new_child(0u32).unwrap(),
///         child2: parent_lock_builder.new_child(0u32).unwrap(),
///     };
///
///     parent_lock_builder.build(parent).unwrap()
/// };
/// ```
pub struct GLockBuilder {
    kernel: LockKernelRc,
}

impl GLockBuilder {

    /// Creates a new root `GLock` builder
    pub fn new_root_builder() -> GLockBuilder {
        GLockBuilder { kernel: LockKernelRc::new(LockKernel::new(None, None)) }
    }

    /// Creates a builder for a `GLock` that is a child of the current `GLock`.
    pub fn new_child_builder(&self) -> LockResult<GLockBuilder> {
        self.kernel
            .new_child()
            .map(|child_kernel| GLockBuilder { kernel: child_kernel })
    }

    /// Creates a new `Glock` that is a child of the current `GLock` and protects the specified.
    pub fn new_child<T>(&self, data: T) -> LockResult<GLock<T>> {
        self.new_child_builder().and_then(|cb| cb.build(data))
    }

    /// Builds the `GLock` object that protects the specified `data`.
    pub fn build<T>(self, data: T) -> LockResult<GLock<T>> {
        self.kernel.own()
            .map(|_| GLock {
                kernel: self.kernel,
                data,
            })
    }
}

/// Represents a granular lock object. A `GLock` is used to protect a data value of type `T`, which
/// can only be accessed with a mutable reference after calling `lock_exclusive()`,
/// `try_lock_exclusive()`, `lock_exclusive_using_parent()` or `try_lock_exclusive_using_parent()`.
///
/// Each `GLock` can have zero or more child `GLock`s, which can be nested (i.e. placed inside the
/// protected data) or non-nested. The same locking rules apply in both cases.
///
/// When locking a child `GLock`, first you need to lock its parent `GLock`, then lock it by calling
/// `lock_using_parent()`, `try_lock_using_parent()`, `lock_exclusive_using_parent()` or
/// `lock_exclusive_using_parent()`, and passing a reference to the parent's `GLockGuard`.
///
/// If you do not lock the parent and proceed to lock the child `GLock` directly using `lock()`,
/// `try_lock()`, `lock_exclusive()` or `try_lock_exclusive()`, an implicit lock will be acquired
/// for the parent `GLock` that will be release when dropping this lock's `GLockGuard`.
#[derive(Debug)]
pub struct GLock< T> {
    kernel: LockKernelRc,
    data: T,
}

impl< T> GLock<T> {

    /// Creates a new root `GLockBuilder`. This is similar to calling `GLockBuilder::new_root_builder()`.
    pub fn new_root_builder() -> GLockBuilder { GLockBuilder::new_root_builder() }

    /// Creates a new root `GLock` protecting the specified data.
    pub fn new_root(data: T) -> LockResult<GLock<T>> { GLockBuilder::new_root_builder().build(data) }

    /// Creates a `GLockBuilder` for a lock that is a child of the current `GLock`.
    pub fn new_child_builder(&self) -> LockResult<GLockBuilder> {
        self.kernel
            .new_child()
            .map(|child_kernel| GLockBuilder { kernel: child_kernel })
    }

    /// Creates a `GLock` that is a child of the current `GLock`, protecting the specified data.
    pub fn new_child<T2>(&self, data: T2) -> LockResult<GLock<T2>> {
        self.new_child_builder().and_then(|cb| cb.build(data))
    }

    /// Acquires a lock of the specified type on the current `GLock`. If the lock is busy, it will
    /// block until it is ready. If this is a child `GLock`, it will implicitly acquire the
    /// appropriate lock on its parent `GLock`.
    /// 
    /// If you are trying to acquire an `Exclusive` lock, it is better to use `lock_exclusive()`,
    /// because the `GLockGuard` returned by `lock()` will not allow mutation of protected data.
    pub fn lock(&self, lock_type: LockType) -> LockResult<GLockGuard<T>> {
        self.do_lock::<()>(lock_type, None, false)
    }

    /// Attempts to acquire a lock of the specified type on the current `GLock`. If the lock is busy,
    /// it will return a `LockError::LockBusy` error. If this is a child `GLock`, it will implicitly
    /// attempt to acquire the appropriate lock on its parent `GLock`.
    /// 
    /// If you are trying to acquire an `Exclusive` lock, it is better to use `try_lock_exclusive()`,
    /// because the `GLockGuard` returned by `try_lock()` will not allow mutation of protected data.
    pub fn try_lock(&self, lock_type: LockType) -> LockResult<GLockGuard<T>> {
        self.do_lock::<()>(lock_type, None, true)
    }

    /// Acquires a lock of the specified type on the current child `GLock`, using the specified
    /// `GLockGuard` of the parent `GLock`. If the lock is busy, it will block until it is ready.
    /// 
    /// If you are trying to acquire an `Exclusive` lock, it is better to use
    /// `lock_exclusive_using_parent()`, because the `GLockGuard` returned by
    /// `lock_using_parent()` will not allow mutation of protected data.
    pub fn lock_using_parent<T2>(&self, lock_type: LockType, parent: &GLockGuard<T2>) -> LockResult<GLockGuard<T>> {
        self.do_lock(lock_type, Some(parent), false)
    }

    /// Attempts to acquire a lock of the specified type on the current child `GLock`, using the
    /// specified `GLockGuard` of the parent `GLock`. If the lock is busy, it will return a
    /// `LockError::LockBusy` error.
    /// 
    /// If you are trying to acquire an `Exclusive` lock, it is better to use
    /// `try_lock_exclusive_using_parent()`, because the `GLockGuard` returned by
    /// `try_lock_using_parent()` will not allow mutation of protected data.
    pub fn try_lock_using_parent<T2>(&self, lock_type: LockType, parent: &GLockGuard<T2>) -> LockResult<GLockGuard<T>> {
        self.do_lock(lock_type, Some(parent), true)
    }

    /// Acquires an `Exclusive` lock on the current `GLock`. If the lock is busy, it will block
    /// until it is ready. If this is a child `GLock`, it will implicitly acquire the appropriate
    /// lock on its parent `GLock`.
    ///
    /// The returned `GLockGuardMut` allows mutating the protected data.
    pub fn lock_exclusive(&self) -> LockResult<GLockGuardMut<T>> {
        self.do_lock_exclusive::<()>(None, false)
    }

    /// Attempts to acquire an `Exclusive` lock on the current `GLock`. If the lock is busy,
    /// it will return a `LockError::LockBusy` error. If this is a child `GLock`, it will implicitly
    /// attempt to acquire the appropriate lock on its parent `GLock`.
    ///
    /// The returned `GLockGuardMut` allows mutating the protected data.
    pub fn try_lock_exclusive(&self) -> LockResult<GLockGuardMut<T>> {
        self.do_lock_exclusive::<()>(None, true)
    }

    /// Acquires an `Exclusive` lock on the current child `GLock`, using the specified `GLockGuard`
    /// of the parent `GLock`. If the lock is busy, it will block until it is ready.
    ///
    /// The returned `GLockGuardMut` allows mutating the protected data.
    pub fn lock_exclusive_using_parent<T2>(&self, parent: &GLockGuard<T2>) -> LockResult<GLockGuardMut<T>> {
        self.do_lock_exclusive(Some(parent), false)
    }

    /// Attempts to acquire an `Exclusive` lock on the current child `GLock`, using the
    /// specified `GLockGuard` of the parent `GLock`. If the lock is busy, it will return a
    /// `LockError::LockBusy` error.
    ///
    /// The returned `GLockGuardMut` allows mutating the protected data.
    pub fn try_lock_exclusive_using_parent<T2>(&self, parent: &GLockGuard<T2>) -> LockResult<GLockGuardMut<T>> {
        self.do_lock_exclusive(Some(parent), true)
    }

    fn do_lock<T2>(&self, lock_type: LockType, parent: Option<&GLockGuard<T2>>, try_only: bool) -> LockResult<GLockGuard<T>> {
        self.kernel
            .acquire(lock_type, parent.map(|p| p.lock_instance.clone()), true, try_only)
            .map(|lock_instance| GLockGuard { lock: self, lock_instance })
    }

    fn do_lock_exclusive<T2>(&self, parent: Option<&GLockGuard<T2>>, try_only: bool) -> LockResult<GLockGuardMut<T>> {
        self.do_lock(LockType::Exclusive, parent, try_only).map(|lg| GLockGuardMut { lock_guard: lg })
    }

    fn data_ptr(&self) -> *mut T {
        (&self.data as *const T) as *mut T
    }
}

impl< T> Drop for GLock<T> {
    fn drop(&mut self) {
        self.kernel
            .unown()
            .unwrap();
    }
}


/// A `GLockGuard` represents an acquired lock instance of any type. It can be used to access the
/// protected data. The lock is released by dropping the `GLockGuard` object.
#[derive(Debug)]
pub struct GLockGuard<'lck, T: 'lck> {
    lock: &'lck GLock<T>,
    lock_instance: Arc<LockInstance>,
}

impl<'lck, T: 'lck> GLockGuard<'lck, T> {

    /// Returns the type of the lock currently held.
    pub fn lock_type(&self) -> LockResult<LockType> {
        self.lock_instance.lock_type()
    }

    /// Upgrades the type of this `GLockGuard` to the specified type. If parent lock does not support
    /// the new type, it will be upgraded as well. If the lock is currently busy, it will block until
    /// it is ready.
    pub fn upgrade(&self, to_type: LockType) -> LockResult<()> {
        self.lock_instance.upgrade(to_type, true, false)
    }

    /// Attempts to upgrade the type of this `GLockGuard` to the specified type. If parent lock
    /// does not support the new type, it will be upgraded as well. If the lock is currently busy,
    /// it will return a `LockError::LockBusy` error.
    pub fn try_upgrade(&self, to_type: LockType) -> LockResult<()> {
        self.lock_instance.upgrade(to_type, true, true)
    }

    /// Upgrades the type of this `GLockGuard` to `Exclusive`. If parent lock does not support
    /// the new type, it will be upgraded as well. If the lock is currently busy, it will block until
    /// it is ready.
    ///
    /// This method consumes the current `GLockGuard` and returns a new `GLockGuardMut`.
    /// In case of failure, it will return a tuple containing the error as well as the original
    /// `GLockGuard`.
    pub fn upgrade_to_exclusive(self) -> Result<GLockGuardMut<'lck, T>, (LockError, GLockGuard<'lck, T>)> {
        match self.lock_instance.upgrade(LockType::Exclusive, true, false) {
            Ok(_)   => { Ok(GLockGuardMut { lock_guard: self }) },
            Err(e)  => { Err((e, self)) },
        }
    }

    /// Attempts to upgrade the type of this `GLockGuard` to `Exclusive`. If parent lock does not support
    /// the new type, it will be upgraded as well. If the lock is currently busy, If the lock is
    /// currently busy, it will return a `LockError::LockBusy` error.
    ///
    /// This method consumes the current `GLockGuard` and returns a new `GLockGuardMut`.
    /// In case of failure, it will return a tuple containing the error as well as the original
    /// `GLockGuard`.
    pub fn try_upgrade_to_exclusive(self) -> Result<GLockGuardMut<'lck, T>, (LockError, GLockGuard<'lck, T>)> {
        match self.lock_instance.upgrade(LockType::Exclusive, true, true) {
            Ok(_)   => { Ok(GLockGuardMut { lock_guard: self }) },
            Err(e)  => { Err((e, self)) },
        }
    }
}

impl<'lck, T: 'lck> Deref for GLockGuard<'lck, T> {
    type Target = T;

    fn deref(&self) -> &<Self as Deref>::Target {
        unsafe { &*self.lock.data_ptr() }
    }
}

/// A `GLockGuard` represents an acquired `Exclusive` lock instance. It can be used to read as well
/// as mutate  the protected data. The lock is released by dropping the `GLockGuardMut` object.
#[derive(Debug)]
pub struct GLockGuardMut<'lck, T: 'lck> {
    lock_guard: GLockGuard<'lck, T>,
}

impl<'lck, T: 'lck> Deref for GLockGuardMut<'lck, T> {
    type Target = T;
    fn deref(&self) -> &<Self as Deref>::Target { self.lock_guard.deref() }
}

impl<'lck, T: 'lck> DerefMut for GLockGuardMut<'lck, T> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        unsafe { &mut *self.lock_guard.lock.data_ptr() }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn non_nested_locks() {
        let p = GLock::new_root(0u32).unwrap();

        let c1 = p.new_child(0u32).unwrap();
        let c2 = p.new_child(0u32).unwrap();

        for p_type1 in LockType::lock_types() {
            let p_lg1 = p.try_lock(*p_type1).unwrap();

            for p_type2 in LockType::lock_types() {
                assert_eq!(p.try_lock(*p_type2).is_ok(), p_type1.compatible_with(*p_type2));
            }

            for c1_type1 in LockType::lock_types() {
                let _c1_lg1 = c1.try_lock_using_parent(*c1_type1, &p_lg1).unwrap();

                for c1_type2 in LockType::lock_types() {
                    assert_eq!(c1.try_lock_using_parent(*c1_type2, &p_lg1).is_ok(), c1_type1.compatible_with(*c1_type2));
                }

                for c2_type in LockType::lock_types() {
                    c2.try_lock_using_parent(*c2_type, &p_lg1).unwrap();
                }
            }
        }
    }

    #[test]
    fn nested_locks() {

        struct Parent {
            child1: GLock<u32>,
            child2: GLock<u32>,
        };

        let parent_lock = {
            let parent_lb = GLock::<Parent>::new_root_builder();

            let parent = Parent {
                child1: parent_lb.new_child(0u32).unwrap(),
                child2: parent_lb.new_child(0u32).unwrap(),
            };

            parent_lb.build(parent).unwrap()
        };

        for p_type1 in LockType::lock_types() {
            let p_lg1 = parent_lock.try_lock(*p_type1).unwrap();

            for p_type2 in LockType::lock_types() {
                assert_eq!(parent_lock.try_lock(*p_type2).is_ok(), p_type1.compatible_with(*p_type2));
            }

            for c1_type1 in LockType::lock_types() {
                let _c1_lg1 = p_lg1.child1.try_lock_using_parent(*c1_type1, &p_lg1).unwrap();

                for c1_type2 in LockType::lock_types() {
                    assert_eq!(p_lg1.child1.try_lock_using_parent(*c1_type2, &p_lg1).is_ok(), c1_type1.compatible_with(*c1_type2));
                }

                for c2_type in LockType::lock_types() {
                    p_lg1.child2.try_lock_using_parent(*c2_type, &p_lg1).unwrap();
                }
            }
        }
    }

    #[test]
    fn upgrade_to_exclusive() {

        let p = GLock::new_root(0u32).unwrap();

        let c = p.new_child(0u32).unwrap();

        let c_g = c.lock(LockType::Shared).unwrap();

        assert_eq!(p.try_lock(LockType::Shared).is_ok(), true);

        let mut c_g_mut = c_g.upgrade_to_exclusive().unwrap();
        *c_g_mut = 10;

        assert_eq!(p.try_lock(LockType::Shared).is_ok(), false);
    }
}
