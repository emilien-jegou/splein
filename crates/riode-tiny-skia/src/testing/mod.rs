// Single responsibility: Diagnostics for driving, freezing, inspecting, and photographing the engine.

pub mod damage;
pub mod filmstrip;
pub mod harness;
pub mod overflow;
pub mod snapshot;

pub use damage::{parity as damage_parity, DamageParity};
pub use filmstrip::{Filmstrip, Strip};
pub use harness::{Harness, Located, MotionTrace};
pub use overflow::{assert_disjoint, assert_no_viewport_overflow, report, Escape};
pub use snapshot::{Rgba, Snapshot};
