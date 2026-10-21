//! 离线 Mock Provider：不联网，按关键词生成演示场景并模拟流式输出。

use std::time::Duration;

use super::Sink;
use crate::demo;
use crate::scene::Scene;

pub async fn run(sink: &Sink, prompt: &str) -> (String, Scene) {
    let scene = demo::scene_from_prompt(prompt);
    let text = demo::mock_explanation(&scene, prompt);

    // 模拟逐字流式输出，让交互手感与真实模型一致。
    let mut buffer = String::new();
    for ch in text.chars() {
        buffer.push(ch);
        if buffer.chars().count() >= 6 || ch == '\n' {
            sink.delta(&buffer);
            buffer.clear();
            tokio::time::sleep(Duration::from_millis(18)).await;
        }
    }
    if !buffer.is_empty() {
        sink.delta(&buffer);
    }

    (text, scene)
}
