//! Typed, validated wire records for the Bingux search v1 protocols.
//!
//! The socket and provider transports are newline-delimited UTF-8 JSON. Callers pass a
//! single line without its delimiter to the parsing functions and write the complete line
//! returned by the encoding functions.

use serde::Serialize;
use serde_json::{Map, Value};
use std::fmt;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_QUERY_BYTES: usize = 512;
pub const MIN_QUERY_LIMIT: u8 = 1;
pub const MAX_QUERY_LIMIT: u8 = 50;
pub const HOST_ID: &str = "bingux-searchd";
pub const PROVIDER_MANIFEST_KIND: &str = "bingux.search-provider";

pub type ProtocolResult<T> = Result<T, ProtocolError>;

/// A deliberately non-diagnostic failure safe to expose at a trust boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolError {
    kind: ProtocolErrorKind,
}

impl ProtocolError {
    pub const fn kind(self) -> ProtocolErrorKind {
        self.kind
    }

    const fn new(kind: ProtocolErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            ProtocolErrorKind::RecordTooLarge => "protocol record exceeds the size limit",
            ProtocolErrorKind::MalformedJson => "protocol record is not valid JSON",
            ProtocolErrorKind::ExpectedObject => "protocol record must be a JSON object",
            ProtocolErrorKind::MissingField => "protocol record has a missing required field",
            ProtocolErrorKind::InvalidField => "protocol record has an invalid field",
            ProtocolErrorKind::UnsupportedProtocol => "protocol version is unsupported",
            ProtocolErrorKind::UnknownRecordType => "protocol record type is unknown",
            ProtocolErrorKind::InvalidIdentifier => "protocol record has an invalid identifier",
            ProtocolErrorKind::InvalidQuery => "protocol record has an invalid query",
            ProtocolErrorKind::InvalidLimit => "protocol record has an invalid limit",
            ProtocolErrorKind::InvalidResult => "protocol record has an invalid result",
            ProtocolErrorKind::InvalidManifest => "provider manifest is invalid",
        })
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    RecordTooLarge,
    MalformedJson,
    ExpectedObject,
    MissingField,
    InvalidField,
    UnsupportedProtocol,
    UnknownRecordType,
    InvalidIdentifier,
    InvalidQuery,
    InvalidLimit,
    InvalidResult,
    InvalidManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultKind {
    Application,
    File,
    Folder,
    Database,
    Calculation,
    Weather,
    Chat,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStartup {
    Eager,
    Lazy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationState {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DaemonErrorCode {
    InvalidRequest,
    UnsupportedProtocol,
    Unavailable,
    ProviderFailed,
    UnknownResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderErrorCode {
    InvalidRequest,
    Unavailable,
    ProviderFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRequest {
    pub request_id: String,
    pub query: String,
    pub limit: u8,
}

impl QueryRequest {
    pub fn validate(&self) -> ProtocolResult<()> {
        validate_request_id(&self.request_id)?;
        validate_query(&self.query)?;
        validate_limit(self.limit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateRequest {
    pub request_id: String,
    /// This value is opaque to the shell and protocol boundary.
    pub result_id: String,
}

impl ActivateRequest {
    pub fn validate(&self) -> ProtocolResult<()> {
        validate_request_id(&self.request_id)?;
        validate_opaque_result_id(&self.result_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellRequest {
    Query(QueryRequest),
    Activate(ActivateRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderResult {
    pub result_id: String,
    pub kind: ResultKind,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub score: f64,
}

impl ProviderResult {
    pub fn validate(&self) -> ProtocolResult<()> {
        validate_provider_result_id(&self.result_id)?;
        validate_score(self.score)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonResult {
    /// A short-lived opaque identifier resolved only by the daemon.
    pub result_id: String,
    pub provider_id: String,
    pub kind: ResultKind,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub score: f64,
}

impl DaemonResult {
    pub fn validate(&self) -> ProtocolResult<()> {
        validate_opaque_result_id(&self.result_id)?;
        validate_provider_id(&self.provider_id)?;
        validate_score(self.score)
    }
}

/// A trusted profile manifest. Command is an argument vector, never a shell string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderManifest {
    pub id: String,
    pub display_name: String,
    pub command: Vec<String>,
    pub startup: ProviderStartup,
    pub priority: u16,
    pub timeout_ms: u16,
}

impl ProviderManifest {
    pub fn validate(&self) -> ProtocolResult<()> {
        validate_provider_id(&self.id)
            .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?;
        if self.command.is_empty()
            || self.priority > 1_000
            || self.timeout_ms == 0
            || self.timeout_ms > 10_000
        {
            return Err(ProtocolError::new(ProtocolErrorKind::InvalidManifest));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderResponse {
    Hello,
    Results {
        query_id: String,
        complete: bool,
        results: Vec<ProviderResult>,
    },
    Activated {
        activation_id: String,
    },
    Error(ProviderError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub correlation: ProviderErrorCorrelation,
    pub code: ProviderErrorCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderErrorCorrelation {
    Query { query_id: String },
    Activation { activation_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DaemonEvent {
    ShowSearch {
        monotonic_usec: String,
    },
    IntegrationState {
        state: IntegrationState,
    },
    Results {
        request_id: String,
        complete: bool,
        elapsed_usec: u64,
        results: Vec<DaemonResult>,
    },
    Activated {
        request_id: String,
    },
    Error {
        request_id: String,
        code: DaemonErrorCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRequest {
    Hello,
    Query {
        query_id: String,
        query: String,
        limit: u8,
    },
    Activate {
        activation_id: String,
        result_id: String,
    },
}

/// Parses one shell request record after its newline delimiter has been removed.
pub fn parse_shell_request(record: &[u8]) -> ProtocolResult<ShellRequest> {
    let value = parse_record(record)?;
    let object = as_object(&value)?;
    validate_protocol_version(object)?;

    match required_string(object, "type")? {
        "query" => {
            let request = QueryRequest {
                request_id: required_string(object, "requestId")?.to_owned(),
                query: required_string(object, "query")?.to_owned(),
                limit: required_limit(object, "limit")?,
            };
            request.validate()?;
            Ok(ShellRequest::Query(request))
        }
        "activate" => {
            let request = ActivateRequest {
                request_id: required_string(object, "requestId")?.to_owned(),
                result_id: required_string(object, "resultId")?.to_owned(),
            };
            request.validate()?;
            Ok(ShellRequest::Activate(request))
        }
        _ => Err(ProtocolError::new(ProtocolErrorKind::UnknownRecordType)),
    }
}

/// Parses and validates one provider manifest JSON document.
pub fn parse_provider_manifest(record: &[u8]) -> ProtocolResult<ProviderManifest> {
    let value = parse_record(record)?;
    let object = as_object(&value)?;
    validate_protocol_version(object).map_err(|error| match error.kind() {
        ProtocolErrorKind::UnsupportedProtocol => error,
        _ => ProtocolError::new(ProtocolErrorKind::InvalidManifest),
    })?;

    if required_string(object, "kind")
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?
        != PROVIDER_MANIFEST_KIND
    {
        return Err(ProtocolError::new(ProtocolErrorKind::InvalidManifest));
    }

    let command = required_array(object, "command")
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?
        .iter()
        .map(|argument| {
            argument
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidManifest))
        })
        .collect::<ProtocolResult<Vec<_>>>()?;

    let manifest = ProviderManifest {
        id: required_string(object, "id")
            .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?
            .to_owned(),
        display_name: required_string(object, "displayName")
            .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?
            .to_owned(),
        command,
        startup: parse_startup(
            required_string(object, "startup")
                .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?,
        )
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?,
        priority: required_u16_in_range(object, "priority", 0, 1_000)
            .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?,
        timeout_ms: required_u16_in_range(object, "timeoutMs", 1, 10_000)
            .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidManifest))?,
    };
    manifest.validate()?;
    Ok(manifest)
}

/// Parses one provider response record after its newline delimiter has been removed.
pub fn parse_provider_response(record: &[u8]) -> ProtocolResult<ProviderResponse> {
    let value = parse_record(record)?;
    let object = as_object(&value)?;
    validate_protocol_version(object)?;

    match required_string(object, "type")? {
        "hello" => {
            if required_bool(object, "accepted")? {
                Ok(ProviderResponse::Hello)
            } else {
                Err(ProtocolError::new(ProtocolErrorKind::InvalidField))
            }
        }
        "results" => {
            let query_id = required_string(object, "queryId")?.to_owned();
            validate_request_id(&query_id)?;
            let complete = required_bool(object, "complete")?;
            let results = required_array(object, "results")
                .and_then(|entries| entries.iter().map(parse_provider_result).collect())?;
            Ok(ProviderResponse::Results {
                query_id,
                complete,
                results,
            })
        }
        "activated" => {
            let activation_id = required_string(object, "activationId")?.to_owned();
            validate_request_id(&activation_id)?;
            Ok(ProviderResponse::Activated { activation_id })
        }
        "error" => parse_provider_error(object).map(ProviderResponse::Error),
        _ => Err(ProtocolError::new(ProtocolErrorKind::UnknownRecordType)),
    }
}

fn parse_provider_result(value: &Value) -> ProtocolResult<ProviderResult> {
    let object =
        as_object(value).map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidResult))?;
    let result = ProviderResult {
        result_id: required_string(object, "resultId")?.to_owned(),
        kind: parse_result_kind(required_string(object, "kind")?)?,
        title: required_string(object, "title")?.to_owned(),
        subtitle: required_string(object, "subtitle")?.to_owned(),
        icon: required_string(object, "icon")?.to_owned(),
        score: required_score(object, "score")?,
    };
    result.validate()?;
    Ok(result)
}

fn parse_provider_error(object: &Map<String, Value>) -> ProtocolResult<ProviderError> {
    let correlation = match (object.get("queryId"), object.get("activationId")) {
        (Some(Value::String(query_id)), None) => {
            validate_request_id(query_id)?;
            ProviderErrorCorrelation::Query {
                query_id: query_id.clone(),
            }
        }
        (None, Some(Value::String(activation_id))) => {
            validate_request_id(activation_id)?;
            ProviderErrorCorrelation::Activation {
                activation_id: activation_id.clone(),
            }
        }
        _ => return Err(ProtocolError::new(ProtocolErrorKind::InvalidField)),
    };
    if let Some(message) = object.get("message") {
        if !message.is_string() {
            return Err(ProtocolError::new(ProtocolErrorKind::InvalidField));
        }
    }
    Ok(ProviderError {
        correlation,
        code: parse_provider_error_code(required_string(object, "code")?)?,
    })
}

fn parse_record(record: &[u8]) -> ProtocolResult<Value> {
    if record.len() > MAX_RECORD_BYTES {
        return Err(ProtocolError::new(ProtocolErrorKind::RecordTooLarge));
    }
    serde_json::from_slice(record).map_err(|_| ProtocolError::new(ProtocolErrorKind::MalformedJson))
}

fn as_object(value: &Value) -> ProtocolResult<&Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::ExpectedObject))
}

fn required_string<'a>(object: &'a Map<String, Value>, name: &str) -> ProtocolResult<&'a str> {
    object
        .get(name)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_str()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidField))
}

fn required_bool(object: &Map<String, Value>, name: &str) -> ProtocolResult<bool> {
    object
        .get(name)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_bool()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidField))
}

fn required_array<'a>(object: &'a Map<String, Value>, name: &str) -> ProtocolResult<&'a [Value]> {
    object
        .get(name)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidField))
}

fn required_u16_in_range(
    object: &Map<String, Value>,
    name: &str,
    minimum: u16,
    maximum: u16,
) -> ProtocolResult<u16> {
    let value = object
        .get(name)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_u64()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidField))?;
    let value =
        u16::try_from(value).map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidField))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(ProtocolError::new(ProtocolErrorKind::InvalidField));
    }
    Ok(value)
}

