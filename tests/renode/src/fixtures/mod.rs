//! Fixtures for the Renode-driven e2e suite.

mod px4_renode;
pub use px4_renode::{
    ProbeOutcome, Px4RenodeSitl, probe_platform, renode_available, renode_binary_available,
};
