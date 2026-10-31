/// 绘制指令护栏（纯函数，无 DOM 依赖，便于单元测试）。
/// Rust 端的 DrawCmd 是 AI 的产出物经 serde 下发的，字段虽经 normalize
/// 清洗，仍可能在脚本路径上漏进异常值；这里在光栅化前再兜一层底，
/// 保证 Canvas 收到的每条指令数值安全（有限、非负尺寸、透明度在 0-1）。

import type { DrawCmd } from "./types";

/** 数字为有限值。 */
function finite(v: number | undefined): v is number {
  return typeof v === "number" && Number.isFinite(v);
}

/** 收敛到 [lo, hi]；非有限时返回缺省值。 */
function clampOr(v: number | undefined, lo: number, hi: number, fallback: number): number {
  return finite(v) ? Math.min(hi, Math.max(lo, v)) : fallback;
}

/** 颜色字符串：仅接受 #rgb/#rrggbb/#rrggbbaa 与 rgba()/rgb()/hsl() 常见形式，其他回退 fallback（可为空串表示"无"）。 */
export function sanitizeColor(color: string | undefined, fallback: string): string {
  if (typeof color !== "string") return fallback;
  const c = color.trim();
  if (/^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(c)) return c;
  if (/^(rgba?|hsla?)\(\s*[\d.%\s,/-]+\)$/i.test(c)) return c;
  return fallback;
}

/** 校验颜色或拒绝：合法返回颜色本身，非法返回 undefined。 */
function colorOrUndefined(color: string | undefined): string | undefined {
  if (color === undefined) return undefined;
  return sanitizeColor(color, "") || undefined;
}

/** 校验并修正一条绘制指令：异常数值回退安全值，返回新对象（不修改入参）。 */
export function sanitizeCmd(cmd: DrawCmd): DrawCmd {
  switch (cmd.kind) {
    case "rect":
      return {
        ...cmd,
        x: clampOr(cmd.x, -1e6, 1e6, 0),
        y: clampOr(cmd.y, -1e6, 1e6, 0),
        w: clampOr(cmd.w, 0, 1e6, 0),
        h: clampOr(cmd.h, 0, 1e6, 0),
        radius: cmd.radius === undefined ? undefined : clampOr(cmd.radius, 0, 1e5, 0),
        opacity: clampOr(cmd.opacity, 0, 1, 1),
        rotation: cmd.rotation === undefined ? undefined : clampOr(cmd.rotation, -1e4, 1e4, 0),
        fill: sanitizeColor(cmd.fill, "#ffffff"),
        stroke: colorOrUndefined(cmd.stroke),
        strokeWidth: cmd.strokeWidth === undefined ? undefined : clampOr(cmd.strokeWidth, 0, 1e4, 2),
      };
    case "circle":
      return {
        ...cmd,
        x: clampOr(cmd.x, -1e6, 1e6, 0),
        y: clampOr(cmd.y, -1e6, 1e6, 0),
        r: clampOr(cmd.r, 0, 1e6, 0),
        opacity: clampOr(cmd.opacity, 0, 1, 1),
        rotation: cmd.rotation === undefined ? undefined : clampOr(cmd.rotation, -1e4, 1e4, 0),
        fill: sanitizeColor(cmd.fill, "#ffffff"),
        stroke: colorOrUndefined(cmd.stroke),
        strokeWidth: cmd.strokeWidth === undefined ? undefined : clampOr(cmd.strokeWidth, 0, 1e4, 2),
      };
    case "ellipse":
      return {
        ...cmd,
        x: clampOr(cmd.x, -1e6, 1e6, 0),
        y: clampOr(cmd.y, -1e6, 1e6, 0),
        rx: clampOr(cmd.rx, 0, 1e6, 0),
        ry: clampOr(cmd.ry, 0, 1e6, 0),
        opacity: clampOr(cmd.opacity, 0, 1, 1),
        rotation: cmd.rotation === undefined ? undefined : clampOr(cmd.rotation, -1e4, 1e4, 0),
        fill: sanitizeColor(cmd.fill, "#ffffff"),
        stroke: colorOrUndefined(cmd.stroke),
        strokeWidth: cmd.strokeWidth === undefined ? undefined : clampOr(cmd.strokeWidth, 0, 1e4, 2),
      };
    case "line":
      return {
        ...cmd,
        x1: clampOr(cmd.x1, -1e6, 1e6, 0),
        y1: clampOr(cmd.y1, -1e6, 1e6, 0),
        x2: clampOr(cmd.x2, -1e6, 1e6, 0),
        y2: clampOr(cmd.y2, -1e6, 1e6, 0),
        width: clampOr(cmd.width, 0, 1e4, 2),
        opacity: clampOr(cmd.opacity, 0, 1, 1),
        stroke: sanitizeColor(cmd.stroke, "#ffffff"),
      };
    case "text":
      return {
        ...cmd,
        x: clampOr(cmd.x, -1e6, 1e6, 0),
        y: clampOr(cmd.y, -1e6, 1e6, 0),
        size: clampOr(cmd.size, 1, 1e4, 48),
        opacity: clampOr(cmd.opacity, 0, 1, 1),
        rotation: cmd.rotation === undefined ? undefined : clampOr(cmd.rotation, -1e4, 1e4, 0),
        fill: sanitizeColor(cmd.fill, "#ffffff"),
        stroke: colorOrUndefined(cmd.stroke),
        strokeWidth: cmd.strokeWidth === undefined ? undefined : clampOr(cmd.strokeWidth, 0, 1e4, 2),
      };
    case "path":
      return {
        ...cmd,
        d: typeof cmd.d === "string" ? cmd.d : "",
        opacity: clampOr(cmd.opacity, 0, 1, 1),
        rotation: cmd.rotation === undefined ? undefined : clampOr(cmd.rotation, -1e4, 1e4, 0),
        fill: sanitizeColor(cmd.fill, "#ffffff"),
        stroke: colorOrUndefined(cmd.stroke),
        strokeWidth: cmd.strokeWidth === undefined ? undefined : clampOr(cmd.strokeWidth, 0, 1e4, 2),
        cx: cmd.cx === undefined ? undefined : clampOr(cmd.cx, -1e6, 1e6, 0),
        cy: cmd.cy === undefined ? undefined : clampOr(cmd.cy, -1e6, 1e6, 0),
      };
  }
}

/** 批量护栏：逐条修正绘制指令。 */
export function sanitizeCmds(cmds: DrawCmd[]): DrawCmd[] {
  return Array.isArray(cmds) ? cmds.map(sanitizeCmd) : [];
}
