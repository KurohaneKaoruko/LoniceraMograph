/// 工具活动行的展示决策（纯函数，无 DOM 依赖，便于单元测试）。
/// Rust 侧 Tool 事件带 ok 标记（false = 该步工具执行失败，错误已回传模型）；
/// 失败行需要 ✗ 前缀与红色样式，成功/旧事件保持原样。

/** 活动行的 class：失败加 failed 修饰（对应 index.html 的红色样式）。 */
export function toolActivityClass(ok: boolean): string {
  return ok ? "tool-activity" : "tool-activity failed";
}

/** 活动行文案：失败加 ✗ 前缀，成功原样展示。 */
export function toolActivityText(detail: string, ok: boolean): string {
  return ok ? detail : `✗ ${detail}`;
}
