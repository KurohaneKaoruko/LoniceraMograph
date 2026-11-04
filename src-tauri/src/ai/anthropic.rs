//! Anthropic Messages API（流式 SSE，支持工具调用）。

use serde_json::{json, Value};

use super::{endpoint, read_lines, truncate, AiConfig, ChatMessage, Sink, TurnResult};

const DEFAULT_BASE: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-sonnet-4-5";
const API_VERSION: &str = "2023-06-01";

/// OpenAI 工具定义 → Anthropic 格式。
fn convert_tools(tools: &Value) -> Value {
    let arr: Vec<Value> = tools
        .as_array()
        .map(|v| v.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.get("name").cloned().unwrap_or(json!("unknown")),
                "description": t.get("description").cloned().unwrap_or(json!("")),
                "input_schema": t.get("input_schema").cloned().unwrap_or(json!({"type":"object"})),
            })
        })
        .collect();
    Value::Array(arr)
}

/// 内部消息列表 → Anthropic messages（tool 结果用 tool_result block）。
fn build_messages(history: &[ChatMessage], user_message: Option<&str>) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();
    for msg in history {
        match msg.role.as_str() {
            "tool" => {
                // tool 结果挂在最近一条 assistant 消息之后
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
                    "content": msg.content,
                });
                match messages.last_mut() {
                    Some(last) if last.get("role").and_then(|r| r.as_str()) == Some("user") => {
                        if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                            arr.push(block);
                        }
                    }
                    _ => {
                        messages.push(json!({ "role": "user", "content": [block] }));
                    }
                }
            }
            "assistant" if msg.tool_calls.is_some() => {
                let mut blocks: Vec<Value> = Vec::new();
                if !msg.content.is_empty() {
                    blocks.push(json!({ "type": "text", "text": msg.content }));
                }
                if let Some(tcs) = msg.tool_calls.as_ref().and_then(|v| v.as_array()) {
                    for tc in tcs {
                        let name = tc
                            .pointer("/function/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let args_str = tc
                            .pointer("/function/arguments")
                            .and_then(|v| v.as_str())
                            .unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.get("id").cloned().unwrap_or(json!("toolu_0")),
                            "name": name,
                            "input": input,
                        }));
                    }
                }
                messages.push(json!({ "role": "assistant", "content": blocks }));
            }
            role => {
                let role = if role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                // 合并相邻同角色消息（Anthropic 要求交替）
                let mergeable = messages.last().is_some_and(|last| {
                    last.get("role").and_then(|r| r.as_str()) == Some(role)
                        && last.get("content").and_then(|c| c.as_array()).is_some()
                });
                if mergeable {
                    if let Some(last) = messages.last_mut() {
                        if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                            arr.push(json!({ "type": "text", "text": msg.content }));
                        }
                    }
                } else {
                    messages.push(json!({
                        "role": role,
                        "content": [{ "type": "text", "text": msg.content }],
                    }));
                }
            }
        }
    }
    if let Some(user) = user_message {
        messages.push(json!({
            "role": "user",
            "content": [{ "type": "text", "text": user }],
        }));
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
    if config.api_key.trim().is_empty() {
        return Err("请先点击「设置」填写 Anthropic API Key。".into());
    }

    let url = endpoint(&config.base_url, DEFAULT_BASE, "/v1/messages");
    let model = if config.model.trim().is_empty() {
        DEFAULT_MODEL
    } else {
        config.model.trim()
    };

    let mut body = json!({
        "model": model,
        "max_tokens": 8192,
        "system": system,
        "messages": build_messages(history, user_message),
        "stream": true,
    });
    if let Some(tools) = tools {
        body["tools"] = convert_tools(tools);
    }

    let resp = client
        .post(&url)
        .header("x-api-key", config.api_key.trim())
        .header("anthropic-version", API_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 {url} 失败：{e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic 返回 {status}：{}", truncate(&text, 400)));
    }

    let mut acc = String::new();
    // 工具调用累积：index → (id, name, json 分片)
    let mut tool_acc: std::collections::BTreeMap<u64, (String, String, String)> =
        std::collections::BTreeMap::new();
    let mut stream_error: Option<String> = None;

    read_lines(resp, |line| {
        if stream_error.is_some() {
            return;
        }
        let Some(data) = line.trim().strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data.is_empty() {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        match value.get("type").and_then(|v| v.as_str()) {
            Some("content_block_delta") => {
                match value.pointer("/delta/type").and_then(|v| v.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) = value.pointer("/delta/text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                acc.push_str(text);
                                sink.delta(text);
                            }
                        }
                    }
                    Some("input_json_delta") => {
                        let idx = value
                            .pointer("/index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        if let Some(partial) = value
                            .pointer("/delta/partial_json")
                            .and_then(|v| v.as_str())
                        {
                            tool_acc.entry(idx).or_default().2.push_str(partial);
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_start") => {
                if value
                    .pointer("/content_block/type")
                    .and_then(|v| v.as_str())
                    == Some("tool_use")
                {
                    let idx = value
                        .pointer("/index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let id = value
                        .pointer("/content_block/id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("toolu_0")
                        .to_string();
                    let name = value
                        .pointer("/content_block/name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    tool_acc.insert(idx, (id, name, String::new()));
                }
            }
            Some("error") => {
                let message = value
                    .pointer("/error/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知错误");
                stream_error = Some(message.to_string());
            }
            _ => {}
        }
    })
    .await?;

    if let Some(err) = stream_error {
        return Err(format!("Anthropic 流式返回错误：{err}"));
    }

    let calls: Vec<Value> = tool_acc
        .into_values()
        .filter(|(_, name, _)| !name.is_empty())
        .map(|(id, name, args)| {
            json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": args },
            })
        })
        .collect();

    if acc.trim().is_empty() && calls.is_empty() {
        return Err("Anthropic 没有返回任何内容，请检查模型名与 API Key。".into());
    }
    Ok(TurnResult {
        text: acc,
        tool_calls: Value::Array(calls),
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
    do_stream(
        client,
        sink,
        config,
        system,
        history,
        Some(user_message),
        None,
    )
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
