//! AI 对话层：把用户的自然语言转成场景 JSON。
//!
//! 支持四种 Provider：
//! - `mock`      —— 离线演示，不联网，内置模板生成器
//! - `openai`    —— 任何 OpenAI 兼容接口（OpenAI / DeepSeek / Kimi / 通义 …）
//! - `anthropic` —— Anthropic Messages API
//! - `ollama`    —— 本地 Ollama（原生 /api/chat）
//!
//! 所有 Provider 都以流式方式把文本增量发给前端，结束后再从完整文本中
//! 提取 ```json 场景并交给 [`crate::scene::Scene`]。

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::events::StreamEvent;
use crate::scene::Scene;

/// 前端 localStorage 中的 AI 配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// tool_calls（assistant 消息）：本轮 agent 循环内部使用，不进前端历史。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    /// tool 消息：对应调用的工具名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// tool 消息标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn text(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            tool_calls: None,
            name: None,
            tool_call_id: None,
        }
    }
}

/// 事件发送器：Provider 只关心“把增量文本推给前端”。
#[derive(Clone)]
pub struct Sink(pub AppHandle);

impl Sink {
    pub fn delta(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let _ = self.0.emit(
            "stream",
            StreamEvent::Delta {
                text: text.to_string(),
            },
        );
    }
}

/// 系统提示词：约束模型输出一份可被严格解析的场景 JSON。
pub const SYSTEM_PROMPT: &str = r##"你是 LoniceraMograph 的 MG 动画导演兼场景编译器。用户用自然语言描述动画需求，你要先简要说明设计思路，再输出完整场景 JSON。

输出格式（严格遵守）：
1. 用 1-3 句中文说明设计思路，不要罗列 JSON。
2. 然后输出一个 ```json 代码块，里面是**完整**的场景 JSON（不是差异补丁），不要输出代码块之外的其他内容。

场景 JSON 结构：
{
  "name": "场景名称",
  "durationSec": 6,
  "width": 1280,
  "height": 720,
  "background": "#0e1017",
  "layers": [
    {
      "shape": { "kind": "text", "x": 640, "y": 320, "text": "标题", "size": 92, "fill": "#ffffff", "align": "center", "baseline": "middle" },
      "animations": [
        { "prop": "opacity", "from": 0, "to": 1, "start": 0.15, "end": 1.0, "easing": "easeOutCubic" },
        { "prop": "y", "from": 366, "to": 320, "start": 0.15, "end": 1.2, "easing": "easeOutCubic" }
      ]
    }
  ]
}

坐标系：左上角为原点，x 向右、y 向下，单位像素；画布固定 1280×720。图层按数组顺序绘制，越靠后越在上层。

shape 类型（除标注外字段必填）：
- rect:    { "kind": "rect", "x", "y", "w", "h", "fill", "radius"?, "opacity"?, "rotation"?, "stroke"?, "strokeWidth"? }
- circle:  { "kind": "circle", "x", "y", "r", "fill", "opacity"?, "rotation"?, "stroke"?, "strokeWidth"? }
- ellipse: { "kind": "ellipse", "x", "y", "rx", "ry", "fill", "opacity"?, "rotation"?, "stroke"?, "strokeWidth"? }
- line:    { "kind": "line", "x1", "y1", "x2", "y2", "stroke", "width", "opacity"? }
- text:    { "kind": "text", "x", "y", "text", "size", "fill", "opacity"?, "align"?: "left"|"center"|"right", "baseline"?: "top"|"middle"|"bottom", "rotation"?, "stroke"?, "strokeWidth"? }
- path:    { "kind": "path", "d", "fill", "opacity"?, "rotation"?, "stroke"?, "strokeWidth"? }   // d 为 SVG path 数据，坐标用画布绝对坐标

颜色使用 CSS 十六进制字符串；rotation 单位为弧度；opacity 取值 0-1。stroke/strokeWidth 可给填充图形加轮廓线（strokeWidth 缺省 2）。

animations 中每项把某个数值属性从 from 补间到 to：
- 可补间属性：x, y, w, h, r, rx, ry, x1, y1, x2, y2, size, opacity, rotation, radius, strokeWidth
- start / end 单位为秒，缺省 start=0、end=场景时长
- easing 可选：linear, easeIn, easeOut, easeInOut, easeInCubic, easeOutCubic, easeInOutCubic, easeOutBack, easeOutBounce（缺省 linear）
- 同一图层可以有多个动画，实现淡入 + 位移 + 缩放等组合
- 元素要错峰出现（stagger），入场多用 easeOutCubic，位移用 easeInOut，弹跳用 easeOutBack

设计准则：
- 时长 2-30 秒，通常 4-10 秒。
- 深色背景 + 1-2 个强调色，配色克制；文本对比度足够，标题字号 48-110。
- 构图留白，主体居中；标题建议 x=640、y 在 280-380。
- 画面要有层次：背景光斑/网格/粒子、主体文字、强调线/装饰点。
- 最后 10%-20% 的时间里安排收尾（淡出、扫光或元素退场），避免戛然而止。
- 用户要求修改时，输出修改后的**完整**场景，保留未被要求改动的内容。

严禁：JSON 注释、尾随逗号、省略号占位、markdown 之外的额外说明。必须能被 JSON.parse 直接解析。"##;

/// Agent 模式追加说明：工具使用守则（拼接在 SYSTEM_PROMPT 之后）。
pub const AGENT_PROMPT: &str = r##"
【Agent 能力】你可以调用工具操控项目工作区（~/Documents/LoniceraMograph/），目录约定：
- scenes/*.json —— 声明式场景
- scripts/*.js —— JS 代码生成场景（适合程序化/参数化动画，如粒子、波浪、随机布局）

工作方式：
1. 动手前先 list_files 了解已有文件；需要参考时 read_file。
2. 用 write_file 保存作品（场景→scenes/*.json；脚本→scripts/*.js）。
3. 用 load_scene 或 compile_script 把成果加载到预览，再用 get_scene_errors 自查，发现问题就修复并重新加载。
4. 任务完成后用一小段话总结：创建了哪些文件、场景设计要点。
5. 用户只是提问或聊设计思路时，直接回答，不必调用工具。
6. 修改已有场景时先 read_file 再改，保留未涉及的内容。

简单的一次性动画也可以直接在回复里输出 ```json 场景（前端会解析并加载）；但用户提到"保存/文件/脚本/代码/复杂/粒子/随机"等需求时，必须走文件工作流。"##;

