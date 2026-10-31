//! 内置示例场景 + 离线 Mock 生成器。
//!
//! Mock 生成器让应用在没有 API Key 的情况下也能完整体验：
//! 从用户描述里提取标题、时长、配色倾向，拼出一个节奏不错的标题动画。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::scene::{Animation, Easing, Layer, Scene, Shape};

fn anim(prop: &str, from: f64, to: f64, start: f64, end: f64, easing: Easing) -> Animation {
    Animation {
        prop: prop.into(),
        from: Some(from),
        to: Some(to),
        start: Some(start),
        end: Some(end),
        easing,
    }
}

fn animated(shape: Shape, animations: Vec<Animation>) -> Layer {
    Layer {
        shape,
        animations,
    }
}

fn text(x: f64, y: f64, content: &str, size: f64, fill: &str) -> Shape {
    Shape::Text {
        x,
        y,
        text: content.to_string(),
        size,
        fill: fill.into(),
        opacity: Some(1.0),
        align: Some("center".into()),
        baseline: Some("middle".into()),
        rotation: None,
        stroke: None,
        stroke_width: None,
    }
}

fn circle(x: f64, y: f64, r: f64, fill: &str) -> Shape {
    Shape::Circle {
        x,
        y,
        r,
        fill: fill.into(),
        opacity: Some(1.0),
        rotation: None,
        stroke: None,
        stroke_width: None,
    }
}

