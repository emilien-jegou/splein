// Single responsibility: Uniform read-and-register surface shared by signals and derivations.

use std::cell::RefCell;
use std::rc::Rc;

use crate::reactive::derived::Derived;
use crate::reactive::runtime::ReactiveRuntime;
use crate::reactive::signal::Signal;
use crate::reactive::ticker::Ticker;

/// A value source an adapter can read and attach frame-driven work to.
///
/// Both [`Signal`] and [`Derived`] implement this, so a caller can animate a value whether
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

impl<T: Clone + 'static> Readable<T> for Derived<T> {
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