/// 执行一轮对话（非 agent 模式，mock 用），返回（完整文本，可选场景）。
#[allow(dead_code)]
pub async fn run(
    app: &AppHandle,
    user_message: &str,
    config: &AiConfig,
    history: &[ChatMessage],
) -> Result<(String, Option<Scene>), String> {
    let sink = Sink(app.clone());
    let provider = config.provider.trim();

    if provider.is_empty() || provider == "mock" {
        let (text, scene) = mock::run(&sink, user_message).await;
        return Ok((text, Some(scene)));
    }
    let _ = sink;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))?;

    let history = recent_history(history);

    let text = match provider {
        "openai" => {
            openai::stream(
                &client,
                &sink,
                config,
                SYSTEM_PROMPT,
                &history,
                user_message,
            )
            .await?
        }
        "anthropic" => {
            anthropic::stream(
                &client,
                &sink,
                config,
                SYSTEM_PROMPT,
                &history,
                user_message,
            )
            .await?
        }
        "ollama" => {
            ollama::stream(
                &client,
                &sink,
                config,
                SYSTEM_PROMPT,
                &history,
                user_message,
            )
            .await?
        }
        other => return Err(format!("未知的 AI Provider：{other}")),
    };

    let scene = extract_scene(&text);
    Ok((text, scene))
}

/// 单轮流式请求（带工具定义）的统一入口，返回（文本，工具调用列表）。
pub struct TurnResult {
    pub text: String,
    /// OpenAI 格式的 tool_calls 原始值（数组）或空。
    pub tool_calls: serde_json::Value,
}