fn required_limit(object: &Map<String, Value>, name: &str) -> ProtocolResult<u8> {
    let value = object
        .get(name)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_u64()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidLimit))?;
    let value =
        u8::try_from(value).map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidLimit))?;
    validate_limit(value)?;
    Ok(value)
}

fn required_score(object: &Map<String, Value>, name: &str) -> ProtocolResult<f64> {
    let score = object
        .get(name)
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_f64()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidResult))?;
    validate_score(score)?;
    Ok(score)
}

/// Serializes one daemon event and appends the newline transport delimiter.
pub fn encode_daemon_event_line(event: &DaemonEvent) -> ProtocolResult<Vec<u8>> {
    match event {
        DaemonEvent::ShowSearch { monotonic_usec } => {
            validate_decimal_string(monotonic_usec)?;
            encode_line(&ShowSearchWire {
                protocol_version: PROTOCOL_VERSION,
                record_type: "show-search",
                monotonic_usec,
            })
        }
        DaemonEvent::IntegrationState { state } => encode_line(&IntegrationStateWire {
            protocol_version: PROTOCOL_VERSION,
            record_type: "integration-state",
            name: "gnoblin-super-release",
            state,
        }),
        DaemonEvent::Results {
            request_id,
            complete,
            elapsed_usec,
            results,
        } => {
            validate_request_id(request_id)?;
            results.iter().try_for_each(DaemonResult::validate)?;
            encode_line(&DaemonResultsWire {
                protocol_version: PROTOCOL_VERSION,
                record_type: "results",
                request_id,
                complete: *complete,
                elapsed_usec: *elapsed_usec,
                results,
            })
        }
        DaemonEvent::Activated { request_id } => {
            validate_request_id(request_id)?;
            encode_line(&ActivatedWire {
                protocol_version: PROTOCOL_VERSION,
                record_type: "activated",
                request_id,
            })
        }
        DaemonEvent::Error { request_id, code } => {
            validate_request_id(request_id)?;
            encode_line(&DaemonErrorWire {
                protocol_version: PROTOCOL_VERSION,
                record_type: "error",
                request_id,
                code,
                message: daemon_error_message(*code),
            })
        }
    }
}

