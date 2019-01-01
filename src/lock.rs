use std::hash::Hash;
use std::ops::{ Deref, DerefMut };
use std::fmt::{ Display, Debug };
use std::sync::Arc;

use self::super::common::*;
use self::super::locktype::*;
use self::super::kernel::*;


pub struct LockBuilder<I: Clone + Eq + Hash + Display + Debug>  {
    kernel: LockKernelRc<I>,
}

impl<I: Clone + Eq + Hash + Display + Debug> LockBuilder<I> {

    pub fn new_root_builder() -> LockBuilder<I> {
        LockBuilder { kernel: LockKernelRc::new(LockKernel::new(None, None)) }
    }

    pub fn new_child_builder(&self, id: I) -> LockResult<I, LockBuilder<I>> {
        self.kernel
            .get_or_create_child(id)
            .map(|child_kernel| LockBuilder { kernel: child_kernel })
    }

    pub fn new_child<T>(&self, id: I, data: T) -> LockResult<I, Lock<I, T>> {
        self.new_child_builder(id).and_then(|cb| cb.build(data))
    }

    pub fn build<T>(self, data: T) -> LockResult<I, Lock<I, T>> {
        self.kernel.own()
            .map(|_| Lock {
                kernel: self.kernel,
                data,
            })
    }

    pub fn id(&self) -> Option<I> { self.kernel.id() }

    pub fn path(&self) -> LockPath<I> { self.kernel.path() }
}


pub struct Lock<I: Clone + Eq + Hash + Display + Debug, T> {
    kernel: LockKernelRc<I>,
    data: T,
}

impl<I: Clone + Eq + Hash + Display + Debug, T> Lock<I, T> {

    pub fn new_root_builder() -> LockBuilder<I> { LockBuilder::new_root_builder() }

    pub fn new_root(data: T) -> LockResult<I, Lock<I, T>> { LockBuilder::new_root_builder().build(data) }

    pub fn new_child_builder(&self, id: I) -> LockResult<I, LockBuilder<I>> {
        self.kernel
            .get_or_create_child(id)
            .map(|child_kernel| LockBuilder { kernel: child_kernel })
    }

    pub fn new_child<T2>(&self, id: I, data: T2) -> LockResult<I, Lock<I, T2>> {
        self.new_child_builder(id).and_then(|cb| cb.build(data))
    }

    pub fn id(&self) -> Option<I> { self.kernel.id() }

    pub fn path(&self) -> LockPath<I> { self.kernel.path() }

    pub fn lock(&self, lock_type: LockType) -> LockResult<I, LockGuard<I, T>> {
        self.do_lock::<()>(lock_type, None, false)
    }

    pub fn try_lock(&self, lock_type: LockType) -> LockResult<I, LockGuard<I, T>> {
        self.do_lock::<()>(lock_type, None, true)
    }

    pub fn lock_using_parent<T2>(&self, lock_type: LockType, parent: &LockGuard<I, T2>) -> LockResult<I, LockGuard<I, T>> {
        self.do_lock(lock_type, Some(parent), false)
    }

    pub fn try_lock_using_parent<T2>(&self, lock_type: LockType, parent: &LockGuard<I, T2>) -> LockResult<I, LockGuard<I, T>> {
        self.do_lock(lock_type, Some(parent), true)
    }

    pub fn lock_exclusive(&self) -> LockResult<I, LockGuardMut<I, T>> {
        self.do_lock_exclusive::<()>(None, false)
    }

    pub fn try_lock_exclusive(&self) -> LockResult<I, LockGuardMut<I, T>> {
        self.do_lock_exclusive::<()>(None, true)
    }

    pub fn lock_exclusive_using_parent<T2>(&self, parent: &LockGuard<I, T2>) -> LockResult<I, LockGuardMut<I, T>> {
        self.do_lock_exclusive(Some(parent), false)
    }

    pub fn try_lock_exclusive_using_parent<T2>(&self, parent: &LockGuard<I, T2>) -> LockResult<I, LockGuardMut<I, T>> {
        self.do_lock_exclusive(Some(parent), true)
    }

    fn do_lock<T2>(&self, lock_type: LockType, parent: Option<&LockGuard<I, T2>>, try_only: bool) -> LockResult<I, LockGuard<I, T>> {
        self.kernel
            .acquire(lock_type, parent.map(|p| p.lock_instance.clone()), true, try_only)
            .map(|lock_instance| LockGuard { lock: self, lock_instance })
    }

    fn do_lock_exclusive<T2>(&self, parent: Option<&LockGuard<I, T2>>, try_only: bool) -> LockResult<I, LockGuardMut<I, T>> {
        self.do_lock(LockType::Exclusive, parent, try_only).map(|lg| LockGuardMut { lock_guard: lg })
    }