fn ellipse(x: f64, y: f64, rx: f64, ry: f64, fill: &str, opacity: f64) -> Shape {
    Shape::Ellipse {
        x,
        y,
        rx,
        ry,
        fill: fill.into(),
        opacity: Some(opacity),
        rotation: Some(0.0),
        stroke: None,
        stroke_width: None,
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64, fill: &str, radius: f64, opacity: f64) -> Shape {
    Shape::Rect {
        x,
        y,
        w,
        h,
        fill: fill.into(),
        radius: Some(radius),
        opacity: Some(opacity),
        rotation: None,
        stroke: None,
        stroke_width: None,
    }
}

/// 启动时展示的开场动画。
pub fn default_scene() -> Scene {
    let accent = "#7c6cf0";
    let accent2 = "#5a8bff";
    let cyan = "#38bdf8";

    let mut layers = Vec::new();

    // 背景光斑
    layers.push(animated(
        ellipse(1040.0, 150.0, 320.0, 320.0, "#171b2b", 1.0),
        vec![
            anim("opacity", 0.0, 1.0, 0.0, 1.4, Easing::EaseOut),
            anim("x", 1200.0, 1040.0, 0.0, 1.6, Easing::EaseOutCubic),
        ],
    ));
    layers.push(animated(
        ellipse(230.0, 610.0, 260.0, 260.0, "#141a2e", 1.0),
        vec![
            anim("opacity", 0.0, 1.0, 0.2, 1.6, Easing::EaseOut),
            anim("y", 700.0, 610.0, 0.2, 1.8, Easing::EaseOutCubic),
        ],
    ));

    // 斜向扫光
    layers.push(animated(
        Shape::Rect {
            x: -360.0,
            y: 190.0,
            w: 220.0,
            h: 200.0,
            fill: "#ffffff".into(),
            radius: Some(14.0),
            opacity: Some(0.05),
            rotation: Some(-0.18),
            stroke: None,
            stroke_width: None,
        },
        vec![anim("x", -360.0, 1580.0, 2.1, 3.7, Easing::EaseInOut)],
    ));

    // 标题
    layers.push(animated(
        text(640.0, 308.0, "LONICERA", 104.0, "#f4f6fc"),
        vec![
            anim("opacity", 0.0, 1.0, 0.15, 1.15, Easing::EaseOutCubic),
            anim("y", 352.0, 308.0, 0.15, 1.35, Easing::EaseOutCubic),
        ],
    ));

    // 副标题
    layers.push(animated(
        text(640.0, 374.0, "AI MOTION GRAPHICS STUDIO", 20.0, "#8f97ab"),
        vec![
            anim("opacity", 0.0, 1.0, 0.85, 1.75, Easing::EaseOut),
            anim("y", 394.0, 374.0, 0.85, 1.95, Easing::EaseOutCubic),
        ],
    ));

    // 强调下划线
    layers.push(animated(
        rect(560.0, 414.0, 160.0, 6.0, accent, 3.0, 1.0),
        vec![
            anim("w", 0.0, 160.0, 1.1, 1.9, Easing::EaseOutCubic),
            anim("x", 640.0, 560.0, 1.1, 1.9, Easing::EaseOutCubic),
        ],
    ));

    // 三个强调点
    let dots = [(600.0, accent), (640.0, accent2), (680.0, cyan)];
    for (i, (x, color)) in dots.into_iter().enumerate() {
        let delay = 1.5 + i as f64 * 0.14;
        layers.push(animated(
            circle(x, 470.0, 7.0, color),
            vec![
                anim("r", 0.0, 7.0, delay, delay + 0.7, Easing::EaseOutBack),
                anim("opacity", 0.0, 1.0, delay, delay + 0.4, Easing::EaseOut),
            ],
        ));
    }

    // 底部提示
    layers.push(animated(
        text(640.0, 570.0, "在右侧描述你想要的动画 →", 17.0, "#5b6275"),
        vec![anim("opacity", 0.0, 1.0, 2.4, 3.4, Easing::EaseOut)],
    ));

    Scene {
        name: "Lonicera 开场".into(),
        duration_sec: 6.0,
        width: 1280.0,
        height: 720.0,
        background: "#0e1017".into(),
        layers,
    }
}

struct Palette {
    name: &'static str,
    bg: &'static str,
    accent: &'static str,
    accent2: &'static str,
    accent3: &'static str,
    title: &'static str,
}

fn palette_for(prompt: &str) -> Palette {
    if prompt.contains("红") {
        Palette { name: "赤霞", bg: "#150d12", accent: "#ef4444", accent2: "#fb7185", accent3: "#f97316", title: "#fdf2f3" }
    } else if prompt.contains("橙") {
        Palette { name: "落日", bg: "#160f0b", accent: "#f97316", accent2: "#fbbf24", accent3: "#fb7185", title: "#fdf6ee" }
    } else if prompt.contains("黄") {
        Palette { name: "暖阳", bg: "#141208", accent: "#eab308", accent2: "#facc15", accent3: "#f97316", title: "#fdfbea" }
    } else if prompt.contains("绿") {
        Palette { name: "青苔", bg: "#0b1410", accent: "#22c55e", accent2: "#4ade80", accent3: "#2dd4bf", title: "#f0fdf4" }
    } else if prompt.contains("紫") || prompt.contains("粉") {
        Palette { name: "霓虹", bg: "#120d1c", accent: "#a855f7", accent2: "#ec4899", accent3: "#818cf8", title: "#faf5ff" }
    } else if prompt.contains("蓝") || prompt.contains("科技") {
        Palette { name: "深海", bg: "#080f1e", accent: "#3b82f6", accent2: "#38bdf8", accent3: "#818cf8", title: "#eff6ff" }
    } else {
        Palette { name: "紫曜", bg: "#0e1017", accent: "#7c6cf0", accent2: "#5a8bff", accent3: "#38bdf8", title: "#f4f6fc" }
    }
}

/// 提取时长：匹配「N 秒 / Ns / N s」，找不到返回默认值。
fn extract_duration(prompt: &str) -> f64 {
    let chars: Vec<char> = prompt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            let mut num = String::new();
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                num.push(chars[j]);
                j += 1;
            }
            // 跳过数字与单位之间的空白
            let mut k = j;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            let is_seconds = chars
                .get(k)
                .map(|c| {
                    *c == '秒'
                        || (*c == 's' && !chars.get(k + 1).is_some_and(|n| n.is_alphabetic()))
                })
                .unwrap_or(false);
            if is_seconds {
                if let Ok(v) = num.parse::<f64>() {
                    if (0.5..=120.0).contains(&v) {
                        return v;
                    }
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    6.0
}

fn extract_title(prompt: &str) -> String {
    // 优先取引号内文字
    for (open, close) in [('「', '」'), ('『', '』'), ('“', '”'), ('"', '"'), ('\'', '\'')] {
        if let Some(start) = prompt.find(open) {
            let after = start + open.len_utf8();
            if let Some(len) = prompt[after..].find(close) {
                let s = prompt[after..after + len].trim();
                if !s.is_empty() && s.chars().count() <= 24 {
                    return s.to_string();
                }
            }
        }
    }
    // 其次取关键词后面的内容
    for kw in ["文字", "标题", "写上", "写着", "内容"] {
        if let Some(pos) = prompt.find(kw) {
            let after = &prompt[pos + kw.len()..];
            let after = after.trim_start_matches(['是', '为', '：', ':', ' ', '的']);
            let candidate = after
                .split(['，', ',', '。', '！', '!', '？', '?', '\n'])
                .next()
                .unwrap_or("")
                .trim();
            if !candidate.is_empty() && candidate.chars().count() <= 24 {
                return candidate.to_string();
            }
        }
    }
    let fallback: String = prompt.chars().take(12).collect();
    let fallback = fallback.trim();
    if fallback.is_empty() {
        "MOTION".into()
    } else {
        fallback.to_string()
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / ((1u64 << 31) as f64)
    }
}

/// 根据自然语言描述离线生成演示场景。
pub fn scene_from_prompt(prompt: &str) -> Scene {
    let palette = palette_for(prompt);
    let duration = extract_duration(prompt);
    let title = extract_title(prompt);
    let has_particles = ["粒子", "星空", "星星", "光点", "尘埃"]
        .iter()
        .any(|k| prompt.contains(k));
    let has_geo = ["几何", "方块", "矩形", "卡片", "科技感"]
        .iter()
        .any(|k| prompt.contains(k));

    let mut layers = Vec::new();

    // 背景光斑
    layers.push(animated(
        ellipse(1010.0, 170.0, 300.0, 300.0, palette.accent, 0.10),
        vec![
            anim("opacity", 0.0, 0.10, 0.0, 1.2, Easing::EaseOut),
            anim("x", 1180.0, 1010.0, 0.0, 1.6, Easing::EaseOutCubic),
        ],
    ));
    layers.push(animated(
        ellipse(280.0, 600.0, 240.0, 240.0, palette.accent2, 0.08),
        vec![
            anim("opacity", 0.0, 0.08, 0.2, 1.4, Easing::EaseOut),
            anim("y", 700.0, 600.0, 0.2, 1.8, Easing::EaseOutCubic),
        ],
    ));

    // 粒子 / 几何装饰
    if has_particles {
        let mut seed = DefaultHasher::new();
        prompt.hash(&mut seed);
        let mut rng = Lcg(seed.finish() | 1);
        for _ in 0..30 {
            let x = 70.0 + rng.next() * 1140.0;
            let y = 60.0 + rng.next() * 600.0;
            let r = 0.8 + rng.next() * 2.6;
            let delay = 0.2 + rng.next() * 2.4;
            let color = if rng.next() > 0.5 { palette.accent2 } else { palette.accent3 };
            layers.push(animated(
                circle(x, y, r, color),
                vec![
                    anim("opacity", 0.0, 0.75, delay, delay + 0.8, Easing::EaseOut),
                    anim("y", y + 18.0, y, delay, duration.min(delay + 2.0), Easing::EaseOutCubic),
                ],
            ));
        }
    }

    if has_geo {
        for (i, (x, y, w, h, color)) in [
            (180.0, 170.0, 120.0, 160.0, palette.accent),
            (1080.0, 210.0, 90.0, 240.0, palette.accent2),
            (1020.0, 520.0, 160.0, 80.0, palette.accent3),
        ]
        .into_iter()
        .enumerate()
        {
            let delay = 0.15 + i as f64 * 0.15;
            layers.push(animated(
                rect(x, y, w, h, color, 18.0, 0.12),
                vec![
                    anim("opacity", 0.0, 0.12, delay, delay + 0.7, Easing::EaseOut),
                    anim("y", y + 40.0, y, delay, delay + 0.9, Easing::EaseOutCubic),
                ],
            ));
        }
    }

    // 标题
    layers.push(animated(
        text(640.0, 320.0, &title, 92.0, palette.title),
        vec![
            anim("opacity", 0.0, 1.0, 0.15, 1.05, Easing::EaseOutCubic),
            anim("y", 366.0, 320.0, 0.15, 1.3, Easing::EaseOutCubic),
            anim("size", 84.0, 92.0, 0.15, 1.3, Easing::EaseOutCubic),
        ],
    ));

    // 副标题
    layers.push(animated(
        text(640.0, 390.0, "MOTION GRAPHICS · AI", 20.0, "#8f97ab"),
        vec![
            anim("opacity", 0.0, 1.0, 0.75, 1.55, Easing::EaseOut),
            anim("y", 410.0, 390.0, 0.75, 1.75, Easing::EaseOutCubic),
        ],
    ));

    // 强调线
    layers.push(animated(
        rect(560.0, 432.0, 160.0, 6.0, palette.accent, 3.0, 1.0),
        vec![
            anim("w", 0.0, 160.0, 0.95, 1.7, Easing::EaseOutCubic),
            anim("x", 640.0, 560.0, 0.95, 1.7, Easing::EaseOutCubic),
        ],
    ));

    // 三个强调点
    let dots = [(598.0, palette.accent), (640.0, palette.accent2), (682.0, palette.accent3)];
    for (i, (x, color)) in dots.into_iter().enumerate() {
        let delay = 1.35 + i as f64 * 0.14;
        layers.push(animated(
            circle(x, 486.0, 6.5, color),
            vec![
                anim("r", 0.0, 6.5, delay, delay + 0.7, Easing::EaseOutBack),
                anim("opacity", 0.0, 1.0, delay, delay + 0.4, Easing::EaseOut),
            ],
        ));
    }

    // 收尾扫光
    layers.push(animated(
        Shape::Rect {
            x: -340.0,
            y: 240.0,
            w: 200.0,
            h: 160.0,
            fill: "#ffffff".into(),
            radius: Some(12.0),
            opacity: Some(0.05),
            rotation: Some(-0.18),
            stroke: None,
            stroke_width: None,
        },
        vec![anim(
            "x",
            -340.0,
            1560.0,
            (duration * 0.55).max(2.0),
            (duration * 0.85).max(3.0),
            Easing::EaseInOut,
        )],
    ));

    let mut scene = Scene {
        name: title.clone(),
        duration_sec: duration,
        width: 1280.0,
        height: 720.0,
        background: palette.bg.into(),
        layers,
    };
    scene.normalize();
    scene
}

/// Mock 模式下流式输出的解说文本。
pub fn mock_explanation(scene: &Scene, prompt: &str) -> String {
    let palette = palette_for(prompt);
    let effect = if prompt.contains("粒子") || prompt.contains("星空") {
        "背景叠加了错峰浮现的粒子层"
    } else if prompt.contains("几何") || prompt.contains("方块") {
        "背景加入了缓慢上浮的几何色块"
    } else {
        "前景强调线与装饰点依次弹出"
    };
    format!(
        "已按你的描述生成「{}」：{} 秒，{} 配色。\n标题从下方淡入并缓动上浮，{}，最后有一道斜向扫光收尾。\n预览已自动播放；想调整节奏、颜色或文案，直接继续告诉我。",
        scene.name, scene.duration_sec, palette.name, effect
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scene_has_layers_and_evaluates() {
        let scene = default_scene();
        assert!(!scene.layers.is_empty());
        let frame = scene.evaluate(1.0);
        assert_eq!(frame.len(), scene.layers.len() + 1);
    }

    #[test]
    fn prompt_scene_extracts_title_and_duration() {
        let scene = scene_from_prompt("做一个 4 秒的蓝色科技感动画，标题「HELLO AI」");
        assert_eq!(scene.duration_sec, 4.0);
        assert_eq!(scene.name, "HELLO AI");
    }

    #[test]
    fn animations_are_clamped_in_normalize() {
        let mut scene = default_scene();
        scene.duration_sec = 2.0;
        scene.normalize();
        for layer in &scene.layers {
            for anim in &layer.animations {
                assert!(anim.start.unwrap_or(0.0) <= 2.0);
                assert!(anim.end.unwrap_or(0.0) <= 2.0);
            }
        }
    }
}
