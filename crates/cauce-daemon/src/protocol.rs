//! JSON-RPC 2.0 envelope over NDJSON (`docs/contracts.md` §5, §8 and §10).
//!
//! The envelope is built and parsed here so no other module hand-writes a
//! response: a success carries `result`, a failure carries the stable `code`,
//! the human `message` and `data.retryable`, and a notification carries neither
//! `id` nor `result`.

use cauce_core::{ErrorCode, JobError};
use serde::Deserialize;
use serde_json::{Value, json};

/// Maximum size of one line, newline excluded (`docs/contracts.md` §5).
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// The only protocol version v0.1 speaks (`docs/contracts.md` §10).
pub const PROTOCOL_VERSION: u64 = 1;

const JSONRPC: &str = "2.0";

/// A parsed request. `id` may be any JSON scalar, as JSON-RPC 2.0 allows.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// Parses one line. Invalid JSON and a missing `method` are the same class of
/// error for the transport: the daemon answers with this error and closes.
pub fn parse_request(line: &str) -> Result<Request, JobError> {
    serde_json::from_str(line).map_err(|error| {
        invalid(format!(
            "Mensaje JSON-RPC inválido: {error}. Cada mensaje debe ser un objeto con `method`."
        ))
    })
}

/// `{"jsonrpc":"2.0","id":…,"result":…}`
pub fn success(id: Option<&Value>, result: Value) -> Value {
    json!({"jsonrpc": JSONRPC, "id": id.cloned().unwrap_or(Value::Null), "result": result})
}

/// `{"jsonrpc":"2.0","id":…,"error":{"code":…,"message":…,"data":{"retryable":…}}}`
pub fn failure(id: Option<&Value>, error: &JobError) -> Value {
    json!({
        "jsonrpc": JSONRPC,
        "id": id.cloned().unwrap_or(Value::Null),
        "error": {
            "code": error.code,
            "message": error.message,
            "data": {"retryable": error.retryable},
        },
    })
}

/// `{"jsonrpc":"2.0","method":…,"params":…}` — no `id`, so it is a notification.
pub fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": JSONRPC, "method": method, "params": params})
}

/// The hash maps the daemon builds (`docs/contracts.md` §8).
pub fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::InvalidInput, message, false)
}

/// Non-retryable: repeating an unknown method can never succeed.
pub fn internal(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::Internal, message, false)
}

/// Unknown methods use `INTERNAL` with the agreed Spanish message; the code set
/// is closed by `docs/contracts.md` §8, so no new code is invented.
pub fn unknown_method(method: &str) -> JobError {
    internal(format!(
        "método desconocido `{method}`: este daemon no implementa esa operación en v0.1."
    ))
}

/// Any serializable domain value becomes the `result` of a response.
pub fn encode<T: serde::Serialize + ?Sized>(value: &T) -> Result<Value, JobError> {
    serde_json::to_value(value).map_err(json_error)
}

pub fn json_error(error: serde_json::Error) -> JobError {
    invalid(format!("No se pudo serializar el resultado: {error}"))
}

/// Shallow patch: only the keys the client sent replace the current value.
pub fn merge(mut base: Value, patch: &Value) -> Value {
    let Value::Object(patch) = patch else {
        return base;
    };
    if let Value::Object(base) = &mut base {
        for (key, value) in patch {
            base.insert(key.clone(), value.clone());
        }
    }
    base
}

pub fn optional_param(params: Option<&Value>, key: &str) -> Result<Option<Value>, JobError> {
    match params.and_then(|params| params.get(key)) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(value.clone())),
    }
}

pub fn string_param(params: Option<&Value>, key: &str) -> Result<String, JobError> {
    optional_param(params, key)?
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| invalid(format!("Falta el parámetro `{key}` (texto).")))
}

pub fn string_list(params: Option<&Value>, key: &str) -> Result<Vec<String>, JobError> {
    match optional_param(params, key)? {
        None => Ok(Vec::new()),
        Some(value) => serde_json::from_value::<Vec<String>>(value)
            .map_err(|error| invalid(format!("`{key}` debe ser una lista de textos: {error}"))),
    }
}

/// Typed object parameter such as `snapshot`.
pub fn decode<T: serde::de::DeserializeOwned>(
    params: Option<&Value>,
    key: &str,
) -> Result<T, JobError> {
    let value = optional_param(params, key)?
        .ok_or_else(|| invalid(format!("Falta el parámetro `{key}`.")))?;
    serde_json::from_value(value).map_err(|error| invalid(format!("`{key}` no es válido: {error}")))
}
