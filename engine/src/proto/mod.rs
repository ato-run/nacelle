pub mod onescluster {
    pub mod agent {
        pub mod v1 {
            // build.rs で生成される
            include!("onescluster.agent.v1.rs");
        }
    }
    pub mod engine {
        pub mod v1 {
            // build.rs で生成される
            include!("onescluster.engine.v1.rs");
        }
    }
    pub mod coordinator {
        pub mod v1 {
            // build.rs で生成される
            include!("onescluster.coordinator.v1.rs");
        }
    }
}
