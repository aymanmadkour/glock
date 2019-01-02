use std::ops::{ Deref, DerefMut };
use std::sync::Arc;

use self::super::common::*;
use self::super::locktype::*;
use self::super::kernel::*;


pub struct LockBuilder  {
    kernel: LockKernelRc,
}

impl LockBuilder {

    pub fn new_root_builder() -> LockBuilder {
        LockBuilder { kernel: LockKernelRc::new(LockKernel::new(None, None)) }
    }

    pub fn new_child_builder(&self) -> LockResult<LockBuilder> {
        self.kernel
            .new_child()
            .map(|child_kernel| LockBuilder { kernel: child_kernel })
    }

    pub fn new_child<T>(&self, data: T) -> LockResult<Lock<T>> {
        self.new_child_builder().and_then(|cb| cb.build(data))
    }

    pub fn build<T>(self, data: T) -> LockResult<Lock<T>> {
        self.kernel.own()
            .map(|_| Lock {
                kernel: self.kernel,
                data,
            })
    }
}


pub struct Lock< T> {
    kernel: LockKernelRc,
    data: T,
}

impl< T> Lock<T> {

    pub fn new_root_builder() -> LockBuilder { LockBuilder::new_root_builder() }

    pub fn new_root(data: T) -> LockResult<Lock<T>> { LockBuilder::new_root_builder().build(data) }

    pub fn new_child_builder(&self) -> LockResult<LockBuilder> {
        self.kernel
            .new_child()
            .map(|child_kernel| LockBuilder { kernel: child_kernel })
    }

    pub fn new_child<T2>(&self, data: T2) -> LockResult<Lock<T2>> {
        self.new_child_builder().and_then(|cb| cb.build(data))
    }

    pub fn lock(&self, lock_type: LockType) -> LockResult<LockGuard<T>> {
        self.do_lock::<()>(lock_type, None, false)
    }

    pub fn try_lock(&self, lock_type: LockType) -> LockResult<LockGuard<T>> {
        self.do_lock::<()>(lock_type, None, true)
    }

    pub fn lock_using_parent<T2>(&self, lock_type: LockType, parent: &LockGuard<T2>) -> LockResult<LockGuard<T>> {
        self.do_lock(lock_type, Some(parent), false)
    }

    pub fn try_lock_using_parent<T2>(&self, lock_type: LockType, parent: &LockGuard<T2>) -> LockResult<LockGuard<T>> {
        self.do_lock(lock_type, Some(parent), true)
    }

    pub fn lock_exclusive(&self) -> LockResult<LockGuardMut<T>> {
        self.do_lock_exclusive::<()>(None, false)
    }

    pub fn try_lock_exclusive(&self) -> LockResult<LockGuardMut<T>> {
        self.do_lock_exclusive::<()>(None, true)
    }

    pub fn lock_exclusive_using_parent<T2>(&self, parent: &LockGuard<T2>) -> LockResult<LockGuardMut<T>> {
        self.do_lock_exclusive(Some(parent), false)
    }

    pub fn try_lock_exclusive_using_parent<T2>(&self, parent: &LockGuard<T2>) -> LockResult<LockGuardMut<T>> {
        self.do_lock_exclusive(Some(parent), true)
    }

    fn do_lock<T2>(&self, lock_type: LockType, parent: Option<&LockGuard<T2>>, try_only: bool) -> LockResult<LockGuard<T>> {
        self.kernel
            .acquire(lock_type, parent.map(|p| p.lock_instance.clone()), true, try_only)
            .map(|lock_instance| LockGuard { lock: self, lock_instance })
    }

    fn do_lock_exclusive<T2>(&self, parent: Option<&LockGuard<T2>>, try_only: bool) -> LockResult<LockGuardMut<T>> {
        self.do_lock(LockType::Exclusive, parent, try_only).map(|lg| LockGuardMut { lock_guard: lg })
    }

    fn data_ptr(&self) -> *mut T {
        (&self.data as *const T) as *mut T
    }
}

impl< T> Drop for Lock<T> {
    fn drop(&mut self) {
        self.kernel
            .unown()
            .unwrap();
    }
}


pub struct LockGuard<'lck, T: 'lck> {
    lock: &'lck Lock<T>,
    lock_instance: Arc<LockInstance>,
}

impl<'lck, T: 'lck> LockGuard<'lck, T> {

    pub fn lock_type(&self) -> LockResult<LockType> {
        self.lock_instance.lock_type()
    }

    pub fn upgrade(&self, to_type: LockType) -> LockResult<()> {
        self.lock_instance.upgrade(to_type, true, false)
    }

    pub fn try_upgrade(&self, to_type: LockType) -> LockResult<()> {
        self.lock_instance.upgrade(to_type, true, true)
    }
}

impl<'lck, T: 'lck> Deref for LockGuard<'lck, T> {
    type Target = T;

    fn deref(&self) -> &<Self as Deref>::Target {
        unsafe { &*self.lock.data_ptr() }
    }
}


pub struct LockGuardMut<'lck, T: 'lck> {
    lock_guard: LockGuard<'lck, T>,
}

impl<'lck, T: 'lck> LockGuardMut<'lck, T> {
    pub fn lock_type(&self) -> LockResult<LockType> { self.lock_guard.lock_type() }
    pub fn upgrade(&self, to_type: LockType) -> LockResult<()> { self.lock_guard.upgrade(to_type) }
    pub fn try_upgrade(&self, to_type: LockType) -> LockResult<()> { self.lock_guard.try_upgrade(to_type) }
}

impl<'lck, T: 'lck> Deref for LockGuardMut<'lck, T> {
    type Target = T;
    fn deref(&self) -> &<Self as Deref>::Target { self.lock_guard.deref() }
}

impl<'lck, T: 'lck> DerefMut for LockGuardMut<'lck, T> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        unsafe { &mut *self.lock_guard.lock.data_ptr() }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn non_nested_locks() {
        let p = Lock::new_root(0u32).unwrap();

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
            child1: Lock<u32>,
            child2: Lock<u32>,
        };

        let parent_lock = {
            let parent_lb = Lock::<Parent>::new_root_builder();

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
}
