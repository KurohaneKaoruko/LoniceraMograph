//! 内置 MG 模板库：打包进二进制的动画起点集。
//!
//! 每个模板 = 一份场景 JSON（`templates/*.json`，`include_str!` 编译期嵌入）
//! + 一条元数据（名称/分类/描述/时长，供前端模板面板浏览）。
//! 模板与 agent 生成的场景走同一套 [`Scene`] 模型，"一键插入"即加载预览，
//! 用户再保存或让 AI 改造均可。
//!
//! 模板遵守两条写作约定（`tests::templates_are_valid` 强制校验）：
//! 1. 每个图层每个属性只挂一条补间：引擎中每条补间在 `t >= end` 后会把属性
//!    永远钳在 `to`，同 prop 多条会互相覆盖——需要"往返"效果时克隆图层接力；
//! 2. 数值都落在 normalize/check 的合法值域内（时长 0.5-300s、缓动为九种之一）。

use serde::Serialize;

use crate::scene::Scene;

/// 模板分类（前端按此分组展示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TemplateCategory {
    /// 标题/文字入场
    Title,
    /// 进度条、数据可视化
    Data,
    /// Logo/品牌展示
    Logo,
    /// 转场（淡入淡出、滑入滑出）
    Transition,
    /// 可循环背景（呼吸、漂浮）
    Background,
    /// 倒计时等引导画面
    Countdown,
}

impl TemplateCategory {
    /// 分类的中文展示名（面板分组标题；前端 templates.ts 有同一映射）。
    #[allow(dead_code)] // 序列化后随 TemplateMeta 下发，Rust 侧自身不读取
    pub fn label(&self) -> &'static str {
        match self {
            TemplateCategory::Title => "标题入场",
            TemplateCategory::Data => "数据图表",
            TemplateCategory::Logo => "Logo 展示",
            TemplateCategory::Transition => "转场",
            TemplateCategory::Background => "循环背景",
            TemplateCategory::Countdown => "倒计时",
        }
    }
}

/// 一条模板的元数据（场景 JSON 另行下发）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateMeta {
    /// 稳定 ID（文件名去掉 .json），load_template 用它索引。
    pub id: String,
    pub name: String,
    pub category: TemplateCategory,
    pub description: String,
    /// 场景时长（秒），列表页即可判断节奏。
    pub duration_sec: f64,
    /// 图层数，粗略复杂度提示。
    pub layer_count: usize,
}

/// 单条模板登记：ID、分类、描述。JSON 文件由 SOURCES 表的
/// include_str! 路径登记（两表按位置一一对应，测试强制校验）。
struct TemplateEntry {
    id: &'static str,
    category: TemplateCategory,
    description: &'static str,
}

macro_rules! entry {
    ($id:literal, $cat:ident, $desc:literal) => {
        TemplateEntry {
            id: $id,
            category: TemplateCategory::$cat,
            description: $desc,
        }
    };
}

/// 内置模板登记表：新增模板 = 放一份 JSON + 在此加一行（顺序即面板展示顺序）。
static ENTRIES: &[TemplateEntry] = &[
    entry!(
        "title-fade",
        Title,
        "主标题上浮淡入，副标题跟进，装饰线左右展开；适合片头与章节题"
    ),
    entry!(
        "progress-bar",
        Data,
        "两组数据条依次生长并亮出百分比，改文案换数值即可复用"
    ),
    entry!(
        "logo-stroke",
        Logo,
        "图形先描边勾勒再填充浮现，品牌名随后入场；换 path 即换成你的 Logo"
    ),
    entry!(
        "transition-fade",
        Transition,
        "整屏色幕淡入承载章节题再淡出，节奏柔和，适合章节间过渡"
    ),
    entry!(
        "transition-wipe",
        Transition,
        "色块从左滑入、停留展示、向右滑出，动感更强"
    ),
    entry!(
        "bg-float",
        Background,
        "五个圆错相漂浮，首尾闭合无接缝循环；垫在标题/数据动画底下"
    ),
    entry!(
        "bg-breathe",
        Background,
        "中心光晕缓慢呼吸，无接缝循环，任何前景都适用"
    ),
    entry!(
        "countdown",
        Countdown,
        "圆环弹出后 5→1 数字接力：每个数字放大缩小淡出，收尾可接 GO 画面"
    ),
];

/// (id, 原始 JSON) 表：include_str! 编译期嵌入，零运行时 IO。
static SOURCES: &[(&str, &str)] = &[
    ("title-fade", include_str!("../templates/title-fade.json")),
    (
        "progress-bar",
        include_str!("../templates/progress-bar.json"),
    ),
    ("logo-stroke", include_str!("../templates/logo-stroke.json")),
    (
        "transition-fade",
        include_str!("../templates/transition-fade.json"),
    ),
    (
        "transition-wipe",
        include_str!("../templates/transition-wipe.json"),
    ),
    ("bg-float", include_str!("../templates/bg-float.json")),
    ("bg-breathe", include_str!("../templates/bg-breathe.json")),
    ("countdown", include_str!("../templates/countdown.json")),
];

fn source_of(id: &str) -> Option<&'static str> {
    SOURCES
        .iter()
        .find(|(tid, _)| *tid == id)
        .map(|(_, src)| *src)
}

