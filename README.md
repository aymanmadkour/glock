# glock
Granular locking crate for Rust. Instead of using coarse-grained `Mutex` or `RwLock` which can be used to lock an entire structure, glock provides more granular locking.

`GLock` can be either nested or non-nested. The same locking rules apply in both cases, but the way they are constructed is different in each case.

## Nested GLocks
When `GLock`s are nested (i.e. child `GLock`s are protected inside parent `GLock`), a `GLockBuilder` must be used to construct the parent `GLock`.

### Example
```
extern crate glock;

use glock::*;

struct Parent {
    a: GLock<Child>,
    b: GLock<Child>,
}

struct Child {
    x: GLock<i32>,
    y: GLock<i32>,
}

fn main() {
    // Construct parent GLocks and all of its nested GLocks
    let parent_lock = {
        // Prepare parent builder
        let parent_builder = GLock::<Parent>::new_root_builder();

        // Build child GLock protecting first Child struct
        let child1_lock = {
            let child1_builder = parent_builder.new_child_builder().unwrap();

            let child1 = Child {
                x: child1_builder.new_child(0i32).unwrap(),
                y: child1_builder.new_child(0i32).unwrap(),
            };

            child1_builder.build(child1).unwrap()
        };

        // Build child GLock protecting second Child struct
        let child2_lock = {
            let child2_builder = parent_builder.new_child_builder().unwrap();

            let child2 = Child {
                x: child2_builder.new_child(0i32).unwrap(),
                y: child2_builder.new_child(0i32).unwrap(),
            };

            child2_builder.build(child2).unwrap()
        };

        // Create parent struct
        let parent = Parent {
            a: child1_lock,
            b: child2_lock,
        };

        // Build parent GLock protecting parent struct
        parent_builder.build(parent).unwrap()
    };

    // Lock parent
    let p_guard = parent_lock.lock(LockType::IntentionExclusive).unwrap();

    // Lock first child
    let a_guard = p_guard.a.lock_using_parent(LockType::IntentionExclusive, &p_guard).unwrap();

    // Lock and modify field x inside first child
    let mut a_x_guard = a_guard.x.lock_exclusive_using_parent(&a_guard).unwrap();
    *a_x_guard = 10;

    // Lock and modify field y inside first child
    let mut a_y_guard = a_guard.y.lock_exclusive_using_parent(&a_guard).unwrap();
    *a_y_guard = 20;

    // This one will fail with LockError::LockBusy
    let p_guard_2 = parent_lock.try_lock(LockType::Shared).unwrap();
}
```

# Non-Nested GLocks
When `GLock`s are not nested, you can access child locks directly. If you try to acquire a child `GLock` without locking its parent first, an implicit lock will be acquired on its parent and released when the child `GLockGuard` is dropped.

## Example

```
extern crate glock;

use glock::*;

fn main() {
    // Create parent lock
    let parent_lock = GLock::new_root(0u32).unwrap();

    // Create child locks
    let child1_lock = parent_lock.new_child(0u32).unwrap();
    let child2_lock = parent_lock.new_child(0u32).unwrap();

    // An implicit IntentionExclusive lock is acquired on parent_lock,
    // Because child1_lock is a child of parent_lock.
    let mut child1_guard = child1_lock.lock_exclusive().unwrap();
    *child1_guard = 10;

    // This will fail with LockError::LockBusy because an IntentionExclusive lock is held
    // on parent_lock
    let parent_guard2 = parent_lock.try_lock(LockType::Shared).unwrap();
}
```
