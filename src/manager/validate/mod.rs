// Validate module — user_id extraction from gRPC metadata.
use tonic::metadata::MetadataMap;

/// Extracts the user_id from gRPC metadata `x-user-id` header.
#[allow(clippy::result_large_err)]
pub fn user_id_from_metadata(meta: &MetadataMap) -> Result<String, tonic::Status> {
    meta.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| tonic::Status::unauthenticated("missing x-user-id header"))
}

/// Validates that a string field is not empty.
#[allow(clippy::result_large_err)]
pub fn non_empty(v: &str, field: &str) -> Result<(), tonic::Status> {
    if v.is_empty() {
        Err(tonic::Status::invalid_argument(format!("{} must not be empty", field)))
    } else {
        Ok(())
    }
}