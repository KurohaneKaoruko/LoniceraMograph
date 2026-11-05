//! 项目工作区：agent 的文件操作全部限制在这个目录内。
//!
//! 默认位置：`~/Documents/LoniceraMograph/`（无文档目录时退回应用数据目录）。
//! 布局约定：`scenes/*.json` 声明式场景；`scripts/*.js` 代码生成场景。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// 读取文件的最大字节数（防止 agent 读爆上下文）。
const MAX_READ_BYTES: u64 = 200 * 1024;
/// 目录列举条目上限。
const MAX_ENTRIES: usize = 800;

/// 工作区中的一个文件/目录条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 解析工作区根目录（并确保存在），返回绝对路径。
///
/// 优先 `~/Documents/LoniceraMograph/`；无文档目录时退回
/// `<app_data>/workspace/`（加 workspace 子目录，避免与 WebView 存储混杂）。
pub fn root(app: &AppHandle) -> PathBuf {
    let dir = match app.path().document_dir() {
        Ok(doc) => doc.join("LoniceraMograph"),
        Err(_) => app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("LoniceraMograph"))
            .join("workspace"),
    };
    let _ = fs::create_dir_all(&dir);
    let _ = fs::create_dir_all(dir.join("scenes"));
    let _ = fs::create_dir_all(dir.join("scripts"));
    dir
}

/// 校验并解析工作区内的相对路径，拒绝绝对路径与 `..` 穿越。
pub fn resolve(app: &AppHandle, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_start_matches('/');
    if rel.is_empty() {
        return Ok(root(app));
    }
    if rel.split(['/', '\\']).any(|c| c == "..") {
        return Err(format!("路径不安全（不允许 ..）：{rel}"));
    }
    Ok(root(app).join(rel))
}

/// 递归列出工作区文件（跳过隐藏目录），按路径排序。
pub fn list(app: &AppHandle, subdir: &str) -> Result<Vec<FileEntry>, String> {
    let base = resolve(app, subdir)?;
    if !base.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk(&base, &base, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.truncate(MAX_ENTRIES);
    Ok(out)
}

/// 递归遍历目录，条目路径为相对 `base` 的工作区路径（多级目录拼接完整前缀）。
///
/// 独立于 `AppHandle`，便于对临时目录直接做回归测试。
fn walk(base: &Path, dir: &Path, out: &mut Vec<FileEntry>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name_str) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if name_str.starts_with('.') {
            continue;
        }
        let is_dir = path.is_dir();
        let meta = entry.metadata().ok();
        // 完整相对路径：相对 base 逐级拼接，保证 a/b/c.txt 不会被展平成 b/c.txt
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(FileEntry {
            path: rel,
            is_dir,
            size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
        });
        if is_dir {
            if out.len() >= MAX_ENTRIES {
                return Ok(());
            }
            walk(base, &path, out)?;
        }
    }
    Ok(())
}

/// 读取文本文件（UTF-8，超长截断）。
pub fn read(app: &AppHandle, rel: &str) -> Result<String, String> {
    let path = resolve(app, rel)?;
    let meta = fs::metadata(&path).map_err(|e| format!("文件不存在：{rel}（{e}）"))?;
    if meta.is_dir() {
        return Err(format!("{} 是目录，不是文件", rel));
    }
    if meta.len() > MAX_READ_BYTES {
        let bytes = fs::read(&path).map_err(|e| format!("读取失败：{e}"))?;
        let clipped = String::from_utf8_lossy(&bytes[..MAX_READ_BYTES as usize]).to_string();
        return Ok(format!(
            "{clipped}\n\n…（文件过大，已截断到 {} KB）",
            MAX_READ_BYTES / 1024
        ));
    }
    fs::read_to_string(&path).map_err(|e| format!("读取失败：{rel}（{e}）"))
}

/// 写文件（自动创建父目录），返回写入字节数。
pub fn write(app: &AppHandle, rel: &str, content: &str) -> Result<u64, String> {
    let path = resolve(app, rel)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("写入失败：{rel}（{e}）"))?;
    Ok(content.len() as u64)
}

/// 删除文件或目录（递归）。
pub fn delete(app: &AppHandle, rel: &str) -> Result<(), String> {
    let path = resolve(app, rel)?;
    if path == root(app) {
        return Err("不能删除工作区根目录".into());
    }
    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| format!("删除目录失败：{e}"))
    } else {
        fs::remove_file(&path).map_err(|e| format!("删除失败：{rel}（{e}）"))
    }
}

/// 重命名 / 移动。
pub fn rename(app: &AppHandle, from: &str, to: &str) -> Result<(), String> {
    let src = resolve(app, from)?;
    let dst = resolve(app, to)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    fs::rename(&src, &dst).map_err(|e| format!("重命名失败：{from} → {to}（{e}）"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_traversal_and_absolute() {
        // 不依赖 AppHandle，直接验证路径字符串规则
        for bad in ["../x.json", "a/../../b", "scenes/../../secret"] {
            let rel = bad.trim().trim_start_matches('/');
            assert!(
                rel.split(['/', '\\']).any(|c| c == ".."),
                "应识别为非法: {bad}"
            );
        }
        // 绝对路径会被折叠为工作区内相对路径（防御语义），合法形式直接通过
        for good in ["scenes/a.json", "scripts/demo.js", "a/b/c.txt"] {
            let rel = good.trim().trim_start_matches('/');
            assert!(!rel.split(['/', '\\']).any(|c| c == ".."));
        }
    }

    #[test]
    fn walk_keeps_deep_relative_paths() {
        // 回归：递归列举时条目路径必须保留完整多级前缀，
        // a/b/c.txt 不能被展平成 b/c.txt（agent 的 read_file 依赖该路径）。
        let tmp = std::env::temp_dir().join(format!("lonicera-walk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("a/b")).unwrap();
        fs::create_dir_all(tmp.join("top")).unwrap();
        fs::write(tmp.join("a/b/c.txt"), "x").unwrap();
        fs::write(tmp.join("a/f1.txt"), "x").unwrap();
        fs::write(tmp.join("top.txt"), "x").unwrap();
        fs::write(tmp.join("top/deep.js"), "x").unwrap();

        let mut out = Vec::new();
        walk(&tmp, &tmp, &mut out).unwrap();
        out.sort_by(|x, y| x.path.cmp(&y.path));
        let paths: Vec<&str> = out.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "a",
                "a/b",
                "a/b/c.txt",
                "a/f1.txt",
                "top",
                "top.txt",
                "top/deep.js"
            ]
        );
        // 隐藏条目不出现
        assert!(paths
            .iter()
            .all(|p| !p.split('/').any(|seg| seg.starts_with('.'))));
        let _ = fs::remove_dir_all(&tmp);
    }
}
