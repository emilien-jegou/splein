// Single responsibility: Uniform read-and-register surface shared by signals and derivations.

use std::cell::RefCell;
use std::rc::Rc;

use crate::reactive::computed::Computed;
use crate::reactive::runtime::ReactiveRuntime;
use crate::reactive::signal::Signal;
use crate::reactive::ticker::Ticker;

/// Read-only view of a reactive value.
///
/// Implemented by both [`Signal`] and [`Computed`], so a component that only needs to read
/// a value can accept `impl SignalRead<T>` and the compiler guarantees the child cannot
/// mutate parent state. This is the least-privilege prop type for child components.
pub trait SignalRead<T: Clone + 'static> {
    /// Reads the current value, subscribing the active observer.
    fn get(&self) -> T;
}

impl<T: Clone + 'static> SignalRead<T> for Signal<T> {
    fn get(&self) -> T {
        Signal::get(self)
    }
}

impl<T: Clone + 'static> SignalRead<T> for Computed<T> {
    fn get(&self) -> T {
        Computed::get(self)
    }
}

/// A value source an adapter can read and attach frame-driven work to.
///
/// Both [`Signal`] and [`Computed`] implement this, so a caller can animate a value whether
/// it was declared directly or derived from other signals, with no adapter difference.
pub trait Readable<T: Clone + 'static> {
    /// Reads the current value.
    fn read(&self) -> T;

    /// The runtime owning this value.
    fn owner(&self) -> Rc<RefCell<ReactiveRuntime>>;

    /// Registers a frame-driven ticker on the owning runtime.
    fn attach(&self, ticker: Box<dyn Ticker>);

    /// Takes any motion hand-off attached to this source's most recent write.
    ///
    /// Defaults to `None`, so a source that cannot be written directly simply never carries
    /// one. Only a [`Signal`] can, because only a signal is something a gesture writes to.
    fn take_motion_handoff(&self) -> Option<f32> {
        None
    }
}

impl<T: Clone + 'static> Readable<T> for Signal<T> {
    fn read(&self) -> T {
        self.get()
    }

    fn owner(&self) -> Rc<RefCell<ReactiveRuntime>> {
        self.runtime()
    }

    fn attach(&self, ticker: Box<dyn Ticker>) {
        self.register_ticker(ticker);
    }

    fn take_motion_handoff(&self) -> Option<f32> {
        self.runtime().borrow_mut().take_motion_handoff(self.id())
    }
}

impl<T: Clone + 'static> Readable<T> for Computed<T> {
    fn read(&self) -> T {
        self.get()
    }

    fn owner(&self) -> Rc<RefCell<ReactiveRuntime>> {
        self.runtime_handle()
    }

    fn attach(&self, ticker: Box<dyn Ticker>) {
        self.runtime_handle().borrow_mut().register_ticker(ticker);
    }
}