pub async fn turn_with_tools(
    client: &reqwest::Client,
    sink: &Sink,
    config: &AiConfig,
    system: &str,
    messages: &[ChatMessage],
    tools: &serde_json::Value,
) -> Result<TurnResult, String> {
    let provider = config.provider.trim();
    match provider {
        "openai" => openai::turn(client, sink, config, system, messages, tools).await,
        "anthropic" => anthropic::turn(client, sink, config, system, messages, tools).await,
        "ollama" => ollama::turn(client, sink, config, system, messages, tools).await,
        other => Err(format!("未知的 AI Provider：{other}")),
    }
}

/// Agent 循环：LLM ↔ 工具多步协作，直到模型给出最终回答或达到步数上限。
///
/// 每步的文本增量实时流给前端；工具调用以 `tool` 事件展示活动。
/// 返回（最终文本，本轮是否加载过新场景）。
pub async fn run_agent(
    app: &AppHandle,
    sink: &Sink,
    config: &AiConfig,
    user_message: &str,
    history: &[ChatMessage],
    max_steps: usize,
) -> Result<(String, bool), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))?;

    let system = format!(
        "{SYSTEM_PROMPT}{AGENT_PROMPT}\n{}",
        crate::agent_tools::SCRIPT_GUIDE
    );
    let tools = crate::agent_tools::definitions();

    // messages: 历史 + 本轮动态消息（assistant/tool 交替）
    let mut messages: Vec<ChatMessage> = recent_history(history);
    messages.push(ChatMessage::text("user", user_message));

    let mut loaded_scene = false;
    let mut final_text = String::new();
    // 12 步全部用来调工具、始终没给出最终回答时置 true。
    let mut ran_out = true;

    // 工具上下文在整个 agent 循环中保持：get_scene / get_scene_errors 需要
    // 读到此前任一步骤加载的场景，而不是只在同一步内可见。
    let ctx = crate::agent_tools::ToolCtx {
        app,
        current_scene: std::sync::Mutex::new(None),
    };

    for _step in 0..max_steps {
        let turn = turn_with_tools(&client, sink, config, &system, &messages, &tools).await?;
        final_text = turn.text.clone();

        let calls = turn.tool_calls.as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            // 模型给出最终回答，循环结束
            ran_out = false;
            break;
        }

        // 记录 assistant 消息（含 tool_calls）
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: turn.text,
            tool_calls: Some(turn.tool_calls.clone()),
            name: None,
            tool_call_id: None,
        });

        // 逐个执行工具（ctx 跨步骤共享）
        for call in calls {
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("call_0")
                .to_string();
            let name = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let raw_args = call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");

            // 参数不是合法 JSON 时不再静默回退空参数：那样工具只会报
            // 「缺少 path」之类表面原因，模型无从得知真实错误。把解析
            // 错误直接回传，模型下一轮可自行修正。
            let args = match parse_tool_args(raw_args) {
                Ok(v) => v,
                Err(e) => {
                    let detail =
                        format!("{e}，请修正后重试。原始参数：{}", truncate(raw_args, 300));
                    let _ = app.emit(
                        "stream",
                        crate::events::StreamEvent::Tool {
                            name: name.clone(),
                            detail: tool_detail(&name, &serde_json::json!({})),
                        },
                    );
                    messages.push(ChatMessage {
                        role: "tool".into(),
                        content: format!("错误：{detail}"),
                        tool_calls: None,
                        name: Some(name),
                        tool_call_id: Some(id),
                    });
                    continue;
                }
            };

            // 前端展示工具活动
            let _ = app.emit(
                "stream",
                crate::events::StreamEvent::Tool {
                    name: name.clone(),
                    detail: tool_detail(&name, &args),
                },
            );

            let result = match crate::agent_tools::execute(&ctx, &name, &args) {
                Ok(text) => text,
                Err(e) => format!("错误：{e}"),
            };
            // 工具成功即视作场景已加载（错误统一带「错误：」前缀，不再靠
            // 结果文本是否以「错误」开头反推——场景名恰好以这两个字开头时
            // 曾被误判）。成功标记只认工具名，避免脆弱的字符串约定。
            let is_error = result.starts_with("错误：");
            if (name == "load_scene" || name == "compile_script") && !is_error {
                loaded_scene = true;
            }

            let shown = crate::ai::truncate(&result, 1500);
            messages.push(ChatMessage {
                role: "tool".into(),
                content: shown,
                tool_calls: None,
                name: Some(name),
                tool_call_id: Some(id),
            });
        }

        // 场景状态已由 commands::push_scene 推给前端；ctx.current_scene
        // 供后续步骤的 get_scene / get_scene_errors 读取。
    }

    // 步数用尽且模型始终没给出最终文本：告知前端，而不是让最后一轮的
    // 工具结果留在气泡里、用户面对无声的「完成」。
    if ran_out && final_text.trim().is_empty() {
        return Err(format!(
            "已连续执行 {max_steps} 步仍未完成任务，已停止。请把需求拆小后重试。"
        ));
    }

    Ok((final_text, loaded_scene))
}

