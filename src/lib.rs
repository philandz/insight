pub mod pb {
    pub mod service {
        pub mod insight {
            tonic::include_proto!("service.insight");
        }
        pub mod entry {
            tonic::include_proto!("service.entry");
        }
        pub mod budget {
            tonic::include_proto!("service.budget");
        }
    }
    pub mod common {
        pub mod base {
            tonic::include_proto!("common.base");
        }
    }
}

pub mod converters;
pub mod handler;
pub mod manager;