/// 全部模板元数据（按登记顺序，即面板展示顺序）。
/// 名称/时长/图层数从场景 JSON 解析得出，保证与实际内容一致。
pub fn list() -> Vec<TemplateMeta> {
    ENTRIES
        .iter()
        .filter_map(|e| {
            let scene: Scene = serde_json::from_str(source_of(e.id)?).ok()?;
            Some(TemplateMeta {
                id: e.id.to_string(),
                name: scene.name,
                category: e.category,
                description: e.description.to_string(),
                duration_sec: scene.duration_sec,
                layer_count: scene.layers.len(),
            })
        })
        .collect()
}

/// 按 ID 取模板场景（解析失败视为内置资源损坏，返回 Err）。
pub fn load(id: &str) -> Result<Scene, String> {
    let src = source_of(id).ok_or_else(|| format!("未知模板：{id}"))?;
    serde_json::from_str(src).map_err(|e| format!("模板 {id} JSON 解析失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Easing;

    /// 全量合法性校验：所有模板都能被现有场景解析器接受，
    /// 时长/缓动值域合法，且没有"同图层同属性多条补间"的覆盖陷阱。
    #[test]
    fn templates_are_valid() {
        let metas = list();
        assert!(
            metas.len() >= 6,
            "内置模板应至少 6 个，实际 {}",
            metas.len()
        );

        let mut seen_ids = std::collections::HashSet::new();
        for meta in &metas {
            assert!(
                seen_ids.insert(meta.id.as_str()),
                "模板 ID 重复：{}",
                meta.id
            );
            assert!(
                !meta.name.trim().is_empty(),
                "模板 {} 名称不能为空",
                meta.id
            );
            assert!(
                !meta.description.trim().is_empty(),
                "模板 {} 描述不能为空",
                meta.id
            );
            assert!(
                (0.5..=300.0).contains(&meta.duration_sec),
                "模板 {} 时长越界：{}",
                meta.id,
                meta.duration_sec
            );
            assert!(meta.layer_count >= 1, "模板 {} 不能没有图层", meta.id);

            let scene = load(&meta.id).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(scene.name, meta.name, "模板 {} 名称应与登记一致", meta.id);
            assert_eq!(scene.duration_sec, meta.duration_sec);
            assert_eq!(scene.layers.len(), meta.layer_count);

            // 缓动值域：反序列化时未知缓动会静默回退 linear，
            // 因此逐条重新解析原始 JSON 校验 easing 字面值。
            let raw: serde_json::Value =
                serde_json::from_str(source_of(&meta.id).expect("登记表与来源表应一一对应"))
                    .unwrap();
            for layer in raw["layers"].as_array().unwrap() {
                for anim in layer["animations"].as_array().unwrap() {
                    let name = anim["easing"].as_str().unwrap_or("linear");
                    assert!(
                        matches!(
                            name,
                            "linear"
                                | "easeIn"
                                | "easeOut"
                                | "easeInOut"
                                | "easeInCubic"
                                | "easeOutCubic"
                                | "easeInOutCubic"
                                | "easeOutBack"
                                | "easeOutBounce"
                        ),
                        "模板 {} 出现未知缓动：{}",
                        meta.id,
                        name
                    );
                }
                // 动画时间必须落在场景时长内
                let dur = raw["durationSec"].as_f64().unwrap();
                for anim in layer["animations"].as_array().unwrap() {
                    for key in ["start", "end"] {
                        if let Some(v) = anim[key].as_f64() {
                            assert!(
                                (0.0..=dur).contains(&v),
                                "模板 {} 动画 {key}={v} 超出时长 {dur}",
                                meta.id
                            );
                        }
                    }
                }
            }

            // 同图层同属性只允许一条补间（引擎按序覆盖，多条即设计错误）
            let mut props = std::collections::HashSet::new();
            for layer in &scene.layers {
                props.clear();
                for anim in &layer.animations {
                    assert!(
                        props.insert(anim.prop.as_str()),
                        "模板 {} 图层「{}」属性 {} 重复补间",
                        meta.id,
                        layer.shape.kind_name(),
                        anim.prop
                    );
                }
                // 每条补间的属性必须是该图元支持的属性
                for anim in &layer.animations {
                    assert!(
                        layer.shape.get_prop(&anim.prop).is_some(),
                        "模板 {} 图层「{}」属性 {} 不受支持",
                        meta.id,
                        layer.shape.kind_name(),
                        anim.prop
                    );
                }
            }

            // easing 反序列化无损：Easing 是 Copy 的九值枚举，此处确保值域合法
            let _ = Easing::Linear;
        }
    }

    /// 登记表（ENTRIES）与来源表（SOURCES）应一一对应，防漏登/错位。
    #[test]
    fn entries_match_sources() {
        assert_eq!(ENTRIES.len(), SOURCES.len(), "登记表与来源表长度不一致");
        for (e, (tid, _)) in ENTRIES.iter().zip(SOURCES.iter()) {
            assert_eq!(e.id, *tid, "登记表与来源表顺序不一致：{} vs {}", e.id, tid);
        }
        let listed = list();
        assert_eq!(listed.len(), ENTRIES.len(), "list() 不应丢弃任何模板");
    }

    /// load() 对未知 ID 应报错而不是 panic。
    #[test]
    fn load_unknown_id_errors() {
        assert!(load("no-such-template").is_err());
        assert!(load("").is_err());
    }
}
