//! API errors in Grafana's JSON shapes (see upstream pkg/api/response/response.go and pkg/apimachinery/errutil).
use crate::store::StoreError;
use http::Response;
use json::Json;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub body: Json,
}

impl ApiError {
    /// `response.Error(status, message, nil)`: `{"message": ...}`.
    pub fn new(status: u16, message: &str) -> ApiError {
        ApiError { status, body: Json::obj().with("message", message) }
    }
    /// `response.Error(status, message, err)` with a non-nil err: Grafana adds `traceID` (empty here), keys sorted.
    pub fn with_trace(status: u16, message: &str) -> ApiError {
        ApiError { status, body: Json::obj().with("message", message).with("traceID", "") }
    }
    /// An errutil error as `response.Err` writes it: `{"message","messageId","statusCode","traceID"}` (Go sorts
    /// the keys when it adds traceID). `message` is the public message, or the status text when empty.
    pub fn errutil(status: u16, message_id: &str, message: &str) -> ApiError {
        let msg = if message.is_empty() { http::reason(status) } else { message };
        ApiError { status, body: Json::obj().with("message", msg).with("messageId", message_id).with("statusCode", status as i64).with("traceID", "") }
    }
    /// Any JSON body (for example a Kubernetes `Status` object).
    pub fn raw(status: u16, body: Json) -> ApiError {
        ApiError { status, body }
    }
    /// 404 `{"message":"Not found"}` (Grafana's NotFoundHandler for /api paths).
    pub fn not_found() -> ApiError {
        ApiError::new(404, "Not found")
    }
    /// 401 `{"message":"Unauthorized"}`.
    pub fn unauthorized() -> ApiError {
        ApiError::new(401, "Unauthorized")
    }
    /// 403 `{"message":"Permission denied"}` (middleware accessForbidden).
    pub fn forbidden() -> ApiError {
        ApiError::new(403, "Permission denied")
    }
    pub fn bad_request(message: &str) -> ApiError {
        ApiError::new(400, message)
    }
    /// 500 with Grafana's default message.
    pub fn internal(message: &str) -> ApiError {
        ApiError::with_trace(500, if message.is_empty() { "Internal Server Error" } else { message })
    }
    pub fn to_response(&self) -> Response {
        Response::json(self.status, &self.body)
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> ApiError {
        match e {
            StoreError::NotFound => ApiError::not_found(),
            StoreError::Conflict(m) => ApiError::new(409, &m),
            StoreError::Other(_) => ApiError::internal(""),
        }
    }
}

impl From<json::JsonError> for ApiError {
    fn from(_: json::JsonError) -> ApiError {
        ApiError::new(400, "bad request data")
    }
}
