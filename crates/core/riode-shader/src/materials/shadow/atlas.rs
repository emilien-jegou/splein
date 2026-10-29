#[derive(Clone, Debug)]
pub struct ShadowAtlas {
    pub data: Vec<u8>,
    pub w: usize,
    pub h: usize,
}
impl Default for ShadowAtlas {
    fn default() -> Self { Self { data: vec![], w: 0, h: 0 } }
}