/// Serializes one daemon request to a provider and appends the newline delimiter.
pub fn encode_provider_request_line(request: &ProviderRequest) -> ProtocolResult<Vec<u8>> {
    match request {
        ProviderRequest::Hello => encode_line(&ProviderHelloWire {
            protocol_version: PROTOCOL_VERSION,
            record_type: "hello",
            host_id: HOST_ID,
        }),
        ProviderRequest::Query {
            query_id,
            query,
            limit,
        } => {
            validate_request_id(query_id)?;
            validate_query(query)?;
            validate_limit(*limit)?;
            encode_line(&ProviderQueryWire {
                protocol_version: PROTOCOL_VERSION,
                record_type: "query",
                query_id,
                query,
                limit,
            })
        }
        ProviderRequest::Activate {
            activation_id,
            result_id,
        } => {
            validate_request_id(activation_id)?;
            validate_provider_result_id(result_id)?;
            encode_line(&ProviderActivateWire {
                protocol_version: PROTOCOL_VERSION,
                record_type: "activate",
                activation_id,
                result_id,
            })
        }
    }
}

/// Fixed daemon error copy. No untrusted payload is interpolated into a wire error.
pub const fn daemon_error_message(code: DaemonErrorCode) -> &'static str {
    match code {
        DaemonErrorCode::InvalidRequest => "request is invalid",
        DaemonErrorCode::UnsupportedProtocol => "protocol version is unsupported",
        DaemonErrorCode::Unavailable => "search service is unavailable",
        DaemonErrorCode::ProviderFailed => "a search provider failed",
        DaemonErrorCode::UnknownResult => "result is no longer available",
    }
}

