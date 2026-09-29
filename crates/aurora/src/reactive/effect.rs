// Reactive effect observer executing upon dependency changes.

use crate::reactive::id::SubscriberId;
use crate::reactive::observer::Observer;
use crate::reactive::runtime::ReactiveRuntime;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Effect {
    id: SubscriberId,
    runtime: Rc<RefCell<ReactiveRuntime>>,
    is_disposed: bool,
}

impl Effect {
    pub fn new<F: Fn() + 'static>(runtime: Rc<RefCell<ReactiveRuntime>>, action: F) -> Self {
        let id = runtime.borrow_mut().alloc_subscriber();
        let action_rc = Rc::new(action);
        let action_for_sub: Rc<dyn Fn()> = {
            let act = Rc::clone(&action_rc);
            let rt = Rc::clone(&runtime);
            Rc::new(move || {
                rt.borrow_mut().push_observer(Observer::Effect(id));
                act();
                rt.borrow_mut().pop_observer();
            })
        };

        runtime.borrow_mut().effects.insert(id, action_for_sub);
        runtime.borrow_mut().push_observer(Observer::Effect(id));
        action_rc();
        runtime.borrow_mut().pop_observer();

        Self {
            id,
            runtime,
            is_disposed: false,
        }
    }

    pub fn dispose(&mut self) {
        if !self.is_disposed {
            self.runtime.borrow_mut().dispose_effect(self.id);
            self.is_disposed = true;
        }
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        self.dispose();
    }
}
