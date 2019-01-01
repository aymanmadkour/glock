use std::hash::Hash;
use std::collections::HashMap;
use std::ops::Deref;
use std::fmt::{ Display, Debug };
use std::sync::{ Arc, Weak, Mutex, MutexGuard, Condvar };

use self::super::common::*;
use self::super::locktype::*;

pub struct LockKernel<I: Clone + Eq + Hash + Display + Debug> {
    id: Option<I>,
    parent: Option<LockKernelRc<I>>,
    condvar: Condvar,
    state: Mutex<LockKernelState<I>>,
}

struct LockKernelState<I: Clone + Eq + Hash + Display + Debug> {
    owned: bool,
    counts: [usize; LOCK_TYPE_COUNT],
    children: HashMap<I, Weak<LockKernel<I>>>,
}

impl<I: Clone + Eq + Hash + Display + Debug> LockKernel<I> {

    pub fn new(id: Option<I>, parent: Option<LockKernelRc<I>>) -> LockKernel<I> {
        LockKernel {
            id,
            parent,
            condvar: Condvar::new(),
            state: Mutex::new(LockKernelState {
                owned: false,
                counts: LOCK_EMPTY_COUNTS,
                children: HashMap::new(),
            }),
        }
    }

    fn lock_state<'slf: 'mg, 'mg>(&'slf self) -> LockResult<I, MutexGuard<'mg, LockKernelState<I>>> {
        self.state
            .lock()
            .map_err(map_unknown_err)
    }

    fn dropping(&self, id: &I) {
        self.lock_state()
            .map(|mut state| state.children.remove(id))
            .unwrap();
    }

    pub fn id(&self) -> Option<I> { self.id.clone() }

    pub fn path(&self) -> LockPath<I> {
        let mut path = self.parent
            .as_ref()
            .map(|p| p.path())
            .unwrap_or_else(|| LockPath::new());

        self.id().map(|id| path.add(id.clone()));

        path
    }

    pub fn own(&self) -> LockResult<I, ()> {
        self.lock_state().and_then(|mut state| {
            if state.owned { Err(LockError::LockAlreadyUsed { path: self.path() }) }
            else {
                state.owned = true;
                Ok(())
            }
        })
    }

    pub fn unown(&self) -> LockResult<I, ()> {
        self.lock_state().map(|mut state| { state.owned = false; })
    }
}

impl<I: Clone + Eq + Hash + Display + Debug> Drop for LockKernel<I> {
    fn drop(&mut self) {
        if self.id.is_some() && self.parent.is_some() {
            self.parent.as_ref().unwrap().dropping(self.id.as_ref().unwrap());
        }
    }
}


pub struct LockKernelRc<I: Clone + Eq + Hash + Display + Debug> {
    kernel: Arc<LockKernel<I>>,
}

impl<I: Clone + Eq + Hash + Display + Debug> LockKernelRc<I> {

