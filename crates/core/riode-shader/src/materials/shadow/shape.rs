#[derive(Clone, Debug)]
pub struct ShadowShape {
    pub w: f32,
    pub h: f32,
    pub r: [f32; 4],
}

#[inline]
pub fn round_rect_sdf(px: f32, py: f32, w: f32, h: f32, r: [f32; 4]) -> f32 {
    let cx = px - w*0.5;
    let cy = py - h*0.5;
    let rad = if cx>=0.0 && cy>=0.0 { r[2] } else if cx<0.0 && cy>=0.0 { r[3] } else if cx<0.0 && cy<0.0 { r[0] } else { r[1] };
    let qx = (cx.abs() - w*0.5 + rad).max(0.0);
    let qy = (cy.abs() - h*0.5 + rad).max(0.0);
    ((qx*qx+qy*qy).sqrt() - rad).min((cx.abs()-w*0.5).max(cy.abs()-h*0.5)) + (qx.max(qy)).min(0.0)
}
