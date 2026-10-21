//! 主进程 -> 前端的事件协议（前端 `StreamEvent` 类型与之对应）。

use serde::Serialize;

use crate::scene::{DrawCmd, SceneMeta};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamEvent {
    /// 流式文本增量（AI 回复）。
    Delta { text: String },
    /// Agent 正在调用工具。
    Tool { name: String, detail: String },
    /// 工作区文件发生变化（供前端刷新文件树）。
    FilesChanged,
    /// 场景已更新，附带 0 时刻的帧，便于前端立即显示。
    Scene {
        scene: SceneMeta,
        frame: Vec<DrawCmd>,
        time: f64,
    },
    /// 本轮对话结束。
    Done,
    /// 可展示给用户的错误。
    Error { message: String },
}
