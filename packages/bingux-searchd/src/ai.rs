use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    config::AiConfig,
    protocol::{MAX_CHAT_RESPONSE_BYTES, MAX_QUERY_BYTES, validate_chat_response_message},
};

const MAX_AI_ENDPOINT_BYTES: usize = 2 * 1024;
const MAX_API_KEY_BYTES: usize = 4 * 1024;
const MAX_CHAT_RESPONSE_BODY_BYTES: u64 = 32 * 1024;
const MAX_CHAT_COMPLETION_TOKENS: u16 = 1_024;
const CHAT_TIMEOUT: Duration = Duration::from_secs(20);
pub const MAX_CHAT_HISTORY_EXCHANGES: usize = 6;

#[derive(Clone)]
pub struct AiProvider {
    agent: ureq::Agent,
    endpoint: String,
    model: String,
    api_key_file: PathBuf,
}

#[derive(Default, Clone)]
pub struct ChatHistory {
    exchanges: VecDeque<ChatExchange>,
}

#[derive(Clone)]
struct ChatExchange {
    user: String,
    assistant: Arc<str>,
}

#[derive(Clone, Copy, Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    max_tokens: u16,
}

impl ChatHistory {
    pub fn record(&mut self, user: String, assistant: impl Into<Arc<str>>) {
        while self.exchanges.len() >= MAX_CHAT_HISTORY_EXCHANGES {
            self.exchanges.pop_front();
        }
        self.exchanges.push_back(ChatExchange {
            user,
            assistant: assistant.into(),
        });
    }

    fn messages_with<'a>(&'a self, prompt: &'a str) -> Vec<ChatMessage<'a>> {
        let mut messages = Vec::with_capacity(self.exchanges.len().saturating_mul(2) + 1);
        for exchange in &self.exchanges {
            messages.push(ChatMessage {
                role: "user",
                content: &exchange.user,
            });
            messages.push(ChatMessage {
                role: "assistant",
                content: exchange.assistant.as_ref(),
            });
        }
        messages.push(ChatMessage {
            role: "user",
            content: prompt,
        });
        messages
    }
}

impl AiProvider {
    pub fn new(config: AiConfig) -> Result<Self> {
        validate_endpoint(&config.endpoint)?;
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .https_only(true)
                .max_redirects(0)
                .timeout_global(Some(CHAT_TIMEOUT))
                .build(),
        );
        Ok(Self {
            agent,
            endpoint: config.endpoint,
            model: config.model,
            api_key_file: config.api_key_file,
        })
    }

    pub fn complete(&self, history: &ChatHistory, prompt: &str) -> Result<String> {
        if prompt.is_empty() || prompt.len() > MAX_QUERY_BYTES {
            bail!("AI prompt is invalid");
        }

        let api_key = self.read_api_key()?;
        let request = ChatCompletionRequest {
            model: &self.model,
            messages: history.messages_with(prompt),
            max_tokens: MAX_CHAT_COMPLETION_TOKENS,
        };
        let authorization = format!("Bearer {api_key}");
        let mut response = self
            .agent
            .post(&self.endpoint)
            .header("Authorization", authorization.as_str())
            .send_json(&request)
            .map_err(|_| anyhow::anyhow!("AI provider request failed"))?;
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_CHAT_RESPONSE_BODY_BYTES)
            .read_to_string()
            .map_err(|_| anyhow::anyhow!("AI provider response failed"))?;

        parse_first_response(&body)
    }

    fn read_api_key(&self) -> Result<String> {
        read_api_key_file(&self.api_key_file)
    }
}

