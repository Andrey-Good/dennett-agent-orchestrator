pub mod dennett {
    pub mod common {
        #[allow(clippy::empty_docs)] // Generated prost/tonic code can contain empty proto comments.
        pub mod v1 {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../generated/rust/dennett/common/v1/dennett.common.v1.rs"
            ));
        }
    }

    pub mod sync {
        #[allow(clippy::empty_docs)] // Generated prost/tonic code can contain empty proto comments.
        pub mod v1 {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../generated/rust/dennett/sync/v1/dennett.sync.v1.rs"
            ));
        }
    }

    pub mod control {
        #[allow(clippy::empty_docs)] // Generated prost/tonic code can contain empty proto comments.
        pub mod v1 {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../generated/rust/dennett/control/v1/dennett.control.v1.rs"
            ));
        }
    }
}