/// 解析工具调用参数：空串视作空对象，其余必须为合法 JSON 对象。
///
/// 部分模型会输出空字符串参数；`serde_json` 原样解析会失败，这里归一化。
fn parse_tool_args(raw: &str) -> Result<serde_json::Value, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(serde_json::json!({}));
    }
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("工具参数不是合法 JSON：{e}"))?;
    if !v.is_object() {
        return Err(format!(
            "工具参数应为 JSON 对象，实际为：{}",
            truncate(&v.to_string(), 80)
        ));
    }
    Ok(v)
}

/// 工具活动在前端的简短描述。
fn tool_detail(name: &str, args: &serde_json::Value) -> String {
    let get = |k: &str| {
        args.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match name {
        "list_files" => {
            let p = get("path");
            format!("📂 浏览 {}", if p.is_empty() { "工作区" } else { &p })
        }
        "read_file" => format!("📖 读取 {}", get("path")),
        "write_file" => format!("✏️ 写入 {}", get("path")),
        "delete_file" => format!("🗑 删除 {}", get("path")),
        "rename_file" => format!("🔁 {} → {}", get("from"), get("to")),
        "load_scene" => format!("🎬 加载 {}", get("path")),
        "compile_script" => format!("⚙️ 执行 {}", get("path")),
        "get_scene_errors" => "🔍 检查场景".into(),
        "get_scene" => "🔍 读取当前场景".into(),
        other => format!("🔧 {other}"),
    }
}

/// 只带最近若干轮，避免上下文过长。
///
/// 裁剪时必须保持 OpenAI/Anthropic 的消息配对完整性：
/// assistant(tool_calls) 与其后所有对应 tool 回复要么整体保留，要么整体丢弃，
/// 否则 API 会以「tool_call_id 无对应 tool_calls」直接拒绝整个请求。
fn recent_history(history: &[ChatMessage]) -> Vec<ChatMessage> {
    const MAX_MESSAGES: usize = 12;
    const MAX_CHARS: usize = 12_000;

    // 先按条数粗裁，再向左回退：若裁剪边界落进一组工具链中段
    // （起点是 tool 消息，或 assistant(tool_calls) 与其 tool 回复被拆开），
    // 继续丢弃更早的消息，直到边界对齐到安全位置。
    let end = history.len();
    let mut start = end.saturating_sub(MAX_MESSAGES);
    while start < end && breaks_tool_pairing(history, start, end) {
        start += 1;
    }

    let mut out: Vec<ChatMessage> = history[start..end].to_vec();

    for msg in &mut out {
        if msg.content.chars().count() > MAX_CHARS {
            msg.content = msg.content.chars().take(MAX_CHARS).collect::<String>();
        }
    }
    out
}

/// 判断 `[start, end)` 是否切开了某组 assistant(tool_calls) ↔ tool 回复。
///
/// 规则：切片起点不得是 tool 消息（其上游 assistant 已被裁掉）；
/// 若起前区间内存在 assistant(tool_calls)，其所有 tool 回复必须都在区间内。
fn breaks_tool_pairing(msgs: &[ChatMessage], start: usize, end: usize) -> bool {
    if start >= end || start == 0 {
        return false;
    }
    // 起点直接是 tool 回复：说明对应的 assistant(tool_calls) 被裁掉了
    if msgs[start].role == "tool" {
        return true;
    }
    // 收集区间内 assistant 消息声明的全部 tool_call id，
    // 区间内出现任一 id 的 tool 回复而没有对应声明 → 配对被切开
    let mut declared: Vec<String> = Vec::new();
    for msg in &msgs[start..end] {
        if msg.role == "assistant" {
            if let Some(calls) = msg.tool_calls.as_ref().and_then(|v| v.as_array()) {
                for c in calls {
                    if let Some(id) = c.get("id").and_then(|v| v.as_str()) {
                        declared.push(id.to_string());
                    }
                }
            }
        }
    }
    for msg in &msgs[start..end] {
        if msg.role == "tool" {
            if let Some(id) = msg.tool_call_id.as_ref() {
                if !declared.iter().any(|d| d == id) {
                    return true;
                }
            }
        }
    }
    false
}

/// 从模型回复中提取场景 JSON，容忍代码块、前后说明文字。
pub fn extract_scene(text: &str) -> Option<Scene> {
    let mut candidates: Vec<String> = Vec::new();

    // 1) 所有 ``` 代码块（优先 json 标记）
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let after = after.trim_start();
        let body = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("JSON"))
            .unwrap_or(after);
        if let Some(end) = body.find("```") {
            candidates.push(body[..end].trim().to_string());
            rest = &body[end + 3..];
        } else {
            candidates.push(body.trim().to_string());
            break;
        }
    }

    // 2) 第一个 { 到最后一个 } 之间的内容
    if let (Some(a), Some(b)) = (text.find('{'), text.rfind('}')) {
        if b > a {
            candidates.push(text[a..=b].to_string());
        }
    }

    // 3) 整段文本
    candidates.push(text.trim().to_string());

    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }
        if let Ok(mut scene) = serde_json::from_str::<Scene>(&candidate) {
            if scene.layers.is_empty() {
                continue;
            }
            scene.normalize();
            return Some(scene);
        }
    }
    None
}

