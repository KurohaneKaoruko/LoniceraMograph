/// 模板面板纯逻辑的单元测试：node --test 运行，无需浏览器环境。
/// 运行：npm test

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  categoryIcon,
  categoryLabel,
  groupByCategory,
  panelHeading,
  templateSummary,
  templateTitle,
  type TemplateMeta,
} from "./templates.ts";

function meta(overrides: Partial<TemplateMeta>): TemplateMeta {
  return {
    id: "t",
    name: "模板",
    category: "title",
    description: "描述",
    durationSec: 6,
    layerCount: 3,
    ...overrides,
  };
}

describe("categoryLabel", () => {
  it("已收录分类映射中文组名", () => {
    assert.equal(categoryLabel("title"), "标题入场");
    assert.equal(categoryLabel("data"), "数据图表");
    assert.equal(categoryLabel("logo"), "Logo 展示");
    assert.equal(categoryLabel("transition"), "转场");
    assert.equal(categoryLabel("background"), "循环背景");
    assert.equal(categoryLabel("countdown"), "倒计时");
  });

  it("未知分类回退原值（不丢数据）", () => {
    assert.equal(categoryLabel("future"), "future");
    assert.equal(categoryLabel(""), "");
  });
});

describe("categoryIcon", () => {
  it("已知分类有专属图标", () => {
    assert.equal(categoryIcon("data"), "📊");
    assert.equal(categoryIcon("countdown"), "⏱");
  });

  it("未知分类回退默认图标", () => {
    assert.equal(categoryIcon("whatever"), "🎬");
  });
});

describe("groupByCategory", () => {
  it("按固定分类顺序分组，组内保持原顺序", () => {
    const input = [
      meta({ id: "b", category: "background" }),
      meta({ id: "t1", category: "title", name: "甲" }),
      meta({ id: "c", category: "countdown" }),
      meta({ id: "t2", category: "title", name: "乙" }),
    ];
    const groups = groupByCategory(input);
    assert.deepEqual(
      groups.map((g) => g.category),
      ["title", "background", "countdown"],
    );
    assert.deepEqual(
      groups[0].items.map((m) => m.id),
      ["t1", "t2"],
    );
    assert.equal(groups[0].label, "标题入场");
  });

  it("未知分类成组排在最后", () => {
    const groups = groupByCategory([
      meta({ id: "x", category: "exotic" }),
      meta({ id: "c", category: "countdown" }),
    ]);
    assert.deepEqual(
      groups.map((g) => g.category),
      ["countdown", "exotic"],
    );
    assert.equal(groups[1].label, "exotic");
  });

  it("空列表返回空分组", () => {
    assert.deepEqual(groupByCategory([]), []);
  });

  it("同一分类只出现一个组", () => {
    const groups = groupByCategory([
      meta({ id: "1", category: "transition" }),
      meta({ id: "2", category: "transition" }),
      meta({ id: "3", category: "transition" }),
    ]);
    assert.equal(groups.length, 1);
    assert.equal(groups[0].items.length, 3);
  });
});

describe("templateTitle / templateSummary", () => {
  it("标题即名称", () => {
    assert.equal(templateTitle(meta({ name: "标题入场" })), "标题入场");
  });

  it("副文案含时长与图层数", () => {
    assert.equal(templateSummary(meta({ durationSec: 6, layerCount: 5 })), "6s · 5 图层");
  });

  it("小数时长保留一位", () => {
    assert.equal(templateSummary(meta({ durationSec: 4.55, layerCount: 2 })), "4.6s · 2 图层");
  });

  it("异常元数据回退安全值", () => {
    assert.equal(templateSummary(meta({ durationSec: Number.NaN, layerCount: 0 })), "0s · 1 图层");
    assert.equal(templateSummary(meta({ durationSec: -3, layerCount: 1 })), "0s · 1 图层");
  });
});

describe("panelHeading", () => {
  it("展示模板总数", () => {
    assert.equal(panelHeading([meta({}), meta({}), meta({})]), "内置模板 · 3");
    assert.equal(panelHeading([]), "内置模板 · 0");
  });
});
