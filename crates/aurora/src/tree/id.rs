// Generational identifier for retained arena nodes.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub index: u32,
    pub generation: u32,
}

impl NodeId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}
