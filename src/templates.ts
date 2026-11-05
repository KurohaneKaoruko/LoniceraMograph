/// 模板面板纯逻辑（无 DOM 依赖，便于单元测试）。
/// 模板元数据由 Rust `list_templates` 下发；这里只做展示决策：
/// 分类排序、中文分组、列表条目文案。

/// 与 Rust `TemplateMeta` 一一对应（serde camelCase）。
export interface TemplateMeta {
  id: string;
  name: string;
  category: string;
  description: string;
  durationSec: number;
  layerCount: number;
}

/** 分类展示顺序（未收录的分类排最后，避免新分类被丢弃）。 */
const CATEGORY_ORDER = [
  "title",
  "data",
  "logo",
  "transition",
  "background",
  "countdown",
];

/** 分类的中文组名。 */
const CATEGORY_LABEL: Record<string, string> = {
  title: "标题入场",
  data: "数据图表",
  logo: "Logo 展示",
  transition: "转场",
  background: "循环背景",
  countdown: "倒计时",
};

/** 分类图标（与文件面板的 🎬/⚙ 风格一致）。 */
const CATEGORY_ICON: Record<string, string> = {
  title: "🎬",
  data: "📊",
  logo: "✦",
  transition: "⇄",
  background: "◐",
  countdown: "⏱",
};

/** 已收录分类直接映射，未知分类回退原值。 */
export function categoryLabel(category: string): string {
  return CATEGORY_LABEL[category] ?? category;
}

export function categoryIcon(category: string): string {
  return CATEGORY_ICON[category] ?? "🎬";
}

/**
 * 按分类顺序分组；组内保持 Rust 下发（登记）顺序。
 * 未知分类单独成组排在已收录分类之后，保证不丢数据。
 */
export function groupByCategory(
  metas: TemplateMeta[],
): Array<{ category: string; label: string; items: TemplateMeta[] }> {
  const byCat = new Map<string, TemplateMeta[]>();
  for (const meta of metas) {
    const list = byCat.get(meta.category) ?? [];
    list.push(meta);
    byCat.set(meta.category, list);
  }
  const ordered = [
    ...CATEGORY_ORDER.filter((c) => byCat.has(c)),
    ...[...byCat.keys()].filter((c) => !CATEGORY_ORDER.includes(c)),
  ];
  return ordered.map((category) => ({
    category,
    label: categoryLabel(category),
    items: byCat.get(category) ?? [],
  }));
}

/** 列表条目主文案：名称。 */
export function templateTitle(meta: TemplateMeta): string {
  return meta.name;
}

/** 列表条目副文案：时长与图层数（如「6s · 5 图层」）。 */
export function templateSummary(meta: TemplateMeta): string {
  const dur = Number.isFinite(meta.durationSec)
    ? Math.max(0, Math.round(meta.durationSec * 10) / 10)
    : 0;
  const layers = Number.isInteger(meta.layerCount) && meta.layerCount > 0 ? meta.layerCount : 1;
  return `${dur}s · ${layers} 图层`;
}

/** 面板标题行：模板总数（如「内置模板 · 8」）。 */
export function panelHeading(metas: TemplateMeta[]): string {
  return `内置模板 · ${metas.length}`;
}
