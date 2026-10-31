/// 前端侧的场景元信息校验（纯函数，无 DOM 依赖，便于单元测试）。
/// Rust 侧已有完整体检（agent_tools::check_scene_errors），这里只做
/// 展示前的快速护栏：时长/画布尺寸非法时回退默认值。

import type { SceneMeta } from "./types";

export const DEFAULT_WIDTH = 1280;
export const DEFAULT_HEIGHT = 720;

/** 数字为有限值且 > 0。 */
function positiveFinite(v: number | undefined): v is number {
  return typeof v === "number" && Number.isFinite(v) && v > 0;
}

/** 校验并修正场景元信息：非法字段回退默认，保证渲染层拿到可用的值。 */
export function sanitizeSceneMeta(meta: SceneMeta): SceneMeta {
  const width = positiveFinite(meta.width) ? meta.width : DEFAULT_WIDTH;
  const height = positiveFinite(meta.height) ? meta.height : DEFAULT_HEIGHT;
  const durationSec = positiveFinite(meta.durationSec) ? meta.durationSec : 0;
  return {
    name: meta.name || "未命名场景",
    durationSec,
    width,
    height,
  };
}
