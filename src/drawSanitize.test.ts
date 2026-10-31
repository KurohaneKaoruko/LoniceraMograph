/// 绘制指令护栏的单元测试：node --test 运行，无需浏览器环境。
/// 运行：npm test

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { sanitizeCmd, sanitizeCmds, sanitizeColor } from "./drawSanitize.ts";
import type { DrawCmd } from "./types";

describe("sanitizeColor", () => {
  it("合法颜色原样保留", () => {
    assert.equal(sanitizeColor("#fff", "#000000"), "#fff");
    assert.equal(sanitizeColor("#7c6cf0", "#000000"), "#7c6cf0");
    assert.equal(sanitizeColor("#7c6cf080", "#000000"), "#7c6cf080");
    assert.equal(sanitizeColor("rgba(1, 2, 3, 0.5)", "#000000"), "rgba(1, 2, 3, 0.5)");
    assert.equal(sanitizeColor("hsl(120, 50%, 50%)", "#000000"), "hsl(120, 50%, 50%)");
  });

  it("非法颜色回退缺省", () => {
    assert.equal(sanitizeColor("javascript:alert(1)", "#ffffff"), "#ffffff");
    assert.equal(sanitizeColor("url(#evil)", "#123456"), "#123456");
    assert.equal(sanitizeColor("", "#123456"), "#123456");
    assert.equal(sanitizeColor(undefined, "#123456"), "#123456");
  });
});

describe("sanitizeCmd", () => {
  it("透明度收敛到 0-1，NaN 回退 1", () => {
    const out = sanitizeCmd({
      kind: "circle", x: 0, y: 0, r: 5, fill: "#fff", opacity: 3,
    });
    assert.equal(out.opacity, 1);
    const nan = sanitizeCmd({
      kind: "circle", x: 0, y: 0, r: 5, fill: "#fff", opacity: NaN,
    });
    assert.equal(nan.opacity, 1);
  });

  it("非有限坐标回退安全值", () => {
    const out = sanitizeCmd({
      kind: "rect", x: NaN, y: Infinity, w: -10, h: 1e999 as unknown as number,
      fill: "#fff",
    });
    assert.equal(out.kind, "rect");
    if (out.kind === "rect") {
      assert.equal(out.x, 0);
      assert.equal(out.y, 0);
      assert.equal(out.w, 0);
      assert.equal(out.h, 0);
    }
  });

  it("strokeWidth 负值归零、超界收敛，stroke 保留", () => {
    const out = sanitizeCmd({
      kind: "rect", x: 0, y: 0, w: 10, h: 10, fill: "#fff",
      stroke: "#f00", strokeWidth: -5,
    });
    if (out.kind === "rect") {
      assert.equal(out.stroke, "#f00");
      assert.equal(out.strokeWidth, 0);
    }
    const big = sanitizeCmd({
      kind: "circle", x: 0, y: 0, r: 1, fill: "#fff",
      stroke: "#f00", strokeWidth: 1e9,
    });
    if (big.kind === "circle") assert.equal(big.strokeWidth, 1e4);
  });

  it("非法 stroke 颜色保留原字符串（由渲染端回退，不产生注入）", () => {
    const out = sanitizeCmd({
      kind: "circle", x: 0, y: 0, r: 1, fill: "#fff", stroke: "not-a-color",
    });
    if (out.kind === "circle") {
      // stroke 不合法时置空：渲染端跳过描边
      assert.equal(out.stroke, undefined);
    }
  });

  it("text 尺寸收敛、path 保留 d 与中心点", () => {
    const t = sanitizeCmd({
      kind: "text", x: 0, y: 0, text: "hi", size: 0, fill: "#fff",
    });
    if (t.kind === "text") assert.equal(t.size, 1);

    const p = sanitizeCmd({
      kind: "path", d: "M0 0 L10 10", fill: "#fff",
      rotation: 99999, cx: 5, cy: -5,
    });
    if (p.kind === "path") {
      assert.equal(p.d, "M0 0 L10 10");
      assert.equal(p.rotation, 1e4);
      assert.equal(p.cx, 5);
      assert.equal(p.cy, -5);
    }
  });

  it("sanitizeCmds 容忍非数组输入", () => {
    assert.deepEqual(sanitizeCmds(null as unknown as DrawCmd[]), []);
    assert.equal(sanitizeCmds([]).length, 0);
  });
});