    pub fn new(kernel: LockKernel<I>) -> LockKernelRc<I> {
        LockKernelRc {
            kernel: Arc::new(kernel),
        }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.kernel, &other.kernel)
    }

    pub fn get_or_create_child(&self, id: I) -> LockResult<I, LockKernelRc<I>> {
        self.kernel
            .lock_state()
            .map(|mut state| {
                state.children
                    .get(&id)
                    .and_then(|child_ptr| child_ptr.upgrade())
                    .map(|kernel| LockKernelRc { kernel })
                    .unwrap_or_else(|| {
                        let kernel = LockKernelRc::new(LockKernel::new(Some(id.clone()), Some(self.clone())));
                        state.children.insert(id, kernel.clone_weak());
                        kernel
                    })
            })
    }

    pub fn clone_weak(&self) -> Weak<LockKernel<I>> {
        Arc::downgrade(&self.kernel)
    }

    pub fn acquire(&self, lock_type: LockType, using_parent: Option<Arc<LockInstance<I>>>, auto_upgrade: bool, try_only: bool) -> LockResult<I, Arc<LockInstance<I>>> {

        let parent_instance = self.ensure_parent_lock(lock_type, using_parent, auto_upgrade, try_only)?;

        self.lock_state()
            .and_then(|mut state| {
                let mut ready = false;

                while !ready {
                    ready = true;

                    for lt in LockType::lock_types().iter() {
                        if state.counts[lt.index()] > 0 && !lock_type.compatible_with(*lt) {
                            ready = false;
                            break;
                        }
                    }

                    if !ready {
                        if try_only { return Err(LockError::LockBusy { path: self.path() }); }
                        else { state = self.condvar.wait(state).map_err(map_unknown_err)?; }
                    }
                }

                state.counts[lock_type.index()] += 1;

                Ok(LockInstance::new(self.clone(), parent_instance, lock_type))
            })
    }

    fn release(&self, lock_type: LockType) -> LockResult<I, ()> {
        self.lock_state()
            .map(|mut state| {
                state.counts[lock_type.index()] -= 1;
                self.condvar.notify_all();
            })
    }

    fn upgrade(&self, from_type: LockType, to_type: LockType, using_parent: Option<Arc<LockInstance<I>>>, auto_upgrade: bool, try_only: bool) -> LockResult<I, ()> {

        if from_type == to_type { return Ok(()); }

        if !from_type.upgradable_to(to_type) {
            return Err(LockError::InvalidUpgrade { original: from_type, requested: to_type });
        }

        self.ensure_parent_lock(to_type, using_parent, auto_upgrade, try_only)?;

        self.lock_state()
            .and_then(|mut state| {
                let mut ready = false;

                while !ready {
                    ready = true;

                    for lt in LockType::lock_types().iter() {
                        let max_count = if *lt == from_type { 1 } else { 0 };

                        if state.counts[lt.index()] > max_count && !to_type.compatible_with(*lt) {
                            ready = false;
                            break;
                        }
                    }

                    if !ready {
                        if try_only { return Err(LockError::LockBusy { path: self.path() }); }
                        else { state = self.condvar.wait(state).map_err(map_unknown_err)?; }
                    }
                }

                state.counts[from_type.index()] -= 1;
                state.counts[to_type.index()] += 1;

                Ok(())
            })
    }

    fn ensure_parent_lock(&self, lock_type: LockType, using_parent: Option<Arc<LockInstance<I>>>, auto_upgrade: bool, try_only: bool) -> LockResult<I, Option<Arc<LockInstance<I>>>> {
        match self.parent.as_ref() {
            Some(parent) => {
                match using_parent {
                    Some(p) => {
                        if !parent.ptr_eq(&p.kernel) { return Err(LockError::InvalidParentLock { expected_path: parent.path(), actual_path: p.kernel.path() }); }

                        let required_parent_lock_type = lock_type.implicit_parent_type();
                        let actual_parent_lock_type = p.lock_state()?.lock_type;

                        if required_parent_lock_type.index() > actual_parent_lock_type.index() {
                            if auto_upgrade {
                                let upgrade_type = actual_parent_lock_type.min_upgradable(required_parent_lock_type);
                                p.upgrade(upgrade_type, auto_upgrade, try_only)?;
                            } else {
                                return Err(LockError::InvalidParentLockType { path: p.kernel.path(), required: required_parent_lock_type, actual: actual_parent_lock_type });
                            }

                        } else if required_parent_lock_type.index() < actual_parent_lock_type.index() {
                            if !required_parent_lock_type.upgradable_to(actual_parent_lock_type) {
                                if auto_upgrade {
                                    let upgrade_type = required_parent_lock_type.min_upgradable(actual_parent_lock_type);
                                    p.upgrade(upgrade_type, auto_upgrade, try_only)?;
                                } else {
                                    return Err(LockError::InvalidParentLockType { path: p.kernel.path(), required: required_parent_lock_type, actual: actual_parent_lock_type });
                                }
                            }
                        }

                        Ok(Some(p))
                    },

                    None => {
                        Ok(Some(parent.acquire(lock_type.implicit_parent_type(), None, auto_upgrade, try_only)?))
                    },
                }
            },

            None => { Ok(None) },
        }
    }
}

impl<I: Clone + Eq + Hash + Display + Debug> Deref for LockKernelRc<I> {
    type Target = LockKernel<I>;
    fn deref(&self) -> &<Self as Deref>::Target { self.kernel.deref() }
}

impl<I: Clone + Eq + Hash + Display + Debug> Clone for LockKernelRc<I> {
    fn clone(&self) -> Self {
        LockKernelRc { kernel: self.kernel.clone() }
    }
}


pub struct LockInstance<I: Clone + Eq + Hash + Display + Debug> {
    kernel: LockKernelRc<I>,
    parent: Option<Arc<LockInstance<I>>>,
    state: Mutex<LockInstanceState>,
}

struct LockInstanceState {
    lock_type: LockType,
}

impl<I: Clone + Eq + Hash + Display + Debug> LockInstance<I> {

    fn new(kernel: LockKernelRc<I>, parent: Option<Arc<LockInstance<I>>>, lock_type: LockType) -> Arc<LockInstance<I>> {

        Arc::new(LockInstance {
            kernel,
            parent,
            state: Mutex::new(LockInstanceState { lock_type, }),
        })
    }

