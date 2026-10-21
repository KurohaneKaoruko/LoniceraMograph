//! Agent 工具集：LLM 可调用的文件操控与场景操作。
//!
//! 每个工具 = 名称 + 参数 schema（给 LLM 的说明）+ 执行函数。
//! 执行结果以文本形式回传给 LLM，形成多步推理循环。

use serde_json::{json, Value};

use crate::scene::Scene;
use crate::workspace;

/// 工具执行所需的上下文。
pub struct ToolCtx<'a> {
    pub app: &'a tauri::AppHandle,
    /// agent 循环中最近一次通过 load_scene 加载的场景（供 get_scene 读取）。
    pub current_scene: std::sync::Mutex<Option<Scene>>,
}

/// 工具的 LLM 侧描述（OpenAI function 格式，Anthropic 亦兼容）。
pub fn definitions() -> Value {
    json!([
        {
            "name": "list_files",
            "description": "列出项目工作区中的文件。工作区分为 scenes/（声明式场景 JSON）与 scripts/（JS 代码生成脚本）。开始工作前先用它了解现有文件。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "子目录，如 scenes 或 scripts；缺省列出全部" }
                }
            }
        },
        {
            "name": "read_file",
            "description": "读取工作区中一个文本文件的内容（场景 JSON 或 JS 脚本）。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "工作区相对路径，如 scenes/intro.json" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "write_file",
            "description": "写入文本文件到工作区（自动创建目录）。保存场景 JSON 到 scenes/*.json，或 JS 动画脚本到 scripts/*.js。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "工作区相对路径" },
                    "content": { "type": "string", "description": "完整文件内容" }
                },
                "required": ["path", "content"]
            }
        },
        {
            "name": "delete_file",
            "description": "删除工作区中的文件或目录。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "rename_file",
            "description": "重命名或移动工作区中的文件。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" }
                },
                "required": ["from", "to"]
            }
        },
        {
            "name": "load_scene",
            "description": "把一个场景 JSON 加载到预览画布并自动播放。文件内容必须是合法的场景 JSON（name/durationSec/width/height/background/layers）。加载后可用 get_scene_errors 检查问题。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "scenes/ 下的场景文件路径" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "compile_script",
            "description": "执行 scripts/ 下的 JS 动画脚本生成场景并加载预览。脚本运行在受限沙箱中，通过 exportScene(scene) 导出场景对象。脚本示例见 write_file 说明。",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "scripts/ 下的 JS 文件路径" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "get_scene_errors",
            "description": "检查当前已加载场景的问题：空图层、越界坐标、无效颜色、重叠动画等。修改场景后应调用它自查。",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_scene",
            "description": "获取当前加载场景的 JSON（agent 本轮会话中最近一次 load/compile 的场景）。",
            "input_schema": { "type": "object", "properties": {} }
        }
    ])
}

/// 给 LLM 的脚本编写指南（拼进系统提示词）。
pub const SCRIPT_GUIDE: &str = r##"【JS 脚本生成动画】scripts/*.js 运行在受限沙箱（无网络/文件/DOM），可用全局辅助函数：
- scene(name, durationSec) 创建场景
- addLayer(shape, animations) 添加图层；shape 用 { kind, ... } 描述
- text(str, x, y, opts) / rect(x,y,w,h,opts) / circle(x,y,r,opts) / ellipse / line / path 快捷构造图元
- anim(prop, from, to, start, end, easing) 构造补间
- easing.easeOutCubic 等为预置缓动
- 最后 exportScene(scene) 导出。整体必须在 300ms 内完成。
示例：
  const s = scene("演示", 5);
  const t = text("你好", 640, 340, { size: 96, fill: "#fff", align: "center", baseline: "middle" });
  addLayer(t, [anim("opacity", 0, 1, 0.2, 1.2, easing.easeOutCubic), anim("y", 400, 340, 0.2, 1.2, easing.easeOutCubic)]);
  exportScene(s);
"##;

