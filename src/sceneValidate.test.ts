/// 前端纯逻辑单元测试：node --test 运行，无需浏览器环境。
/// 运行：npm test

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { sanitizeSceneMeta } from "./sceneValidate.ts";
import type { SceneMeta } from "./types";

function meta(over: Partial<SceneMeta> = {}): SceneMeta {
  return { name: "示例", durationSec: 6, width: 1280, height: 720, ...over };
}

describe("sanitizeSceneMeta", () => {
  it("合法字段原样保留", () => {
    const m = meta({ name: "开场", durationSec: 3.5, width: 1920, height: 1080 });
    assert.deepEqual(sanitizeSceneMeta(m), m);
  });

  it("非法尺寸回退默认值", () => {
    const out = sanitizeSceneMeta(meta({ width: 0, height: -100, durationSec: NaN }));
    assert.equal(out.width, 1280);
    assert.equal(out.height, 720);
    // 时长非法归零：预览立即停在开头，不会出现 NaN 进度
    assert.equal(out.durationSec, 0);
  });

  it("空名称回退默认名", () => {
    assert.equal(sanitizeSceneMeta(meta({ name: "" })).name, "未命名场景");
  });

  it("Infinity / 负时长被拒绝", () => {
    const out = sanitizeSceneMeta(meta({ durationSec: Infinity, width: Infinity }));
    assert.equal(out.durationSec, 0);
    assert.equal(out.width, 1280);
  });
});
