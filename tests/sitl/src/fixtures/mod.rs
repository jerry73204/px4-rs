//! rstest fixtures for SITL E2E tests.

pub mod build;
mod px4_sitl;

pub use px4_sitl::Px4Sitl;
pub use build::{ensure_built, externals_dir, px4_source_dir};
