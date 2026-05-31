use {
    chrono::Utc,
    serde::Serialize,
    serde_json::json,
    vercel_runtime::{Body, Response, StatusCode as VercelStatusCode},
};

#[derive(Debug)]
pub enum Error {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadRequest(m) => write!(f, "Bad Request: {m}"),
            Error::Unauthorized(m) => write!(f, "Unauthorized: {m}"),
            Error::NotFound(m) => write!(f, "Not Found: {m}"),
            Error::Conflict(m) => write!(f, "Conflict: {m}"),
            Error::Internal(m) => write!(f, "Internal Error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

const CORS_METHODS: &str = "GET, POST, PATCH, OPTIONS";
const CORS_HEADERS: &str = "Content-Type, Authorization, Date, X-Date";

impl Error {
    pub fn to_vercel_response(&self) -> Response<Body> {
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let (status, message) = match self {
            Error::BadRequest(m) => (VercelStatusCode::BAD_REQUEST, m),
            Error::Unauthorized(m) => (VercelStatusCode::UNAUTHORIZED, m),
            Error::NotFound(m) => (VercelStatusCode::NOT_FOUND, m),
            Error::Conflict(m) => (VercelStatusCode::CONFLICT, m),
            Error::Internal(m) => (VercelStatusCode::INTERNAL_SERVER_ERROR, m),
        };
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", CORS_METHODS)
            .header("Access-Control-Allow-Headers", CORS_HEADERS)
            .header("X-Date", date)
            .body(Body::Text(
                json!({"error": "Error", "message": message}).to_string(),
            ))
            .unwrap()
    }
}

pub fn into_vercel_response<T: Serialize>(result: Result<T, Error>) -> Response<Body> {
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    match result {
        Ok(value) => Response::builder()
            .status(VercelStatusCode::OK)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", CORS_METHODS)
            .header("Access-Control-Allow-Headers", CORS_HEADERS)
            .header("X-Date", date)
            .body(Body::Text(serde_json::to_string(&value).unwrap_or_else(
                |_| json!({"error": "SerializationFailed"}).to_string(),
            )))
            .unwrap(),
        Err(error) => error.to_vercel_response(),
    }
}
