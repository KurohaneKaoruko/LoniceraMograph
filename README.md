# LoniceraMograph ❀

AI Agent 驱动的 MG（Motion Graphics）动画创作工具。**视频实时预览占据主画面，AI 对话在侧边栏**——用自然语言描述动画，AI 通过工具调用操控项目工作区：写场景 JSON / JS 生成脚本、加载预览、自查错误、迭代修正。

跨平台（Windows / macOS / Linux），基于 Tauri 2，核心逻辑全部在 Rust。许可证 [MIT](LICENSE)。

## 功能

- 🎬 **实时预览**：Canvas 渲染 + requestAnimationFrame 播放循环，播放/暂停/进度条拖拽 scrub
- 🤖 **Agent 能力**：AI 通过 function calling 操控工作区文件——
  - `list_files` / `read_file` / `write_file` / `delete_file` / `rename_file`
  - `load_scene` 加载场景 JSON 到预览；`compile_script` 执行 JS 脚本生成场景
  - `get_scene_errors` 场景体检（越界、空图层、不支持属性…），`get_scene` 读取当前场景
  - 多步循环（上限 12 步）：写文件 → 加载预览 → 自查 → 修复，工具活动实时展示在聊天中
- 📁 **工作区**：`~/文档/LoniceraMograph/`（无文档目录时回退应用数据目录），`scenes/*.json` + `scripts/*.js`；预览区右上角文件面板可点击打开
- ⚙️ **JS 脚本生成动画**：脚本跑在沙箱中（Node `--permission` / 其他引擎），提供 `scene()` / `text()` / `anim()` 等辅助函数，适合程序化粒子、波浪、随机布局
- 💬 **多 Provider**：Mock（离线演示）/ OpenAI 兼容 / Anthropic / Ollama，流式输出
- ⬇️ **一键导出**：自动播放一遍并录制为 WebM（VP9，1280×720），保存到系统下载目录

## 架构

```
用户输入 ──► Rust Agent 循环 ──► LLM（tool calls）
                │  ▲                   │
                │  └── 工具结果回传 ◄───┘
                ├──► workspace（沙箱内文件读写）
                ├──► script 沙箱（执行 JS → 场景）
                ├──► Scene::evaluate(t)（每帧求值）
                ▼
          事件流 ──► 前端：delta / tool / filesChanged / scene / done
```

- **场景即数据**：声明式 JSON（图层 + 补间动画），或由 JS 脚本程序化生成
- **求值在 Rust**：每帧由 `Scene::evaluate(time)` 计算，前端只做光栅化
- **路径安全**：文件操作全部限制在工作区内，拒绝 `..` 穿越
- **动画系统**：属性补间（位置/尺寸/透明度/旋转）× 9 种缓动函数

## 开发

```bash
npm install          # 安装前端依赖
npm run tauri dev    # 开发模式（热重载）
```

```bash
npm test             # 前端纯逻辑单元测试（node:test，无需浏览器）
npm run build        # 前端类型检查 + Vite 构建产物（Tauri 打包时自动调用）
```

| 模块（`src-tauri/src/`） | 职责 |
|------|------|
| `scene.rs`       | 场景模型、补间求值、缓动函数 |
| `agent_tools.rs` | Agent 工具定义与执行器、场景体检 |
| `script.rs`      | JS 脚本沙箱（node --permission / qjs / bun / deno） |
| `workspace.rs`   | 工作区文件操作（路径安全防护） |
| `ai/`            | Provider：openai / anthropic / ollama / mock + agent 循环 |
| `commands.rs`    | Tauri 命令（前端唯一后端入口） |
| `events.rs`      | 主进程 → 前端事件协议 |

| 模块（`src/`） | 职责 |
|------|------|
| `main.ts`           | 界面装配：预览循环、聊天流、导出、文件面板、AI 设置 |
| `renderer.ts`       | Canvas 光栅化：消费 Rust 下发的 DrawCmd |
| `types.ts`          | 与 Rust 结构对应的事件/数据类型 |
| `sceneValidate.ts`  | 场景元信息护栏（纯函数，含单元测试） |

测试：`cd src-tauri && cargo test`（9 个测试：场景求值、JSON 提取、脚本沙箱、路径安全…）；前端 `npm test`。

## 发布构建

```bash
npm run tauri build
```

（直接用 cargo 构建需带 `--features tauri/custom-protocol`，否则前端会尝试连接 dev server。）

## AI 配置

右上角选择 Provider，「设置」中填写 Base URL / API Key / 模型名（存储于 localStorage；Mock 无需配置）。Mock 模式开箱即用。Agent 文件能力需要真实 Provider（OpenAI 兼容 / Anthropic / Ollama 均已支持工具调用）。

## 场景 JSON 速览

```jsonc
{
  "name": "示例",
  "durationSec": 6,
  "width": 1280, "height": 720,
  "background": "#0e1017",
  "layers": [
    {
      "shape": { "kind": "text", "x": 640, "y": 320, "text": "标题", "size": 92,
                 "fill": "#fff", "align": "center", "baseline": "middle" },
      "animations": [
        { "prop": "opacity", "from": 0, "to": 1, "start": 0.2, "end": 1.2, "easing": "easeOutCubic" },
        { "prop": "y", "from": 360, "to": 320, "start": 0.2, "end": 1.2, "easing": "easeOutCubic" }
      ]
    }
  ]
}
```

图形：`rect / circle / ellipse / line / text / path`；缓动：`linear / easeIn / easeOut / easeInOut / easeInCubic / easeOutCubic / easeInOutCubic / easeOutBack / easeOutBounce`。

### JS 脚本示例

```js
const s = scene("演示", 5);
const t = text("你好", 640, 340, { size: 96, fill: "#fff", align: "center", baseline: "middle" });
addLayer(t, [
  anim("opacity", 0, 1, 0.2, 1.2, easing.easeOutCubic),
  anim("y", 400, 340, 0.2, 1.2, easing.easeOutCubic),
]);
for (let i = 0; i < 30; i++) {
  const c = circle(Math.random() * 1280, Math.random() * 720, 3, { fill: "#7c6cf0", opacity: 0 });
  addLayer(c, [anim("opacity", 0, 0.8, i * 0.05, i * 0.05 + 0.8, easing.easeOut)]);
}
exportScene(s);
```
