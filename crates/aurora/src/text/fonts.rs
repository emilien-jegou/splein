// Font identifiers and font data management for cosmic-text.

use std::sync::Arc;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

pub struct FontData {
    pub id: FontId,
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
}

impl FontData {
    pub fn new(id: u32, name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            id: FontId(id),
            name: name.into(),
            bytes: Arc::new(bytes),
        }
    }
}