    fn data_ptr(&self) -> *mut T {
        (&self.data as *const T) as *mut T
    }
}

impl<I: Clone + Eq + Hash + Display + Debug, T> Drop for Lock<I, T> {
    fn drop(&mut self) {
        self.kernel
            .unown()
            .unwrap();
    }
}


pub struct LockGuard<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> {
    lock: &'lck Lock<I, T>,
    lock_instance: Arc<LockInstance<I>>,
}

impl<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> LockGuard<'lck, I, T> {

    pub fn id(&self) -> Option<I> { self.lock.id() }

    pub fn path(&self) -> LockPath<I> { self.lock.path() }

    pub fn lock_type(&self) -> LockResult<I, LockType> {
        self.lock_instance.lock_type()
    }

    pub fn upgrade(&self, to_type: LockType) -> LockResult<I, ()> {
        self.lock_instance.upgrade(to_type, true, false)
    }

    pub fn try_upgrade(&self, to_type: LockType) -> LockResult<I, ()> {
        self.lock_instance.upgrade(to_type, true, true)
    }
}

impl<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> Deref for LockGuard<'lck, I, T> {
    type Target = T;

    fn deref(&self) -> &<Self as Deref>::Target {
        unsafe { &*self.lock.data_ptr() }
    }
}


pub struct LockGuardMut<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> {
    lock_guard: LockGuard<'lck, I, T>,
}

impl<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> LockGuardMut<'lck, I, T> {
    pub fn id(&self) -> Option<I> { self.lock_guard.id() }
    pub fn path(&self) -> LockPath<I> { self.lock_guard.path() }
    pub fn lock_type(&self) -> LockResult<I, LockType> { self.lock_guard.lock_type() }
    pub fn upgrade(&self, to_type: LockType) -> LockResult<I, ()> { self.lock_guard.upgrade(to_type) }
    pub fn try_upgrade(&self, to_type: LockType) -> LockResult<I, ()> { self.lock_guard.try_upgrade(to_type) }
}

impl<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> Deref for LockGuardMut<'lck, I, T> {
    type Target = T;
    fn deref(&self) -> &<Self as Deref>::Target { self.lock_guard.deref() }
}

impl<'lck, I: Clone + Eq + Hash + Display + Debug + 'lck, T: 'lck> DerefMut for LockGuardMut<'lck, I, T> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        unsafe { &mut *self.lock_guard.lock.data_ptr() }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn id_path() {
        let l = Lock::new_root(0u32).unwrap();

        let l1 = l.new_child("1".to_string(), 0u32).unwrap();

        let l1a = l1.new_child("a".to_string(), 0u32).unwrap();
        let l1b = l1.new_child("b".to_string(), 0u32).unwrap();

        let l2 = l.new_child("2".to_string(), 0u32).unwrap();

        let l2a = l2.new_child("a".to_string(), 0u32).unwrap();
        let l2b = l2.new_child("b".to_string(), 0u32).unwrap();

        assert_eq!(l.id(), None);
        assert_eq!(l.path(), LockPath::new());

        assert_eq!(l1.id(), Some("1".to_string()));
        assert_eq!(l1.path(), LockPath::builder().add("1".to_string()).build());

        assert_eq!(l1a.id(), Some("a".to_string()));
        assert_eq!(l1a.path(), LockPath::builder().add("1".to_string()).add("a".to_string()).build());

        assert_eq!(l1b.id(), Some("b".to_string()));
        assert_eq!(l1b.path(), LockPath::builder().add("1".to_string()).add("b".to_string()).build());

        assert_eq!(l2.id(), Some("2".to_string()));
        assert_eq!(l2.path(), LockPath::builder().add("2".to_string()).build());

        assert_eq!(l2a.id(), Some("a".to_string()));
        assert_eq!(l2a.path(), LockPath::builder().add("2".to_string()).add("a".to_string()).build());

        assert_eq!(l2b.id(), Some("b".to_string()));
        assert_eq!(l2b.path(), LockPath::builder().add("2".to_string()).add("b".to_string()).build());
    }

    #[test]
    fn non_nested_locks() {
        let p = Lock::new_root(0u32).unwrap();

        let c1 = p.new_child("1".to_string(), 0u32).unwrap();
        let c2 = p.new_child("2".to_string(), 0u32).unwrap();

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
            child1: Lock<String, u32>,
            child2: Lock<String, u32>,
        };

        let parent_lock = {
            let parent_lb = Lock::<String, Parent>::new_root_builder();

            let parent = Parent {
                child1: parent_lb.new_child("1".to_string(), 0u32).unwrap(),
                child2: parent_lb.new_child("2".to_string(), 0u32).unwrap(),
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