    fn lock_state<'slf: 'mg, 'mg>(&'slf self) -> LockResult<I, MutexGuard<'mg, LockInstanceState>> {
        self.state
            .lock()
            .map_err(map_unknown_err)
    }

    pub fn lock_type(&self) -> LockResult<I, LockType> {
        self.lock_state().map(|state| state.lock_type)
    }

    pub fn upgrade(&self, to_type: LockType, auto_upgrade: bool, try_only: bool) -> LockResult<I, ()> {
        self.lock_state()
            .and_then(|mut state| {
                self.kernel.upgrade(state.lock_type, to_type, self.parent.clone(), auto_upgrade, try_only)?;
                state.lock_type = to_type;
                Ok(())
            })
    }
}

impl<I: Clone + Eq + Hash + Display + Debug> Drop for LockInstance<I> {
    fn drop(&mut self) {
        self.lock_state()
            .and_then(|state| self.kernel.release(state.lock_type))
            .unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn id_path() {
        let k = LockKernelRc::new(LockKernel::new(None, None));

        let k1 = k.get_or_create_child("1".to_string()).unwrap();

        let k1a = k1.get_or_create_child("a".to_string()).unwrap();
        let k1b = k1.get_or_create_child("b".to_string()).unwrap();

        let k2 = k.get_or_create_child("2".to_string()).unwrap();

        let k2a = k2.get_or_create_child("a".to_string()).unwrap();
        let k2b = k2.get_or_create_child("b".to_string()).unwrap();

        assert_eq!(k.id(), None);
        assert_eq!(k.path(), LockPath::new());

        assert_eq!(k1.id(), Some("1".to_string()));
        assert_eq!(k1.path(), LockPath::builder().add("1".to_string()).build());

        assert_eq!(k1a.id(), Some("a".to_string()));
        assert_eq!(k1a.path(), LockPath::builder().add("1".to_string()).add("a".to_string()).build());

        assert_eq!(k1b.id(), Some("b".to_string()));
        assert_eq!(k1b.path(), LockPath::builder().add("1".to_string()).add("b".to_string()).build());

        assert_eq!(k2.id(), Some("2".to_string()));
        assert_eq!(k2.path(), LockPath::builder().add("2".to_string()).build());

        assert_eq!(k2a.id(), Some("a".to_string()));
        assert_eq!(k2a.path(), LockPath::builder().add("2".to_string()).add("a".to_string()).build());

        assert_eq!(k2b.id(), Some("b".to_string()));
        assert_eq!(k2b.path(), LockPath::builder().add("2".to_string()).add("b".to_string()).build());
    }

    #[test]
    fn own_unown() {
        let kernel = LockKernel::<String>::new(None, None);

        assert_eq!(kernel.own(), Ok(()));
        assert_eq!(kernel.own(), Err(LockError::LockAlreadyUsed { path: kernel.path() }));

        assert_eq!(kernel.unown(), Ok(()));

        assert_eq!(kernel.own(), Ok(()));
        assert_eq!(kernel.own(), Err(LockError::LockAlreadyUsed { path: kernel.path() }));

        assert_eq!(kernel.unown(), Ok(()));
        assert_eq!(kernel.unown(), Ok(()));
    }

    #[test]
    fn ptr_eq() {
        let k1 = LockKernelRc::new(LockKernel::new(None, None));
        let k2 = LockKernelRc::new(LockKernel::new(None, None));

        assert_eq!(k1.ptr_eq(&k2), false);
        assert_eq!(k1.ptr_eq(&k1), true);

        let k1a = k1.get_or_create_child("a".to_string()).unwrap();
        let k1a2 = k1.get_or_create_child("a".to_string()).unwrap();

        assert_eq!(k1a.ptr_eq(&k1a2), true);
    }

    #[test]
    fn clone_clone_weak() {
        let k = LockKernelRc::new(LockKernel::new(None, None));

        assert_eq!(Arc::strong_count(&k.kernel), 1);
        assert_eq!(Arc::weak_count(&k.kernel), 0);

        let _k2 = k.clone();

        assert_eq!(Arc::strong_count(&k.kernel), 2);
        assert_eq!(Arc::weak_count(&k.kernel), 0);

        let _k3 = k.clone_weak();

        assert_eq!(Arc::strong_count(&k.kernel), 2);
        assert_eq!(Arc::weak_count(&k.kernel), 1);

        {
            let k_child = k.get_or_create_child("1".to_string()).unwrap();

            assert_eq!(Arc::strong_count(&k.kernel), 3);
            assert_eq!(Arc::weak_count(&k.kernel), 1);

            assert_eq!(Arc::strong_count(&k_child.kernel), 1);
            assert_eq!(Arc::weak_count(&k_child.kernel), 1);
        }

        assert_eq!(Arc::strong_count(&k.kernel), 2);
        assert_eq!(Arc::weak_count(&k.kernel), 1);
    }

    #[test]
    fn acquire_release() {
        for t1 in LockType::lock_types().iter() {
            for t2 in LockType::lock_types().iter() {
                let should_succeed = t1.compatible_with(*t2);
                let k = LockKernelRc::<String>::new(LockKernel::new(None, None));

                {
                    let _t1_lock = k.acquire(*t1, None, true, true).unwrap();
                    assert_eq!(k.acquire(*t2, None, true, true).is_ok(), should_succeed);
                }

                if !should_succeed {
                    assert_eq!(k.acquire(*t2, None, true, true).is_ok(), true);
                }
            }
        }
    }

    #[test]
    fn acquire_release_implicit_parent() {
        for t1 in LockType::lock_types().iter() {
            for t2 in LockType::lock_types().iter() {
                let should_succeed = t1.implicit_parent_type().compatible_with(*t2);
                let k = LockKernelRc::new(LockKernel::new(None, None));
                let k1 = k.get_or_create_child("1".to_string()).unwrap();

                {
                    let _t1_lock = k1.acquire(*t1, None, true, true).unwrap();
                    assert_eq!(k.acquire(*t2, None, true, true).is_ok(), should_succeed);
                }

                if !should_succeed {
                    assert_eq!(k.acquire(*t2, None, true, true).is_ok(), true);
                }
            }
        }
    }

    #[test]
    fn acquire_release_shared_parent() {
        for parent_type in LockType::lock_types().iter() {
            for t1a in LockType::lock_types().iter() {
                for t1b in LockType::lock_types().iter() {
                    for t2 in LockType::lock_types().iter() {
                        let k = LockKernelRc::new(LockKernel::new(None, None));
                        let k1 = k.get_or_create_child("1".to_string()).unwrap();
                        let k2 = k.get_or_create_child("2".to_string()).unwrap();

                        let p_lock = k.acquire(*parent_type, None, true, true).unwrap();
                        let _l1a = k1.acquire(*t1a, Some(p_lock.clone()), true, true).unwrap();
                        assert_eq!(k1.acquire(*t1b, Some(p_lock.clone()), true, true).is_ok(), t1a.compatible_with(*t1b));
                        assert_eq!(k2.acquire(*t2, Some(p_lock.clone()), true, true).is_ok(), true);
                    }
                }
            }
        }
    }

    #[test]
    fn upgrade() {
        for initial_type in LockType::lock_types().iter() {
            for upgrade_type in LockType::lock_types().iter() {
                let should_upgrade_succeed = initial_type.upgradable_to(*upgrade_type);
                let k = LockKernelRc::new(LockKernel::<String>::new(None, None));

                let l1 = k.acquire(*initial_type, None, true, true).unwrap();

                for other_type in LockType::lock_types().iter() {
                    assert_eq!(k.acquire(*other_type, None, true, true).is_ok(), initial_type.compatible_with(*other_type));
                }

                match l1.upgrade(*upgrade_type, true, true) {
                    Ok(()) => {
                        assert_eq!(should_upgrade_succeed, true);

                        for other_type in LockType::lock_types().iter() {
                            assert_eq!(k.acquire(*other_type, None, true, true).is_ok(), upgrade_type.compatible_with(*other_type));
                        }
                    },

                    Err(_) => {
                        assert_eq!(should_upgrade_succeed, false);
                    }
                }
            }
        }
    }

    #[test]
    fn upgrade_implicit_parent() {
        for initial_type in LockType::lock_types().iter() {
            for upgrade_type in LockType::lock_types().iter() {
                let should_upgrade_succeed = initial_type.upgradable_to(*upgrade_type);
                let k = LockKernelRc::new(LockKernel::<String>::new(None, None));
                let k1 = k.get_or_create_child("1".to_string()).unwrap();

                let l1 = k1.acquire(*initial_type, None, true, true).unwrap();

                for other_type in LockType::lock_types().iter() {
                    assert_eq!(k.acquire(*other_type, None, true, true).is_ok(), initial_type.implicit_parent_type().compatible_with(*other_type));
                }

                match l1.upgrade(*upgrade_type, true, true) {
                    Ok(()) => {
                        assert_eq!(should_upgrade_succeed, true);

                        for other_type in LockType::lock_types().iter() {
                            assert_eq!(k.acquire(*other_type, None, true, true).is_ok(), upgrade_type.implicit_parent_type().compatible_with(*other_type));
                        }
                    },

                    Err(_) => {
                        assert_eq!(should_upgrade_succeed, false);
                    }
                }
            }
        }
    }
}
