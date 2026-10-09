// Single responsibility: Global lighting direction for elevation-based shadows.

/// Lighting direction shared by elevation-based shadows.
///
/// Both angles are needed to know where a shadow falls: azimuth gives the
/// compass direction, altitude gives how far the shadow stretches (overhead
/// light casts almost no offset, horizon light casts a long one).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LightEnvironment {
    /// Horizontal angle in degrees: 0 = light from the right, 90 = from below.
    pub azimuth: f32,
    /// Vertical angle in degrees: 90 = directly overhead, 0 = horizon.
    pub altitude: f32,
    /// 0..1 multiplier on shadow opacity.
    pub intensity: f32,
}

impl LightEnvironment {
    pub const DEFAULT: Self = Self {
        azimuth: 135.0,
        altitude: 45.0,
        intensity: 1.0,
    };

    pub const fn from_angles(azimuth: f32, altitude: f32) -> Self {
        Self {
            azimuth,
            altitude,
            intensity: 1.0,
        }
    }

    /// Shadow offset for an element `elevation` px above the surface.
    pub fn offset_for(&self, elevation: f32) -> (f32, f32) {
        let horizontal = elevation * self.altitude.to_radians().cos();
        let az = self.azimuth.to_radians();
        (horizontal * az.cos(), horizontal * az.sin())
    }
}

impl Default for LightEnvironment {
    fn default() -> Self {
        Self::DEFAULT
    }
}
