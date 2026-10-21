//! OpenAI 兼容接口（OpenAI / DeepSeek / Moonshot / 通义千问 等）。
//! 支持普通流式对话与带工具调用的 agent 轮次。

use serde_json::{json, Value};

use super::{endpoint, read_lines, truncate, AiConfig, ChatMessage, Sink, TurnResult};

const DEFAULT_BASE: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

fn build_messages(system: &str, history: &[ChatMessage], user_message: Option<&str>) -> Vec<Value> {
    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(json!({ "role": "system", "content": system }));
    for msg in history {
        match msg.role.as_str() {
            "tool" => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(),
                    "content": msg.content,
                }));
            }
            "assistant" if msg.tool_calls.is_some() => {
                let mut m = json!({ "role": "assistant", "content": msg.content });
                if let Some(tc) = msg.tool_calls.as_ref() {
                    m["tool_calls"] = tc.clone();
                }
                messages.push(m);
            }
            role => {
                let role = if role == "assistant" { "assistant" } else { "user" };
                messages.push(json!({ "role": role, "content": msg.content }));
            }
        }
    }
    if let Some(user) = user_message {
        messages.push(json!({ "role": "user", "content": user }));
    }
    messages
}

async fn do_stream(
    client: &reqwest::Client,
    sink: &Sink,
    config: &AiConfig,
    system: &str,
    messages: &[ChatMessage],
    user_message: Option<&str>,
    tools: Option<&Value>,
) -> Result<TurnResult, String> {
    let url = endpoint(&config.base_url, DEFAULT_BASE, "/chat/completions");
    let model = if config.model.trim().is_empty() {
        DEFAULT_MODEL
    } else {
        config.model.trim()
    };

    let mut body = json!({
        "model": model,
        "messages": build_messages(system, messages, user_message),
        "stream": true,
        "temperature": 0.7,
    });
    if let Some(tools) = tools {
        body["tools"] = tools.clone();
        body["tool_choice"] = json!("auto");
    }

    let mut request = client.post(&url).json(&body);
    if !config.api_key.trim().is_empty() {
        request = request.bearer_auth(config.api_key.trim());
    }

    let resp = request
        .send()
        .await
        .map_err(|e| format!("请求 {url} 失败：{e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("AI 接口返回 {status}：{}", truncate(&text, 400)));
    }

    // 流式累积：文本 + 分片 tool_calls
    let mut acc = String::new();
    let mut tool_acc: Vec<Value> = Vec::new();
    let mut stream_error: Option<String> = None;

    read_lines(resp, |line| {
        if stream_error.is_some() {
            return;
        }
        let Some(data) = line.trim().strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if let Some(err) = value.pointer("/error/message").and_then(|v| v.as_str()) {
            stream_error = Some(err.to_string());
            return;
        }
        let Some(delta) = value.pointer("/choices/0/delta") else {
            return;
        };
        if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                acc.push_str(text);
                sink.delta(text);
            }
        }
        if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                while tool_acc.len() <= idx {
                    tool_acc.push(json!({
                        "id": "", "type": "function",
                        "function": { "name": "", "arguments": "" }
                    }));
                }
                let slot = &mut tool_acc[idx];
                if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                    slot["id"] = json!(id);
                }
                if let Some(name) = tc.pointer("/function/name").and_then(|v| v.as_str()) {
                    slot["function"]["name"] = json!(name);
                }
                if let Some(args) = tc.pointer("/function/arguments").and_then(|v| v.as_str()) {
                    slot["function"]["arguments"] =
                        json!(format!("{}{}", slot["function"]["arguments"].as_str().unwrap_or(""), args));
                }
            }
        }
    })
    .await?;

    if let Some(err) = stream_error {
        return Err(format!("AI 流式返回错误：{err}"));
    }

    // 过滤掉名字为空的分片
    let calls: Vec<Value> = tool_acc
        .into_iter()
        .filter(|tc| !tc["function"]["name"].as_str().unwrap_or("").is_empty())
        .map(|mut tc| {
            if tc["id"].as_str().unwrap_or("").is_empty() {
                tc["id"] = json!(format!("call_{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0)));
            }
            tc
        })
        .collect();

    if acc.trim().is_empty() && calls.is_empty() {
        return Err("AI 没有返回任何内容，请检查模型名、Base URL 和 API Key。".into());
    }
    Ok(TurnResult {
        text: acc,
        tool_calls: Value::Array(calls),
    })
}

/// 普通流式对话（无工具，供非 agent 调用方使用）。
#[allow(dead_code)]
pub async fn stream(
    client: &reqwest::Client,
    sink: &Sink,
    config: &AiConfig,
    system: &str,
    history: &[ChatMessage],
    user_message: &str,
) -> Result<String, String> {
    do_stream(client, sink, config, system, history, Some(user_message), None)
        .await
        .map(|t| t.text)
}

/// agent 轮次：流式输出 + 工具调用收集。
pub async fn turn(
    client: &reqwest::Client,
    sink: &Sink,
    config: &AiConfig,
    system: &str,
    messages: &[ChatMessage],
    tools: &Value,
) -> Result<TurnResult, String> {
    do_stream(client, sink, config, system, messages, None, Some(tools)).await
}
