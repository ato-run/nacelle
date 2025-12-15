// Re-export shared proto types from the onescluster-capsuled-proto crate so
// existing `crate::proto::onescluster::*` paths remain valid without local
// regeneration.
pub use onescluster_capsuled_proto::onescluster;