fn validate_protocol_version(object: &Map<String, Value>) -> ProtocolResult<()> {
    let version = object
        .get("protocolVersion")
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::MissingField))?
        .as_u64()
        .ok_or_else(|| ProtocolError::new(ProtocolErrorKind::InvalidField))?;
    if version == u64::from(PROTOCOL_VERSION) {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::UnsupportedProtocol))
    }
}

fn parse_result_kind(value: &str) -> ProtocolResult<ResultKind> {
    match value {
        "application" => Ok(ResultKind::Application),
        "file" => Ok(ResultKind::File),
        "folder" => Ok(ResultKind::Folder),
        "database" => Ok(ResultKind::Database),
        "calculation" => Ok(ResultKind::Calculation),
        "weather" => Ok(ResultKind::Weather),
        "chat" => Ok(ResultKind::Chat),
        "action" => Ok(ResultKind::Action),
        _ => Err(ProtocolError::new(ProtocolErrorKind::InvalidResult)),
    }
}

fn parse_startup(value: &str) -> ProtocolResult<ProviderStartup> {
    match value {
        "eager" => Ok(ProviderStartup::Eager),
        "lazy" => Ok(ProviderStartup::Lazy),
        _ => Err(ProtocolError::new(ProtocolErrorKind::InvalidManifest)),
    }
}

fn parse_provider_error_code(value: &str) -> ProtocolResult<ProviderErrorCode> {
    match value {
        "invalid-request" => Ok(ProviderErrorCode::InvalidRequest),
        "unavailable" => Ok(ProviderErrorCode::Unavailable),
        "provider-failed" => Ok(ProviderErrorCode::ProviderFailed),
        _ => Err(ProtocolError::new(ProtocolErrorKind::InvalidField)),
    }
}

