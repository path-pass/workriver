// A refusal with its status, and running a handler's blocking part.
use super::*;

/// An error answered as plain text: a broken rule is the caller's (422), anything else ours (500).
pub(in crate::server) struct ApiError(pub(in crate::server) StatusCode, pub(in crate::server) String);

pub(in crate::server) fn bad(msg: String) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg)
}

/// What the store refused or could not do: a broken rule is the caller's, as when the rules
/// are asked directly; storage that fails is ours, in its own words.
impl From<DataError> for ApiError {
    fn from(e: DataError) -> Self {
        match e {
            // A seat that changed since the user was shown it: look again (409), not a bad request.
            DataError::Rejected(r) => r.into(),
            e => ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }
}

impl From<core::Rejected> for ApiError {
    fn from(r: core::Rejected) -> Self {
        // A seat that changed since the user was shown it, or a start already pending: look
        // again (409), not a bad request.
        if r.0.starts_with(core::SEAT_CHANGED) || r.0.starts_with(core::START_PENDING) {
            return ApiError(StatusCode::CONFLICT, r.0);
        }
        bad(r.0)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

/// Runs git and the store off the async threads.
pub(in crate::server) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, ApiError> + Send + 'static) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
}
