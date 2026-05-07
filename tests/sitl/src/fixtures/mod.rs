//! rstest fixtures for SITL E2E tests.

pub mod build;
mod px4_sitl;

pub use build::{ensure_built, externals_dir, px4_source_dir};
pub use px4_sitl::Px4Sitl;
