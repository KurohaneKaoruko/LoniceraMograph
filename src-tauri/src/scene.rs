//! 场景模型与逐帧求值。
//!
//! 场景是一份纯 JSON 数据（AI 的产出物），每帧由 Rust 在 [`Scene::evaluate`]
//! 中求值成一组绘制指令，前端只负责光栅化。这样 AI 只需要写数据，
//! 不需要（也没有能力）直接操作渲染器。

use serde::{Deserialize, Serialize};

fn default_scene_name() -> String {
    "未命名场景".into()
}
fn default_duration() -> f64 {
    6.0
}
fn default_width() -> f64 {
    1280.0
}
fn default_height() -> f64 {
    720.0
}
fn default_background() -> String {
    "#0e1017".into()
}
fn default_fill() -> String {
    "#ffffff".into()
}
fn default_stroke() -> String {
    "#ffffff".into()
}
fn default_line_width() -> f64 {
    2.0
}
fn default_text_size() -> f64 {
    48.0
}

/// 一个完整场景：画布信息 + 按绘制顺序排列的图层。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    #[serde(default = "default_scene_name")]
    pub name: String,
    #[serde(default = "default_duration")]
    pub duration_sec: f64,
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_height")]
    pub height: f64,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

/// 图层 = 基础图形 + 一组补间动画。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub shape: Shape,
    #[serde(default)]
    pub animations: Vec<Animation>,
}

/// 基础图形。序列化后即为前端消费的绘制指令（tag = `kind`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Shape {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        #[serde(default = "default_fill")]
        fill: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
    },
    Circle {
        x: f64,
        y: f64,
        r: f64,
        #[serde(default = "default_fill")]
        fill: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
    },
    Ellipse {
        x: f64,
        y: f64,
        rx: f64,
        ry: f64,
        #[serde(default = "default_fill")]
        fill: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        #[serde(default = "default_stroke")]
        stroke: String,
        #[serde(default = "default_line_width")]
        width: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
    },
    Text {
        x: f64,
        y: f64,
        #[serde(default)]
        text: String,
        #[serde(default = "default_text_size")]
        size: f64,
        #[serde(default = "default_fill")]
        fill: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        /// "left" | "center" | "right"（默认 left）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<String>,
        /// "top" | "middle" | "bottom" | "alphabetic"（默认 alphabetic）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        baseline: Option<String>,
    },
    Path {
        #[serde(default)]
        d: String,
        #[serde(default = "default_fill")]
        fill: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
    },
}

/// 前端绘制指令与图元同构。
pub type DrawCmd = Shape;

/// 一条补间：把属性 `prop` 从 `from` 缓动到 `to`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub prop: String,
    #[serde(default)]
    pub from: Option<f64>,
    #[serde(default)]
    pub to: Option<f64>,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub easing: Easing,
}

/// 缓动函数。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseOutBack,
    EaseOutBounce,
}

impl Easing {
    pub fn apply(self, p: f64) -> f64 {
        let p = p.clamp(0.0, 1.0);
        match self {
            Easing::Linear => p,
            Easing::EaseIn => p * p,
            Easing::EaseOut => 1.0 - (1.0 - p) * (1.0 - p),
            Easing::EaseInOut => {
                if p < 0.5 {
                    2.0 * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(2) / 2.0
                }
            }
            Easing::EaseInCubic => p * p * p,
            Easing::EaseOutCubic => 1.0 - (1.0 - p).powi(3),
            Easing::EaseInOutCubic => {
                if p < 0.5 {
                    4.0 * p * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
                }
            }
            Easing::EaseOutBack => {
                const C1: f64 = 1.70158;
                const C3: f64 = C1 + 1.0;
                1.0 + C3 * (p - 1.0).powi(3) + C1 * (p - 1.0).powi(2)
            }
            Easing::EaseOutBounce => {
                const N1: f64 = 7.5625;
                const D1: f64 = 2.75;
                if p < 1.0 / D1 {
                    N1 * p * p
                } else if p < 2.0 / D1 {
                    let p = p - 1.5 / D1;
                    N1 * p * p + 0.75
                } else if p < 2.5 / D1 {
                    let p = p - 2.25 / D1;
                    N1 * p * p + 0.9375
                } else {
                    let p = p - 2.625 / D1;
                    N1 * p * p + 0.984375
                }
            }
        }
    }
}

