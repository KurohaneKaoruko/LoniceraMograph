//! JS 动画脚本执行器。
//!
//! 优先使用系统 Node.js（`--permission` 沙箱模式），其次尝试其他引擎；
//! 都不可用时返回友好错误。脚本通过 `exportScene(scene)` 导出场景对象，
//! JSON 序列化后按 [`crate::scene::Scene`] 反序列化。
//!
//! 沙箱措施：`node --permission` 禁止文件系统/子进程/原生模块访问，
//! 超时 3 秒，输入输出只走 stdin/stdout JSON。

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::scene::Scene;

/// 注入到脚本前部的运行时胶水：提供 scene/text/rect/… 辅助函数。
const PRELUDE: &str = r##"
"use strict";
const easing = {
  linear: "linear", easeIn: "easeIn", easeOut: "easeOut", easeInOut: "easeInOut",
  easeInCubic: "easeInCubic", easeOutCubic: "easeOutCubic", easeInOutCubic: "easeInOutCubic",
  easeOutBack: "easeOutBack", easeOutBounce: "easeOutBounce",
};
let __scene = null;
function scene(name, durationSec) {
  __scene = {
    name: String(name ?? "未命名场景"),
    durationSec: Number(durationSec ?? 6),
    width: 1280, height: 720,
    background: "#0e1017",
    layers: [],
  };
  return __scene;
}
function anim(prop, from, to, start, end, easingName) {
  return { prop, from, to, start, end, easing: easingName ?? "linear" };
}
function text(str, x, y, o = {}) {
  return { kind: "text", x, y, text: String(str), size: o.size ?? 48,
           fill: o.fill ?? "#ffffff", opacity: o.opacity ?? 1,
           align: o.align, baseline: o.baseline };
}
function rect(x, y, w, h, o = {}) {
  return { kind: "rect", x, y, w, h, fill: o.fill ?? "#ffffff", radius: o.radius,
           opacity: o.opacity ?? 1, rotation: o.rotation };
}
function circle(x, y, r, o = {}) {
  return { kind: "circle", x, y, r, fill: o.fill ?? "#ffffff", opacity: o.opacity ?? 1 };
}
function ellipse(x, y, rx, ry, o = {}) {
  return { kind: "ellipse", x, y, rx, ry, fill: o.fill ?? "#ffffff",
           opacity: o.opacity ?? 1, rotation: o.rotation };
}
function line(x1, y1, x2, y2, o = {}) {
  return { kind: "line", x1, y1, x2, y2, stroke: o.stroke ?? "#ffffff",
           width: o.width ?? 2, opacity: o.opacity ?? 1 };
}
function path(d, o = {}) {
  return { kind: "path", d, fill: o.fill ?? "#ffffff", opacity: o.opacity ?? 1, rotation: o.rotation };
}
function addLayer(shape, animations = []) {
  if (!__scene) throw new Error("请先调用 scene(name, durationSec) 创建场景");
  __scene.layers.push({ shape, animations });
}
let __exported = false;
function exportScene(s) {
  if (s && Array.isArray(s.layers)) __scene = s;
  if (!__scene) throw new Error("没有可导出的场景");
  __exported = true;
  console.log("__LONICERA_SCENE__" + JSON.stringify(__scene));
}
"##;

const SENTINEL: &str = "__LONICERA_SCENE__";
const TIMEOUT: Duration = Duration::from_secs(3);

/// 在沙箱中执行脚本，返回导出的场景。
pub fn run(code: &str) -> Result<Scene, String> {
    let full = format!("{PRELUDE}\n{code}\nif (!__exported) exportScene(__scene);\n");
    let engine = find_engine().ok_or_else(|| {
        "未找到可用的 JS 引擎（node/qjs/deno/bun）。请安装 Node.js 后重试。".to_string()
    })?;

    let mut child = Command::new(&engine.program)
        .args(&engine.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 {} 失败：{e}", engine.program))?;

    // 单独持有 stdin 便于 drop 触发 EOF
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(full.as_bytes())
            .map_err(|e| format!("写入脚本失败：{e}"))?;
    }

    let out = wait_with_timeout(child, TIMEOUT)?;

    if !out.stderr.trim().is_empty() {
        return Err(truncate_err(&out.stderr));
    }

    let line = out
        .stdout
        .lines()
        .find(|l| l.trim_start().starts_with(SENTINEL))
        .ok_or_else(|| "脚本没有调用 exportScene()，未产生场景。".to_string())?;

    let json = line.trim_start()[SENTINEL.len()..].trim();
    let mut scene: Scene = serde_json::from_str(json)
        .map_err(|e| format!("脚本导出的场景不合法：{e}"))?;
    if scene.layers.is_empty() {
        return Err("脚本生成的场景没有任何图层".into());
    }
    scene.normalize();
    Ok(scene)
}

struct Engine {
    program: &'static str,
    args: Vec<&'static str>,
}

fn find_engine() -> Option<Engine> {
    for prog in ["node", "qjs", "bun", "deno"] {
        if which(prog) {
            return Some(match prog {
                "node" => Engine {
                    program: "node",
                    // --permission: 禁文件系统/子进程；--no-warnings 保持输出干净
                    args: vec!["--permission", "--no-warnings"],
                },
                "qjs" => Engine { program: "qjs", args: vec![] },
                "bun" => Engine {
                    program: "bun",
                    args: vec!["run", "-"],
                },
                _ => Engine {
                    program: "deno",
                    args: vec!["run", "--no-prompt", "--no-config", "-"],
                },
            });
        }
    }
    None
}

fn which(prog: &str) -> bool {
    Command::new("which")
        .arg(prog)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

struct ProcOut {
    stdout: String,
    stderr: String,
}

fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<ProcOut, String> {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().map_err(|e| format!("等待脚本退出失败：{e}"))? {
            Some(status) => {
                use std::io::Read;
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_string(&mut stdout);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut stderr);
                }
                if !status.success() && stderr.trim().is_empty() {
                    return Err(format!("脚本异常退出（{status}）"));
                }
                return Ok(ProcOut { stdout, stderr });
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(format!("脚本执行超过 {}s，已中止", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn truncate_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 500 {
        format!("{}…", s.chars().take(500).collect::<String>())
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_simple_script() {
        let code = r##"
            const s = scene("测试", 3);
            const t = text("你好", 640, 340, { size: 80, fill: "#ffffff", align: "center", baseline: "middle" });
            addLayer(t, [anim("opacity", 0, 1, 0.2, 1.0, easing.easeOutCubic)]);
            exportScene(s);
        "##;
        // 测试环境可能有 node 也可能没有；没有时应得到明确错误而不是 panic
        match run(code) {
            Ok(scene) => {
                assert_eq!(scene.name, "测试");
                assert_eq!(scene.layers.len(), 1);
            }
            Err(e) => assert!(e.contains("JS 引擎"), "非引擎错误: {e}"),
        }
    }

    #[test]
    fn rejects_runtime_error() {
        let code = "const s = scene('x', 1); null.crash(); exportScene(s);";
        match run(code) {
            Ok(_) => panic!("应当失败"),
            Err(e) => assert!(!e.is_empty()),
        }
    }
}