fn read_api_key_file(path: &Path) -> Result<String> {
    let maximum_bytes = u64::try_from(MAX_API_KEY_BYTES + 1).expect("API key bound fits u64");
    let mut contents = String::with_capacity(MAX_API_KEY_BYTES + 1);
    File::open(path)
        .map_err(|_| anyhow::anyhow!("AI API key is unavailable"))?
        .take(maximum_bytes)
        .read_to_string(&mut contents)
        .map_err(|_| anyhow::anyhow!("AI API key is unavailable"))?;
    let api_key = contents.trim();
    if api_key.is_empty()
        || api_key.len() > MAX_API_KEY_BYTES
        || api_key.chars().any(char::is_control)
    {
        bail!("AI API key is invalid");
    }
    Ok(api_key.to_owned())
}

pub fn validate_endpoint(endpoint: &str) -> Result<()> {
    if endpoint.is_empty()
        || endpoint.len() > MAX_AI_ENDPOINT_BYTES
        || endpoint.chars().any(char::is_control)
    {
        bail!("AI endpoint is invalid");
    }

    let endpoint = endpoint
        .parse::<ureq::http::Uri>()
        .map_err(|_| anyhow::anyhow!("AI endpoint is invalid"))?;
    if endpoint.scheme_str() != Some("https") || endpoint.authority().is_none() {
        bail!("AI endpoint must use HTTPS");
    }
    Ok(())
}

fn parse_first_response(body: &str) -> Result<String> {
    let response: Value = serde_json::from_str(body)
        .map_err(|_| anyhow::anyhow!("AI provider response is invalid"))?;
    let message = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object)
        .and_then(|choice| choice.get("message"))
        .and_then(Value::as_object)
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("AI provider response is invalid"))?;
    let message = message.trim();
    if message.is_empty() || message.chars().any(char::is_control) {
        bail!("AI provider response is invalid");
    }

    let message = truncate_utf8(message, MAX_CHAT_RESPONSE_BYTES);
    validate_chat_response_message(&message)
        .map_err(|_| anyhow::anyhow!("AI provider response is invalid"))?;
    Ok(message)
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }

    let mut end = 0;
    for character in value.chars() {
        let next = end + character.len_utf8();
        if next > maximum_bytes {
            break;
        }
        end = next;
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        ChatHistory, MAX_API_KEY_BYTES, MAX_CHAT_HISTORY_EXCHANGES, MAX_CHAT_RESPONSE_BYTES,
        parse_first_response, read_api_key_file,
    };
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_key_path() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bingux-searchd-ai-key-test-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn rejects_an_api_key_file_larger_than_the_bound() {
        let path = temporary_key_path();
        fs::write(&path, "a".repeat(MAX_API_KEY_BYTES + 1)).expect("write oversized key");

        assert!(read_api_key_file(&path).is_err());

        fs::remove_file(path).expect("remove oversized key");
    }

    #[test]
    fn parses_only_the_first_bounded_text_choice() {
        let response = format!(
            r#"{{"choices":[{{"message":{{"content":"  {}  "}}}},{{"message":{{"content":"ignored"}}}}]}}"#,
            "é".repeat(7_000),
        );

        let message = parse_first_response(&response).expect("first text choice");

        assert_eq!(message.len(), MAX_CHAT_RESPONSE_BYTES);
        assert!(message.chars().all(|character| !character.is_control()));
        assert!(message.chars().all(|character| character == 'é'));
    }

    #[test]
    fn rejects_empty_or_control_character_responses() {
        for content in ["   ", "answer\\nnext"] {
            let response = format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#);
            assert!(parse_first_response(&response).is_err());
        }
    }

    #[test]
    fn retains_only_the_latest_six_exchanges() {
        let mut history = ChatHistory::default();
        for index in 0..=MAX_CHAT_HISTORY_EXCHANGES {
            history.record(format!("user-{index}"), format!("assistant-{index}"));
        }

        assert_eq!(history.exchanges.len(), MAX_CHAT_HISTORY_EXCHANGES);
        assert_eq!(
            history.exchanges.front().expect("oldest retained").user,
            "user-1"
        );
        assert_eq!(
            history
                .exchanges
                .back()
                .expect("newest retained")
                .assistant
                .as_ref(),
            "assistant-6"
        );
    }
}
