/// 渲染护栏决策的单元测试：node --test 运行，无需浏览器环境。
/// 运行：npm test

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { FALLBACK_AFTER_FAILURES, renderErrorMessage, shouldFallback } from "./renderGuard.ts";

describe("shouldFallback", () => {
  it("连续失败未达阈值时不回退（容忍单帧毛刺）", () => {
    assert.equal(shouldFallback(0, true), false);
    assert.equal(shouldFallback(1, true), false);
    assert.equal(shouldFallback(FALLBACK_AFTER_FAILURES - 1, true), false);
  });

  it("达到阈值且有上一可用场景时回退", () => {
    assert.equal(shouldFallback(FALLBACK_AFTER_FAILURES, true), true);
    assert.equal(shouldFallback(FALLBACK_AFTER_FAILURES + 5, true), true);
  });

  it("没有上一可用场景时永不回退（无处可退）", () => {
    assert.equal(shouldFallback(FALLBACK_AFTER_FAILURES, false), false);
    assert.equal(shouldFallback(99, false), false);
  });

  it("成功帧把连续失败计数清零后不再回退", () => {
    // 模拟：失败 2 次 → 成功（清零）→ 再失败 2 次，任何时刻都不应触发
    assert.equal(shouldFallback(2, true), false);
    assert.equal(shouldFallback(2, true), false);
  });
});

describe("renderErrorMessage", () => {
  it("Error 取 message", () => {
    assert.equal(renderErrorMessage(new Error("boom")), "boom");
  });

  it("非 Error 值 String 化", () => {
    assert.equal(renderErrorMessage("字符串错误"), "字符串错误");
    assert.equal(renderErrorMessage(42), "42");
    assert.equal(renderErrorMessage(undefined), "undefined");
  });
});
