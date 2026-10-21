//! Tauri 命令：前端唯一的后端入口。

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::{self, AiConfig, ChatMessage};
use crate::demo;
use crate::events::StreamEvent;
use crate::scene::{DrawCmd, Scene, SceneMeta};

/// 全局状态：当前场景。求值只读，AI 更新时短暂加锁。
pub struct AppState {
    scene: Mutex<Option<Scene>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scene: Mutex::new(Some(demo::default_scene())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 用新场景更新全局状态并推送 scene 事件（agent 工具与 chat 共用）。
pub fn push_scene(app: &AppHandle, scene: &Scene) -> Result<(), String> {
    let frame = scene.evaluate(0.0);
    let meta = scene.meta();
    {
        let state = app.state::<AppState>();
        let mut guard = state.scene.lock().unwrap();
        *guard = Some(scene.clone());
    }
    let _ = app.emit(
        "stream",
        StreamEvent::Scene {
            scene: meta,
            frame,
            time: 0.0,
        },
    );
    Ok(())
}

/// 返回当前场景元信息（启动时前端用它初始化预览区）。
#[tauri::command]
pub fn get_default_scene(state: State<'_, AppState>) -> SceneMeta {
    let guard = state.scene.lock().unwrap();
    match guard.as_ref() {
        Some(scene) => scene.meta(),
        None => demo::default_scene().meta(),
    }
}

/// 求值指定时刻的绘制指令。前端播放循环按帧调用。
#[tauri::command]
pub fn get_frame(state: State<'_, AppState>, time: f64) -> Vec<DrawCmd> {
    let guard = state.scene.lock().unwrap();
    guard
        .as_ref()
        .map(|scene| scene.evaluate(time))
        .unwrap_or_default()
}

/// 对话：agent 循环（工具调用 + 流式输出），事件实时推给前端。
#[tauri::command]
pub async fn chat(
    app: AppHandle,
    user_message: String,
    config: AiConfig,
    history: Vec<ChatMessage>,
) -> Result<(), String> {
    let result = if config.provider.trim().is_empty() || config.provider.trim() == "mock" {
        // Mock 保持单轮模板行为
        let sink = ai::Sink(app.clone());
        let (text, scene) = ai::mock::run(&sink, &user_message).await;
        let _ = push_scene(&app, &scene);
        Ok((text, true))
    } else {
        let sink = ai::Sink(app.clone());
        // Agent 循环上限 12 步，防止失控
        ai::run_agent(&app, &sink, &config, &user_message, &history, 12).await
    };

    match result {
        Ok((text, loaded_scene)) => {
            // 模型没走工具流时，尝试从最终文本直接提取内联场景
            if !loaded_scene {
                if let Some(scene) = ai::extract_scene(&text) {
                    let _ = push_scene(&app, &scene);
                }
            }
        }
        Err(message) => {
            let _ = app.emit("stream", StreamEvent::Error { message });
        }
    }

    let _ = app.emit("stream", StreamEvent::FilesChanged);
    let _ = app.emit("stream", StreamEvent::Done);
    Ok(())
}

/// 列出工作区文件。
#[tauri::command]
pub fn list_files(app: AppHandle, path: Option<String>) -> Result<Vec<crate::workspace::FileEntry>, String> {
    crate::workspace::list(&app, path.as_deref().unwrap_or(""))
}

/// 读取工作区文本文件。
#[tauri::command]
pub fn read_file(app: AppHandle, path: String) -> Result<String, String> {
    crate::workspace::read(&app, &path)
}

/// 写入工作区文本文件。
#[tauri::command]
pub fn write_file(app: AppHandle, path: String, content: String) -> Result<u64, String> {
    let n = crate::workspace::write(&app, &path, &content)?;
    let _ = app.emit("stream", StreamEvent::FilesChanged);
    Ok(n)
}

/// 删除工作区文件/目录。
#[tauri::command]
pub fn delete_file(app: AppHandle, path: String) -> Result<(), String> {
    crate::workspace::delete(&app, &path)?;
    let _ = app.emit("stream", StreamEvent::FilesChanged);
    Ok(())
}

/// 重命名/移动。
#[tauri::command]
pub fn rename_file(app: AppHandle, from: String, to: String) -> Result<(), String> {
    crate::workspace::rename(&app, &from, &to)?;
    let _ = app.emit("stream", StreamEvent::FilesChanged);
    Ok(())
}

/// 加载工作区场景 JSON 到预览。
#[tauri::command]
pub fn open_scene(app: AppHandle, path: String) -> Result<String, String> {
    let content = crate::workspace::read(&app, &path)?;
    let mut scene: Scene =
        serde_json::from_str(&content).map_err(|e| format!("场景 JSON 解析失败：{e}"))?;
    if scene.layers.is_empty() {
        return Err("场景没有任何图层".into());
    }
    scene.normalize();
    let name = scene.name.clone();
    push_scene(&app, &scene)?;
    Ok(name)
}

/// 执行工作区 JS 脚本生成场景并加载预览。
#[tauri::command]
pub fn run_script(app: AppHandle, path: String) -> Result<String, String> {
    let code = crate::workspace::read(&app, &path)?;
    let scene = crate::script::run(&code)?;
    let name = scene.name.clone();
    push_scene(&app, &scene)?;
    Ok(name)
}

/// 工作区根目录绝对路径（前端展示用）。
#[tauri::command]
pub fn workspace_root(app: AppHandle) -> String {
    crate::workspace::root(&app).to_string_lossy().to_string()
}

/// 保存前端录制的 WebM 到系统下载目录，返回绝对路径。
#[tauri::command]
pub fn save_export(
    app: AppHandle,
    filename: String,
    data_base64: String,
) -> Result<String, String> {
    use base64::Engine;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("导出数据解码失败：{e}"))?;
    if bytes.is_empty() {
        return Err("导出数据为空。".into());
    }

    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("无法定位下载目录：{e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败：{e}"))?;

    let safe: String = filename
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = safe.trim().trim_end_matches(".webm");
    let stem = if stem.is_empty() { "motion" } else { stem };

    let mut path = dir.join(format!("{stem}.webm"));
    let mut index = 1;
    while path.exists() {
        path = dir.join(format!("{stem} ({index}).webm"));
        index += 1;
        if index > 999 {
            break;
        }
    }

    std::fs::write(&path, bytes).map_err(|e| format!("写入视频文件失败：{e}"))?;
    Ok(path.to_string_lossy().to_string())
}
