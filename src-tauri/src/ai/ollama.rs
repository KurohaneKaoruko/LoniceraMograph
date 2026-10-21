//! 本地 Ollama（原生 /api/chat，NDJSON 流，支持工具调用）。

use serde_json::{json, Value};

use super::{endpoint, read_lines, truncate, AiConfig, ChatMessage, Sink, TurnResult};

const DEFAULT_BASE: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "qwen2.5:7b";

fn build_messages(system: &str, history: &[ChatMessage], user_message: Option<&str>) -> Vec<Value> {
    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(json!({ "role": "system", "content": system }));
    for msg in history {
        match msg.role.as_str() {
            "tool" => {
                messages.push(json!({ "role": "tool", "content": msg.content }));
            }
            "assistant" if msg.tool_calls.is_some() => {
                let mut m = json!({ "role": "assistant", "content": msg.content });
                if let Some(tc) = msg.tool_calls.as_ref() {
                    // Ollama 的 tool_calls：{ function: { name, arguments: object } }
                    let converted: Vec<Value> = tc
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|c| {
                            let name = c.pointer("/function/name").cloned().unwrap_or(json!(""));
                            let args_str = c
                                .pointer("/function/arguments")
                                .and_then(|v| v.as_str())
                                .unwrap_or("{}");
                            let args: Value =
                                serde_json::from_str(args_str).unwrap_or(json!({}));
                            json!({ "function": { "name": name, "arguments": args } })
                        })
                        .collect();
                    m["tool_calls"] = Value::Array(converted);
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
    history: &[ChatMessage],
    user_message: Option<&str>,
    tools: Option<&Value>,
) -> Result<TurnResult, String> {
    let url = endpoint(&config.base_url, DEFAULT_BASE, "/api/chat");
    let model = if config.model.trim().is_empty() {
        DEFAULT_MODEL
    } else {
        config.model.trim()
    };

    let mut body = json!({
        "model": model,
        "messages": build_messages(system, history, user_message),
        "stream": true,
        "options": { "temperature": 0.7 },
    });
    if let Some(tools) = tools {
        body["tools"] = tools.clone();
    }

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("连接 Ollama（{url}）失败：{e}。请确认 `ollama serve` 已启动。"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama 返回 {status}：{}", truncate(&text, 400)));
    }

    let mut acc = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    let mut stream_error: Option<String> = None;

    read_lines(resp, |line| {
        if stream_error.is_some() {
            return;
        }
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
            stream_error = Some(err.to_string());
            return;
        }
        if let Some(text) = value.pointer("/message/content").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                acc.push_str(text);
                sink.delta(text);
            }
        }
        if let Some(tcs) = value.pointer("/message/tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let name = tc
                    .pointer("/function/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let args = tc
                    .pointer("/function/arguments")
                    .cloned()
                    .unwrap_or(json!({}));
                tool_calls.push(json!({
                    "id": format!("call_{}", tool_calls.len()),
                    "type": "function",
                    "function": { "name": name, "arguments": args.to_string() },
                }));
            }
        }
    })
    .await?;

    if let Some(err) = stream_error {
        return Err(format!("Ollama 返回错误：{err}"));
    }
    if acc.trim().is_empty() && tool_calls.is_empty() {
        return Err("Ollama 没有返回任何内容，请检查模型是否已 pull。".into());
    }
    Ok(TurnResult {
        text: acc,
        tool_calls: Value::Array(tool_calls),
    })
}

/// 普通流式对话（无工具）。
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
