#[inline]
pub fn gaussian(x: f32, sigma: f32) -> f32 {
    let s2 = 2.0 * sigma * sigma;
    (-(x*x) / s2).exp() / (2.5066282746310002 * sigma)
}