fn validate_request_id(value: &str) -> ProtocolResult<()> {
    if value.len() >= 1
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidIdentifier))
    }
}

fn validate_provider_id(value: &str) -> ProtocolResult<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes[0] == b'-'
        || bytes.last() == Some(&b'-')
        || bytes.windows(2).any(|window| window == b"--")
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidIdentifier))
    } else {
        Ok(())
    }
}

fn validate_provider_result_id(value: &str) -> ProtocolResult<()> {
    if value.len() >= 1
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidIdentifier))
    }
}

fn validate_opaque_result_id(value: &str) -> ProtocolResult<()> {
    if value.is_empty() {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidIdentifier))
    } else {
        Ok(())
    }
}

fn validate_query(value: &str) -> ProtocolResult<()> {
    if value.len() <= MAX_QUERY_BYTES {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidQuery))
    }
}

fn validate_limit(value: u8) -> ProtocolResult<()> {
    if (MIN_QUERY_LIMIT..=MAX_QUERY_LIMIT).contains(&value) {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidLimit))
    }
}

fn validate_score(value: f64) -> ProtocolResult<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidResult))
    }
}

fn validate_decimal_string(value: &str) -> ProtocolResult<()> {
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err(ProtocolError::new(ProtocolErrorKind::InvalidField))
    }
}

