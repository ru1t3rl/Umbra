//! STUB (WI-7): terrain provider trait and `FlatTerrain` (REQ-C-6 partial, ASM-032).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-7: documented when implemented"
)]

use crate::coords::{Enu, Geodetic};
use crate::units::Meters;

pub trait TerrainProvider: Send + Sync {
    fn height_at(&self, at: Geodetic) -> Meters;
    fn surface_normal_at(&self, at: Geodetic) -> Enu;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatTerrain {
    pub height: Meters,
}

impl TerrainProvider for FlatTerrain {
    fn height_at(&self, _at: Geodetic) -> Meters {
        todo!("WI-7")
    }
    fn surface_normal_at(&self, _at: Geodetic) -> Enu {
        todo!("WI-7")
    }
}
