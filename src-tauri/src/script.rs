//! JS 动画脚本执行器。
//!
//! 优先使用系统 Node.js（`--permission` 沙箱模式），其次尝试其他引擎；
//! 都不可用时返回友好错误。脚本通过 `exportScene(scene)` 导出场景对象，
//! JSON 序列化后按 [`crate::scene::Scene`] 反序列化。
//!
//! 沙箱措施：`node --permission` 禁止文件系统/子进程/原生模块访问，
//! 超时 3 秒，输入输出只走 stdin/stdout JSON。

use std::fs;
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
           align: o.align, baseline: o.baseline, rotation: o.rotation,
           stroke: o.stroke, strokeWidth: o.strokeWidth };
}
function rect(x, y, w, h, o = {}) {
  return { kind: "rect", x, y, w, h, fill: o.fill ?? "#ffffff", radius: o.radius,
           opacity: o.opacity ?? 1, rotation: o.rotation,
           stroke: o.stroke, strokeWidth: o.strokeWidth };
}
function circle(x, y, r, o = {}) {
  return { kind: "circle", x, y, r, fill: o.fill ?? "#ffffff", opacity: o.opacity ?? 1,
           rotation: o.rotation, stroke: o.stroke, strokeWidth: o.strokeWidth };
}
function ellipse(x, y, rx, ry, o = {}) {
  return { kind: "ellipse", x, y, rx, ry, fill: o.fill ?? "#ffffff",
           opacity: o.opacity ?? 1, rotation: o.rotation,
           stroke: o.stroke, strokeWidth: o.strokeWidth };
}
function line(x1, y1, x2, y2, o = {}) {
  return { kind: "line", x1, y1, x2, y2, stroke: o.stroke ?? "#ffffff",
           width: o.width ?? 2, opacity: o.opacity ?? 1 };
}
function path(d, o = {}) {
  return { kind: "path", d, fill: o.fill ?? "#ffffff", opacity: o.opacity ?? 1, rotation: o.rotation,
           stroke: o.stroke, strokeWidth: o.strokeWidth };
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

    // stdin 写入放独立线程：管道缓冲区写满时不会阻塞主线程的超时轮询
    let script = full.into_bytes();
    let mut child_stdin = child.stdin.take();
    let stdin_thread = std::thread::spawn(move || {
        if let Some(mut stdin) = child_stdin.take() {
            let _ = stdin.write_all(&script);
            // drop(stdin) 触发 EOF
        }
    });

    let out = wait_with_timeout(child, TIMEOUT)?;

    let _ = stdin_thread.join();

    let line = out
        .stdout
        .lines()
        .find(|l| l.trim_start().starts_with(SENTINEL))
        .ok_or_else(|| {
            // stdout 没有场景：优先报 stderr 里的真实错误（异常堆栈等），
            // 其次按退出情况给出可行动的提示。console.error 调试输出不再
            // 抢占成功结果——只要导出了场景就照常解析。
            let stderr = out.stderr.trim();
            if !stderr.is_empty() {
                return truncate_err(stderr);
            }
            "脚本没有调用 exportScene()，未产生场景。".to_string()
        })?;

    let json = line.trim_start()[SENTINEL.len()..].trim();
    let mut scene: Scene =
        serde_json::from_str(json).map_err(|e| format!("脚本导出的场景不合法：{e}"))?;
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
                "qjs" => Engine {
                    program: "qjs",
                    args: vec![],
                },
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

/// 判断可执行文件是否在 PATH 中（跨平台）。
///
/// 不再调用外部 `which`（Windows 没有）：
/// 按 PATH 逐目录查找，Windows 额外尝试 PATHEXT 里声明的可执行后缀。
fn which(prog: &str) -> bool {
    // 约定俗成：含路径分隔符时直接按字面路径检查
    if prog.contains('/') || prog.contains('\\') {
        return is_executable_file(std::path::Path::new(prog));
    }
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let exts: Vec<String> = if cfg!(windows) {
        match std::env::var("PATHEXT") {
            Ok(v) => v
                .split(';')
                .filter(|e| !e.is_empty())
                .map(|e| e.to_ascii_lowercase())
                .collect(),
            Err(_) => vec![".exe".into(), ".cmd".into(), ".bat".into()],
        }
    } else {
        vec![String::new()]
    };
    let prog_lower = prog.to_ascii_lowercase();
    for dir in std::env::split_paths(&path_env) {
        for ext in &exts {
            // Windows 上「node.exe」也能命中 PATH 里的 node；类 Unix 忽略显式后缀
            let candidate = if ext.is_empty() {
                dir.join(prog)
            } else {
                dir.join(format!("{prog}{ext}"))
            };
            if is_executable_file(&candidate) {
                return true;
            }
            // Windows：不带后缀的精确文件也算（罕见但合法）
            if cfg!(windows) && ext != &".exe" && prog_lower.ends_with(ext.as_str()) {
                let bare = dir.join(prog);
                if is_executable_file(&bare) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_executable_file(path: &std::path::Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    // 类 Unix 还要求可执行位
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

struct ProcOut {
    stdout: String,
    stderr: String,
}

fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<ProcOut, String> {
    use std::io::Read;

    // stdout/stderr 各自放独立线程持续读取：脚本输出超过管道缓冲区（约 64KB）
    // 时，若不消费管道，子进程会阻塞在 write 上；而主线程又在等它退出，互相死锁，
    // 最后只能误报「执行超时」。读线程保证管道始终被排空。
    let mut stdout_handle = child.stdout.take();
    let mut stderr_handle = child.stderr.take();
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(s) = stdout_handle.as_mut() {
            let _ = s.read_to_string(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(s) = stderr_handle.as_mut() {
            let _ = s.read_to_string(&mut buf);
        }
        buf
    });

    let start = std::time::Instant::now();
    let status = loop {
        match child
            .try_wait()
            .map_err(|e| format!("等待脚本退出失败：{e}"))?
        {
            Some(status) => break status,
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    // 回收子进程避免僵尸；读线程随管道关闭自行结束
                    let _ = child.wait();
                    return Err(format!("脚本执行超过 {}s，已中止", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();

    if !status.success() && stderr.trim().is_empty() {
        return Err(format!("脚本异常退出（{status}）"));
    }
    Ok(ProcOut { stdout, stderr })
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
    use crate::scene::Shape;

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

    #[test]
    fn stderr_noise_does_not_override_successful_export() {
        // AI 写脚本时常用 console.error/console.warn 调试；只要场景导出成功，
        // stderr 里的噪声不应把执行误判为失败（曾因「stderr 非空即失败」
        // 把含调试输出的成功脚本整个丢弃）。
        let code = r##"
            console.warn("警告：这只是调试输出");
            console.error("错误：这也只是调试输出");
            const s = scene("带噪声", 2);
            addLayer(text("你好", 10, 10, {}), []);
            exportScene(s);
        "##;
        match run(code) {
            Ok(scene) => assert_eq!(scene.name, "带噪声"),
            Err(e) => assert!(e.contains("JS 引擎"), "非引擎错误: {e}"),
        }
    }

    #[test]
    fn runtime_error_reports_stderr_not_missing_sentinel() {
        // 失败时应上报 stderr 的真实异常，而不是笼统的「没有调用 exportScene」
        let code = "console.error('boom: 坐标计算失败'); null.crash();";
        match run(code) {
            Ok(_) => panic!("应当失败"),
            Err(e) => {
                assert!(
                    e.contains("boom") || e.contains("JS 引擎"),
                    "错误信息应含 stderr 内容: {e}"
                );
                assert!(!e.contains("没有调用 exportScene"), "不应误报缺少导出: {e}");
            }
        }
    }

    #[test]
    fn handles_large_stdout_without_deadlock() {
        // 管道缓冲区约 64KB；脚本输出远超它时应正常完成而不是误报超时
        let code = r##"
            for (let i = 0; i < 2000; i++) console.log("x".repeat(200));
            const s = scene("大输出", 2);
            addLayer(circle(10, 10, 5, { stroke: "#fff", strokeWidth: 3 }), []);
            exportScene(s);
        "##;
        match run(code) {
            Ok(scene) => {
                assert_eq!(scene.name, "大输出");
                match &scene.layers[0].shape {
                    Shape::Circle {
                        stroke,
                        stroke_width,
                        ..
                    } => {
                        assert_eq!(stroke.as_deref(), Some("#fff"));
                        assert_eq!(*stroke_width, Some(3.0));
                    }
                    other => panic!("应为 circle：{other:?}"),
                }
            }
            Err(e) => assert!(e.contains("JS 引擎"), "非引擎错误: {e}"),
        }
    }

    #[test]
    fn which_scans_path_without_external_binary() {
        // 回归：which() 必须自行扫描 PATH，而不是调用外部 which 命令——
        // Windows 上没有 which，旧实现必然失败，JS 引擎会永远找不到。
        // 类 Unix 测试环境里 sh/bash 一定在 PATH 中。
        assert!(
            which("sh") || which("bash"),
            "应在 PATH 中找到 sh 或 bash"
        );
        // 含路径分隔符时按字面路径检查
        assert!(which("/bin/sh") || which("/usr/bin/sh"), "字面路径应可直接命中");
        // 不存在的程序返回 false，不 panic
        assert!(!which("lonicera-definitely-not-a-real-binary-xyz"));
        // 目录不是可执行文件，不能命中
        assert!(!which("/tmp"));
    }
}