/// 执行一个工具调用，返回给 LLM 的文本结果。
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Result<String, String> {
    let app = ctx.app;
    let s = |v: Option<&str>| v.unwrap_or("").trim().to_string();

    match name {
        "list_files" => {
            let subdir = s(args.get("path").and_then(|v| v.as_str()));
            let entries = workspace::list(app, &subdir)?;
            if entries.is_empty() {
                return Ok("（空）".into());
            }
            let mut out = String::from("path\ttype\tbytes\n");
            for e in entries {
                out.push_str(&format!(
                    "{}\t{}\t{}\n",
                    e.path,
                    if e.is_dir { "dir" } else { "file" },
                    e.size
                ));
            }
            Ok(out)
        }

        "read_file" => {
            let path = s(args.get("path").and_then(|v| v.as_str()));
            if path.is_empty() {
                return Err("read_file: 缺少 path".into());
            }
            workspace::read(app, &path)
        }

        "write_file" => {
            let path = s(args.get("path").and_then(|v| v.as_str()));
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if path.is_empty() {
                return Err("write_file: 缺少 path".into());
            }
            let bytes = workspace::write(app, &path, content)?;
            // 写场景文件后顺手校验，让 agent 立即知道 JSON 是否合法
            let validation = if path.ends_with(".json") {
                match serde_json::from_str::<Scene>(content) {
                    Ok(mut scene) => {
                        scene.normalize();
                        format!("；场景 JSON 校验通过（{} 个图层）", scene.layers.len())
                    }
                    Err(e) => format!("；⚠ 场景 JSON 校验失败：{e}"),
                }
            } else {
                String::new()
            };
            Ok(format!("已写入 {path}（{bytes} 字节）{validation}"))
        }

        "delete_file" => {
            let path = s(args.get("path").and_then(|v| v.as_str()));
            if path.is_empty() {
                return Err("delete_file: 缺少 path".into());
            }
            workspace::delete(app, &path)?;
            Ok(format!("已删除 {path}"))
        }

        "rename_file" => {
            let from = s(args.get("from").and_then(|v| v.as_str()));
            let to = s(args.get("to").and_then(|v| v.as_str()));
            if from.is_empty() || to.is_empty() {
                return Err("rename_file: 需要 from 与 to".into());
            }
            workspace::rename(app, &from, &to)?;
            Ok(format!("已重命名 {from} → {to}"))
        }

        "load_scene" => {
            let path = s(args.get("path").and_then(|v| v.as_str()));
            if path.is_empty() {
                return Err("load_scene: 缺少 path".into());
            }
            let content = workspace::read(app, &path)?;
            let mut scene: Scene = serde_json::from_str(&content)
                .map_err(|e| format!("场景 JSON 解析失败：{e}"))?;
            if scene.layers.is_empty() {
                return Err("场景没有任何图层".into());
            }
            scene.normalize();
            crate::commands::push_scene(app, &scene)?;
            *ctx.current_scene.lock().unwrap() = Some(scene.clone());
            Ok(format!(
                "已加载「{}」（{}s，{} 图层）到预览",
                scene.name, scene.duration_sec, scene.layers.len()
            ))
        }

        "compile_script" => {
            let path = s(args.get("path").and_then(|v| v.as_str()));
            if path.is_empty() {
                return Err("compile_script: 缺少 path".into());
            }
            let code = workspace::read(app, &path)?;
            let scene = crate::script::run(&code)
                .map_err(|e| format!("脚本执行失败：{e}"))?;
            let layer_count = scene.layers.len();
            crate::commands::push_scene(app, &scene)?;
            *ctx.current_scene.lock().unwrap() = Some(scene.clone());
            Ok(format!(
                "脚本已执行，「{}」生成成功（{}s，{layer_count} 图层），已加载到预览",
                scene.name, scene.duration_sec
            ))
        }

        "get_scene" => match ctx.current_scene.lock().unwrap().clone() {
            Some(scene) => serde_json::to_string_pretty(&scene)
                .map_err(|e| format!("序列化失败：{e}")),
            None => Ok("（当前没有已加载的场景，请先 load_scene 或 compile_script）".into()),
        },

        "get_scene_errors" => {
            let scene = ctx.current_scene.lock().unwrap().clone();
            let Some(scene) = scene else {
                return Ok("（当前没有已加载的场景）".into());
            };
            Ok(check_scene_errors(&scene))
        }

        other => Err(format!("未知工具：{other}")),
    }
}

/// 场景体检：返回给 LLM 的问题清单（或"未发现问题"）。
fn check_scene_errors(scene: &Scene) -> String {
    let mut problems: Vec<String> = Vec::new();

    for (i, layer) in scene.layers.iter().enumerate() {
        let label = |what: &str| format!("图层 {i}（{}）{what}", layer.shape.kind_name());

        // 可动画属性静态检查
        for anim in &layer.animations {
            if layer.shape.get_prop(&anim.prop).is_none() {
                problems.push(format!(
                    "图层 {i}（{}）动画属性「{}」不受支持，将被忽略",
                    layer.shape.kind_name(),
                    anim.prop
                ));
            }
        }

        match &layer.shape {
            crate::scene::Shape::Rect { x, y, w, h, .. } => {
                if *w < 1.0 || *h < 1.0 {
                    problems.push(label("尺寸过小"));
                }
                if *x > scene.width || *y > scene.height || *x + *w < 0.0 || *y + *h < 0.0 {
                    problems.push(label("完全在画布外"));
                }
            }
            crate::scene::Shape::Circle { x, y, r, .. } => {
                if *r < 0.5 {
                    problems.push(label("半径过小"));
                }
                if *x + *r < 0.0 || *x - *r > scene.width || *y + *r < 0.0 || *y - *r > scene.height
                {
                    problems.push(label("完全在画布外"));
                }
            }
            crate::scene::Shape::Text { text, size, .. } => {
                if text.trim().is_empty() {
                    problems.push(label("文本为空"));
                }
                if *size < 8.0 {
                    problems.push(label("字号过小，观众看不清"));
                }
            }
            _ => {}
        }

        if layer.animations.len() > 12 {
            problems.push(label("动画数量过多（>12），可能难以维护"));
        }
    }

    if scene.layers.is_empty() {
        problems.push("场景没有任何图层".into());
    }
    if scene.duration_sec < 1.0 {
        problems.push("时长小于 1 秒，观众几乎无法看清".into());
    }

    if problems.is_empty() {
        "场景检查通过，未发现问题。".into()
    } else {
        format!("发现 {} 个问题：\n- {}", problems.len(), problems.join("\n- "))
    }
}
