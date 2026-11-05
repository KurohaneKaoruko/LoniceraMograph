/// 渲染失败护栏决策（纯函数，无 DOM 依赖，便于单元测试）。
/// Canvas 指令抛异常会打断 requestAnimationFrame 播放循环，画面定格且无提示；
/// 这里集中「失败计数 → 是否回退上一可用场景」与「错误值 → 用户可读文案」的决策。

/// 连续失败达到该次数才回退：容忍单帧毛刺，确定性故障快速回退。
export const FALLBACK_AFTER_FAILURES = 3;

/**
 * 是否应回退到上一可用场景。
 * 必须存在上次成功绘制的帧（hasLastGood）才有回退目标；
 * 未达阈值时只记录失败，不惊动用户（瞬时毛刺不应打断播放）。
 */
export function shouldFallback(failStreak: number, hasLastGood: boolean): boolean {
  return hasLastGood && failStreak >= FALLBACK_AFTER_FAILURES;
}

/** 把 catch 到的未知错误值转成稳定文案（Error 取 message，其余 String 化）。 */
export function renderErrorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