/// 拼接接口地址：用户填的 base URL 可以带或不带接口路径。
pub fn endpoint(base: &str, default_base: &str, path: &str) -> String {
    let base = if base.trim().is_empty() {
        default_base
    } else {
        base.trim()
    };
    let base = base.trim_end_matches('/');
    if base.ends_with(path) {
        base.to_string()
    } else {
        format!("{base}{path}")
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    let total = s.chars().count();
    let mut out: String = s.chars().take(max).collect();
    if total > max {
        out.push('…');
    }
    out
}

/// 按行读取 HTTP 流（SSE / NDJSON 共用）。
pub async fn read_lines<F>(resp: reqwest::Response, mut on_line: F) -> Result<(), String>
where
    F: FnMut(&str),
{
    use futures_util::StreamExt;

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取 AI 流失败: {e}"))?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            on_line(line.trim_end_matches(['\r', '\n']));
        }
    }
    if !buf.is_empty() {
        let line = String::from_utf8_lossy(&buf);
        on_line(line.trim_end_matches(['\r', '\n']));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_scene() {
        let text = r##"好的，设计一个标题动画。
```json
{
  "name": "测试",
  "durationSec": 4,
  "width": 1280,
  "height": 720,
  "background": "#000000",
  "layers": [
    { "shape": { "kind": "text", "x": 640, "y": 360, "text": "Hi", "size": 72, "fill": "#fff", "align": "center", "baseline": "middle" }, "animations": [] }
  ]
}
```"##;
        let scene = extract_scene(text).expect("should parse");
        assert_eq!(scene.name, "测试");
        assert_eq!(scene.layers.len(), 1);
    }

    #[test]
    fn ignores_invalid_and_empty_scenes() {
        assert!(extract_scene("没有 JSON").is_none());
        assert!(extract_scene(r#"{"name":"x","layers":[]}"#).is_none());
    }

    #[test]
    fn parse_tool_args_normalizes_and_validates() {
        // 空参数（部分模型会输出空串）归一化为空对象
        assert_eq!(parse_tool_args("").unwrap(), serde_json::json!({}));
        assert_eq!(parse_tool_args("  \n").unwrap(), serde_json::json!({}));
        assert_eq!(
            parse_tool_args(r#"{"path":"scenes/a.json"}"#).unwrap(),
            serde_json::json!({ "path": "scenes/a.json" })
        );
        // 非法 JSON：报错且信息含解析原因，而不是静默回退空参数
        let err = parse_tool_args(r#"{"path": }"#).unwrap_err();
        assert!(err.contains("不是合法 JSON"), "错误信息: {err}");
        // 非 JSON 对象（数组/标量）同样拒绝
        assert!(parse_tool_args("[1,2]").is_err());
        assert!(parse_tool_args("42").is_err());
    }

    #[test]
    fn endpoint_accepts_base_or_full_path() {
        assert_eq!(
            endpoint("https://api.openai.com/v1/", "x", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint("", "https://api.openai.com/v1", "/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    /// 构造一条 assistant(tool_calls) 消息。
    fn assistant_with_calls(ids: &[&str]) -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(serde_json::Value::Array(
                ids.iter()
                    .map(|id| serde_json::json!({ "id": id, "type": "function" }))
                    .collect(),
            )),
            name: None,
            tool_call_id: None,
        }
    }

    /// 构造一条 tool 回复消息。
    fn tool_reply(id: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".into(),
            content: "ok".into(),
            tool_calls: None,
            name: None,
            tool_call_id: Some(id.into()),
        }
    }

    #[test]
    fn recent_history_never_splits_tool_pairing() {
        // 12 条普通消息 + 一组完整工具链（assistant + 2 条 tool 回复）。
        // 粗裁会恰好切在工具链开头，必须回退到工具链之前。
        let mut history: Vec<ChatMessage> = (0..12)
            .map(|i| ChatMessage::text("user", format!("u{i}")))
            .collect();
        history.push(assistant_with_calls(&["call_a", "call_b"]));
        history.push(tool_reply("call_a"));
        history.push(tool_reply("call_b"));

        let out = recent_history(&history);
        let roles: Vec<&str> = out.iter().map(|m| m.role.as_str()).collect();
        // 不应以 tool 回复开头
        assert_ne!(
            roles.first(),
            Some(&"tool"),
            "裁剪后以 tool 开头: {roles:?}"
        );
        // 若保留了 assistant(tool_calls)，其全部 tool 回复必须同时保留
        let declared: Vec<String> = out
            .iter()
            .flat_map(|m| {
                m.tool_calls
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|c| c.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        for m in &out {
            if m.role == "tool" {
                let id = m.tool_call_id.as_deref().unwrap_or_default();
                assert!(
                    declared.iter().any(|d| d == id),
                    "tool 回复 {id} 的上游 assistant 被裁掉: {roles:?}"
                );
            }
        }
        // 完整工具链要么整体在、要么整体不在
        let has_a = out
            .iter()
            .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_a"));
        let has_call = out.iter().any(|m| {
            m.role == "assistant"
                && m.tool_calls
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .is_some_and(|calls| {
                        calls
                            .iter()
                            .any(|c| c.get("id").and_then(|v| v.as_str()) == Some("call_a"))
                    })
        });
        assert_eq!(
            has_a, has_call,
            "assistant(tool_calls) 与 tool 回复被拆开: {roles:?}"
        );
    }

    #[test]
    fn recent_history_drops_orphan_chain_without_over_trim() {
        // 尾部是完整工具链 + 一条普通消息：正常裁剪不受影响，仍保留 12 条
        let mut history: Vec<ChatMessage> = (0..11)
            .map(|i| ChatMessage::text("user", format!("u{i}")))
            .collect();
        history.push(assistant_with_calls(&["call_x"]));
        history.push(tool_reply("call_x"));
        history.push(ChatMessage::text("user", "latest"));

        let out = recent_history(&history);
        assert!(out
            .iter()
            .any(|m| m.role == "assistant" && m.tool_calls.is_some()));
        assert!(out.iter().any(|m| m.role == "tool"));
        // 起点没有落在 tool 消息上 → 未过度回退
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn recent_history_trims_long_content() {
        let long = "测".repeat(20_000);
        let history = vec![ChatMessage::text("user", long)];
        let out = recent_history(&history);
        assert_eq!(out[0].content.chars().count(), 12_000);
    }
}
