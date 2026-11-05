/// 工具活动行展示决策的单元测试：node --test 运行，无需浏览器环境。
/// 运行：npm test

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { toolActivityClass, toolActivityText } from "./toolActivity.ts";

describe("toolActivityClass", () => {
  it("成功行保持原 class", () => {
    assert.equal(toolActivityClass(true), "tool-activity");
  });

  it("失败行追加 failed 修饰", () => {
    assert.equal(toolActivityClass(false), "tool-activity failed");
  });
});

describe("toolActivityText", () => {
  it("成功行文案原样", () => {
    assert.equal(toolActivityText("📖 读取 scenes/intro.json", true), "📖 读取 scenes/intro.json");
  });

  it("失败行加 ✗ 前缀", () => {
    assert.equal(toolActivityText("🎬 加载 scenes/bad.json", false), "✗ 🎬 加载 scenes/bad.json");
  });

  it("空 detail 也照常处理", () => {
    assert.equal(toolActivityText("", false), "✗ ");
    assert.equal(toolActivityText("", true), "");
  });
});