impl Shape {
    /// 图元类型名（"rect" / "text" …），用于错误信息与日志。
    pub fn kind_name(&self) -> &'static str {
        match self {
            Shape::Rect { .. } => "rect",
            Shape::Circle { .. } => "circle",
            Shape::Ellipse { .. } => "ellipse",
            Shape::Line { .. } => "line",
            Shape::Text { .. } => "text",
            Shape::Path { .. } => "path",
        }
    }

    /// 读取可动画属性的当前值；不存在的属性返回 `None`。
    pub fn get_prop(&self, prop: &str) -> Option<f64> {
        match (self, prop) {
            (Shape::Rect { x, .. }, "x") => Some(*x),
            (Shape::Rect { y, .. }, "y") => Some(*y),
            (Shape::Rect { w, .. }, "w") => Some(*w),
            (Shape::Rect { h, .. }, "h") => Some(*h),
            (Shape::Rect { radius, .. }, "radius") => Some(radius.unwrap_or(0.0)),

            (Shape::Circle { x, .. }, "x") => Some(*x),
            (Shape::Circle { y, .. }, "y") => Some(*y),
            (Shape::Circle { r, .. }, "r") => Some(*r),

            (Shape::Ellipse { x, .. }, "x") => Some(*x),
            (Shape::Ellipse { y, .. }, "y") => Some(*y),
            (Shape::Ellipse { rx, .. }, "rx") => Some(*rx),
            (Shape::Ellipse { ry, .. }, "ry") => Some(*ry),

            (Shape::Line { x1, .. }, "x1") => Some(*x1),
            (Shape::Line { y1, .. }, "y1") => Some(*y1),
            (Shape::Line { x2, .. }, "x2") => Some(*x2),
            (Shape::Line { y2, .. }, "y2") => Some(*y2),
            (Shape::Line { width, .. }, "width") => Some(*width),

            (Shape::Text { x, .. }, "x") => Some(*x),
            (Shape::Text { y, .. }, "y") => Some(*y),
            (Shape::Text { size, .. }, "size") => Some(*size),

            (Shape::Path { .. }, _) => None,

            (_, "opacity") => Some(self.opacity().unwrap_or(1.0)),
            (_, "rotation") => Some(self.rotation().unwrap_or(0.0)),
            _ => None,
        }
    }

    /// 写入可动画属性；未知属性或该图元不支持的属性会被安全忽略。
    pub fn set_prop(&mut self, prop: &str, value: f64) {
        if !value.is_finite() {
            return;
        }
        match (&mut *self, prop) {
            (Shape::Rect { x, .. }, "x") => *x = value,
            (Shape::Rect { y, .. }, "y") => *y = value,
            (Shape::Rect { w, .. }, "w") => *w = value.max(0.0),
            (Shape::Rect { h, .. }, "h") => *h = value.max(0.0),
            (Shape::Rect { radius, .. }, "radius") => *radius = Some(value.max(0.0)),

            (Shape::Circle { x, .. }, "x") => *x = value,
            (Shape::Circle { y, .. }, "y") => *y = value,
            (Shape::Circle { r, .. }, "r") => *r = value.max(0.0),

            (Shape::Ellipse { x, .. }, "x") => *x = value,
            (Shape::Ellipse { y, .. }, "y") => *y = value,
            (Shape::Ellipse { rx, .. }, "rx") => *rx = value.max(0.0),
            (Shape::Ellipse { ry, .. }, "ry") => *ry = value.max(0.0),

            (Shape::Line { x1, .. }, "x1") => *x1 = value,
            (Shape::Line { y1, .. }, "y1") => *y1 = value,
            (Shape::Line { x2, .. }, "x2") => *x2 = value,
            (Shape::Line { y2, .. }, "y2") => *y2 = value,
            (Shape::Line { width, .. }, "width") => *width = value.max(0.0),

            (Shape::Text { x, .. }, "x") => *x = value,
            (Shape::Text { y, .. }, "y") => *y = value,
            (Shape::Text { size, .. }, "size") => *size = value.max(1.0),

            (_, "opacity") => {
                let v = value.clamp(0.0, 1.0);
                match self {
                    Shape::Rect { opacity, .. }
                    | Shape::Circle { opacity, .. }
                    | Shape::Ellipse { opacity, .. }
                    | Shape::Line { opacity, .. }
                    | Shape::Text { opacity, .. }
                    | Shape::Path { opacity, .. } => *opacity = Some(v),
                }
            }
            (_, "rotation") => {
                match self {
                    Shape::Rect { rotation, .. }
                    | Shape::Ellipse { rotation, .. }
                    | Shape::Path { rotation, .. } => *rotation = Some(value),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn opacity(&self) -> Option<f64> {
        match self {
            Shape::Rect { opacity, .. }
            | Shape::Circle { opacity, .. }
            | Shape::Ellipse { opacity, .. }
            | Shape::Line { opacity, .. }
            | Shape::Text { opacity, .. }
            | Shape::Path { opacity, .. } => *opacity,
        }
    }

    fn rotation(&self) -> Option<f64> {
        match self {
            Shape::Rect { rotation, .. }
            | Shape::Ellipse { rotation, .. }
            | Shape::Path { rotation, .. } => *rotation,
            _ => None,
        }
    }
}

impl Scene {
    /// 求值某一时刻的绘制指令（第 0 条永远是背景）。
    pub fn evaluate(&self, time: f64) -> Vec<DrawCmd> {
        let t = time.clamp(0.0, self.duration_sec);
        let mut cmds = Vec::with_capacity(self.layers.len() + 1);

        cmds.push(Shape::Rect {
            x: 0.0,
            y: 0.0,
            w: self.width,
            h: self.height,
            fill: self.background.clone(),
            radius: None,
            opacity: Some(1.0),
            rotation: None,
        });

        for layer in &self.layers {
            let mut shape = layer.shape.clone();
            for anim in &layer.animations {
                let start = anim.start.unwrap_or(0.0);
                let end = anim.end.unwrap_or(self.duration_sec).max(start + 1e-4);
                let to = anim
                    .to
                    .unwrap_or_else(|| shape.get_prop(&anim.prop).unwrap_or(0.0));
                let from = anim.from.unwrap_or(to);

                let value = if t <= start {
                    from
                } else if t >= end {
                    to
                } else {
                    let p = (t - start) / (end - start);
                    from + (to - from) * anim.easing.apply(p)
                };
                shape.set_prop(&anim.prop, value);
            }
            cmds.push(shape);
        }
        cmds
    }

    /// 修正 AI 输出中明显越界的数值，避免画出无法观看的场景。
    pub fn normalize(&mut self) {
        if self.name.trim().is_empty() {
            self.name = default_scene_name();
        }
        self.duration_sec = if self.duration_sec.is_finite() {
            self.duration_sec.clamp(0.5, 300.0)
        } else {
            default_duration()
        };
        self.width = if self.width.is_finite() {
            self.width.clamp(160.0, 7680.0)
        } else {
            default_width()
        };
        self.height = if self.height.is_finite() {
            self.height.clamp(90.0, 4320.0)
        } else {
            default_height()
        };
        if self.background.trim().is_empty() {
            self.background = default_background();
        }
        for layer in &mut self.layers {
            for anim in &mut layer.animations {
                if let Some(s) = anim.start {
                    anim.start = Some(s.clamp(0.0, self.duration_sec));
                }
                if let Some(e) = anim.end {
                    anim.end = Some(e.clamp(0.0, self.duration_sec));
                }
            }
        }
    }

    pub fn meta(&self) -> SceneMeta {
        SceneMeta::from(self)
    }
}

/// 场景元信息，随 `scene` 事件下发给前端。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneMeta {
    pub name: String,
    pub duration_sec: f64,
    pub width: f64,
    pub height: f64,
}

impl From<&Scene> for SceneMeta {
    fn from(scene: &Scene) -> Self {
        Self {
            name: scene.name.clone(),
            duration_sec: scene.duration_sec,
            width: scene.width,
            height: scene.height,
        }
    }
}
