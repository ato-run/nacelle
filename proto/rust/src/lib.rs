// Shared Rust proto types for capsuled.
//
// This crate intentionally re-uses the checked-in generated sources in
// `capsuled/engine/src/proto` to avoid requiring protoc/buf in consumers.

#![allow(clippy::large_enum_variant)]

pub mod onescluster {
    pub mod common {
        pub mod v1 {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../engine/src/proto/onescluster.common.v1.rs"
            ));
        }
    }

    pub mod coordinator {
        pub mod v1 {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../engine/src/proto/onescluster.coordinator.v1.rs"
            ));
        }
    }

    pub mod engine {
        pub mod v1 {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../engine/src/proto/onescluster.engine.v1.rs"
            ));
        }
    }
}