fn encode_line<T: Serialize>(record: &T) -> ProtocolResult<Vec<u8>> {
    let mut line = serde_json::to_vec(record)
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::InvalidField))?;
    if line.len() > MAX_RECORD_BYTES {
        return Err(ProtocolError::new(ProtocolErrorKind::RecordTooLarge));
    }
    line.push(b'\n');
    Ok(line)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShowSearchWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    monotonic_usec: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IntegrationStateWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    name: &'static str,
    state: &'a IntegrationState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonResultsWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    request_id: &'a str,
    complete: bool,
    elapsed_usec: u64,
    results: &'a [DaemonResult],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivatedWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    request_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonErrorWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    request_id: &'a str,
    code: &'a DaemonErrorCode,
    message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderHelloWire {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    host_id: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderQueryWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    query_id: &'a str,
    query: &'a str,
    limit: &'a u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderActivateWire<'a> {
    protocol_version: u8,
    #[serde(rename = "type")]
    record_type: &'static str,
    activation_id: &'a str,
    result_id: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_shell_query() {
        let request = parse_shell_request(
            br#"{"protocolVersion":1,"type":"query","requestId":"q-01","query":"firefox","limit":20}"#,
        )
        .expect("valid query request");

        assert_eq!(
            request,
            ShellRequest::Query(QueryRequest {
                request_id: "q-01".into(),
                query: "firefox".into(),
                limit: 20,
            })
        );
    }

    #[test]
    fn rejects_other_protocol_versions() {
        let error = parse_shell_request(
            br#"{"protocolVersion":2,"type":"query","requestId":"q-01","query":"firefox","limit":20}"#,
        )
        .expect_err("v2 must be rejected");

        assert_eq!(error.kind(), ProtocolErrorKind::UnsupportedProtocol);
    }

    #[test]
    fn rejects_records_larger_than_64_kib() {
        let padding = "x".repeat(MAX_RECORD_BYTES);
        let record = format!(
            r#"{{"protocolVersion":1,"type":"query","requestId":"q-01","query":"","limit":1,"padding":"{padding}"}}"#
        );

        let error = parse_shell_request(record.as_bytes()).expect_err("oversized record must fail");
        assert_eq!(error.kind(), ProtocolErrorKind::RecordTooLarge);
    }

    #[test]
    fn rejects_query_over_512_utf8_bytes() {
        let query = "é".repeat(257);
        let record = format!(
            r#"{{"protocolVersion":1,"type":"query","requestId":"q-01","query":"{query}","limit":20}}"#
        );

        let error = parse_shell_request(record.as_bytes()).expect_err("514-byte query must fail");
        assert_eq!(error.kind(), ProtocolErrorKind::InvalidQuery);
    }

    #[test]
    fn rejects_query_limits_outside_the_contract_range() {
        for limit in ["0", "51", "1.5"] {
            let record = format!(
                r#"{{"protocolVersion":1,"type":"query","requestId":"q-01","query":"firefox","limit":{limit}}}"#
            );
            let error =
                parse_shell_request(record.as_bytes()).expect_err("invalid limit must fail");
            assert_eq!(error.kind(), ProtocolErrorKind::InvalidLimit);
        }
    }

    #[test]
    fn rejects_invalid_request_identifiers() {
        let error = parse_shell_request(
            br#"{"protocolVersion":1,"type":"activate","requestId":"bad id","resultId":"app:firefox.desktop"}"#,
        )
        .expect_err("spaces are not valid request identifiers");

        assert_eq!(error.kind(), ProtocolErrorKind::InvalidIdentifier);
    }

    #[test]
    fn parses_valid_provider_results() {
        let response = parse_provider_response(
            br#"{"protocolVersion":1,"type":"results","queryId":"provider-query-01","complete":false,"results":[{"resultId":"firefox.desktop","kind":"application","title":"Firefox","subtitle":"Web browser","icon":"firefox","score":0.98}]}"#,
        )
        .expect("valid provider results");

        assert_eq!(
            response,
            ProviderResponse::Results {
                query_id: "provider-query-01".into(),
                complete: false,
                results: vec![ProviderResult {
                    result_id: "firefox.desktop".into(),
                    kind: ResultKind::Application,
                    title: "Firefox".into(),
                    subtitle: "Web browser".into(),
                    icon: "firefox".into(),
                    score: 0.98,
                }],
            }
        );
    }

    #[test]
    fn rejects_provider_result_scores_outside_zero_through_one() {
        let error = parse_provider_response(
            br#"{"protocolVersion":1,"type":"results","queryId":"q-01","complete":true,"results":[{"resultId":"firefox.desktop","kind":"application","title":"Firefox","subtitle":"Web browser","icon":"firefox","score":1.01}]}"#,
        )
        .expect_err("score above one must fail");

        assert_eq!(error.kind(), ProtocolErrorKind::InvalidResult);
    }

    #[test]
    fn rejects_unknown_result_kinds() {
        let error = parse_provider_response(
            br#"{"protocolVersion":1,"type":"results","queryId":"q-01","complete":true,"results":[{"resultId":"firefox.desktop","kind":"command","title":"Firefox","subtitle":"Web browser","icon":"firefox","score":0.98}]}"#,
        )
        .expect_err("unknown kind must fail");

        assert_eq!(error.kind(), ProtocolErrorKind::InvalidResult);
    }

    #[test]
    fn parses_a_valid_provider_manifest() {
        let manifest = parse_provider_manifest(
            br#"{"kind":"bingux.search-provider","protocolVersion":1,"id":"apps","displayName":"Applications","command":["/nix/store/example/bin/bingux-provider-apps"],"startup":"eager","priority":100,"timeoutMs":20}"#,
        )
        .expect("valid provider manifest");

        assert_eq!(manifest.id, "apps");
        assert_eq!(
            manifest.command,
            ["/nix/store/example/bin/bingux-provider-apps"]
        );
        assert_eq!(manifest.timeout_ms, 20);
    }

    #[test]
    fn rejects_malformed_provider_manifest_fields() {
        let error = parse_provider_manifest(
            br#"{"kind":"bingux.search-provider","protocolVersion":1,"id":"Apps","displayName":"Applications","command":[],"startup":"eager","priority":1001,"timeoutMs":0}"#,
        )
        .expect_err("manifest identifier and numeric bounds must be validated");

        assert_eq!(error.kind(), ProtocolErrorKind::InvalidManifest);
    }

    #[test]
    fn encodes_safe_daemon_errors_without_payload_details() {
        let line = encode_daemon_event_line(&DaemonEvent::Error {
            request_id: "q-01".into(),
            code: DaemonErrorCode::ProviderFailed,
        })
        .expect("valid daemon error");

        assert_eq!(
            line,
            b"{\"protocolVersion\":1,\"type\":\"error\",\"requestId\":\"q-01\",\"code\":\"provider-failed\",\"message\":\"a search provider failed\"}\n"
        );
    }

    #[test]
    fn encodes_provider_queries_as_newline_delimited_records() {
        let line = encode_provider_request_line(&ProviderRequest::Query {
            query_id: "provider-query-01".into(),
            query: "firefox".into(),
            limit: 20,
        })
        .expect("valid provider query");

        assert_eq!(
            line,
            b"{\"protocolVersion\":1,\"type\":\"query\",\"queryId\":\"provider-query-01\",\"query\":\"firefox\",\"limit\":20}\n"
        );
    }
}
