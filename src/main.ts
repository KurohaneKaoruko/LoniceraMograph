import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { PreviewRenderer } from "./renderer";
import { sanitizeSceneMeta } from "./sceneValidate";
import { sanitizeCmds } from "./drawSanitize";
import { renderErrorMessage, shouldFallback } from "./renderGuard";
import { toolActivityClass, toolActivityText } from "./toolActivity";
import { categoryIcon, groupByCategory, panelHeading, templateSummary, templateTitle, type TemplateMeta } from "./templates";
import type { AiConfig, ChatMessage, DrawCmd, FileEntry, SceneMeta, StreamEvent } from "./types";

/// ---------------------------------------------------------------------------
/// DOM 引用
/// ---------------------------------------------------------------------------
const $ = <T extends HTMLElement>(id: string) => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el as T;
};

const canvas = $<HTMLCanvasElement>("preview");
const overlay = $("overlay");
const messagesEl = $("messages");
const inputEl = $<HTMLTextAreaElement>("chat-input");
const sendBtn = $<HTMLButtonElement>("btn-send");
const playBtn = $<HTMLButtonElement>("btn-play");
const restartBtn = $<HTMLButtonElement>("btn-restart");
const timelineEl = $("timeline");
const timeLabel = $("time-label");
const providerSelect = $<HTMLSelectElement>("provider-select");
const settingsBtn = $<HTMLButtonElement>("btn-settings");
const providerStatus = $("provider-status");
const settingsDialog = $<HTMLDialogElement>("settings-dialog");
const settingsBaseUrl = $<HTMLInputElement>("settings-base-url");
const settingsApiKey = $<HTMLInputElement>("settings-api-key");
const settingsModel = $<HTMLInputElement>("settings-model");
const settingsSaveBtn = $<HTMLButtonElement>("btn-settings-save");
const settingsCancelBtn = $<HTMLButtonElement>("btn-settings-cancel");

const renderer = new PreviewRenderer(canvas);

/// ---------------------------------------------------------------------------
/// 状态
/// ---------------------------------------------------------------------------
interface AppState {
  scene: SceneMeta | null;
  frame: DrawCmd[];
  playing: boolean;
  time: number;
  busy: boolean;
}

const state: AppState = {
  scene: null,
  frame: [],
  playing: false,
  time: 0,
  busy: false,
};

/// ---------------------------------------------------------------------------
/// 预览循环：requestAnimationFrame 驱动播放，帧内容由 Rust 求值
/// ---------------------------------------------------------------------------
/// 渲染失败护栏：单帧绘制抛异常不能让 rAF 循环整体死亡（画面定格且无提示）。
/// 连续失败达到阈值且有上一可用场景时回退；任一帧成功即清零计数。
let lastTs = 0;
let drawFailStreak = 0;
let lastGood: { scene: SceneMeta; frame: DrawCmd[] } | null = null;

function tick(ts: number) {
  const dt = lastTs ? (ts - lastTs) / 1000 : 0;
  lastTs = ts;
  if (state.playing && state.scene) {
    state.time += dt;
    if (state.time >= state.scene.durationSec) {
      state.time = state.scene.durationSec;
      setPlaying(false);
    }
    void refreshFrame();
  }
  paint();
  requestAnimationFrame(tick);
}

/** 向 Rust 请求当前时间的帧数据（薄前端：所有求值在 Rust）。 */
async function refreshFrame() {
  if (!state.scene) return;
  try {
    state.frame = sanitizeCmds(await invoke<DrawCmd[]>("get_frame", { time: state.time }));
  } catch (e) {
    console.error("get_frame failed", e);
    showRenderError(`获取帧数据失败：${renderErrorMessage(e)}`);
  }
}

/**
 * 渲染失败呈现：把错误亮在画布遮罩上；连续失败达到阈值且存在
 * 上一可用场景时回退到它（自动暂停并提示），避免用户面对定格/黑屏。
 * 成功绘制后由 paint() 清除提示并复位计数。
 */
function showRenderError(message: string) {
  drawFailStreak += 1;
  const good = lastGood;
  if (good && shouldFallback(drawFailStreak, true)) {
    const from = state.scene?.name ?? "未知场景";
    state.scene = good.scene;
    state.frame = sanitizeCmds(good.frame);
    state.time = 0;
    setPlaying(false);
    drawFailStreak = 0;
    overlay.className = "fallback-note";
    overlay.textContent = `⚠ 场景「${from}」渲染失败，已回退到上一可用场景「${good.scene.name}」`;
  } else if (overlay.className !== "fallback-note") {
    overlay.className = "error-note";
    overlay.textContent = `⚠ ${message}`;
  }
  addMessage("error", `渲染失败：${message}`);
}

function paint() {
  if (!state.scene) return;
  try {
    renderer.draw(state.frame, state.scene.width, state.scene.height);
  } catch (e) {
    console.error("draw failed", e);
    showRenderError(renderErrorMessage(e));
    return;
  }
  drawFailStreak = 0;
  if (overlay.className === "error-note" || overlay.className === "fallback-note") {
    overlay.className = "";
    overlay.textContent = `scene: ${state.scene.name}`;
  }
  lastGood = { scene: state.scene, frame: state.frame };
  const pct =
    state.scene.durationSec > 0 ? Math.min(100, (state.time / state.scene.durationSec) * 100) : 0;
  (timelineEl.firstElementChild as HTMLElement).style.width = `${pct}%`;
  timeLabel.textContent = `${state.time.toFixed(1)} / ${state.scene.durationSec.toFixed(1)} s`;
}

function setPlaying(p: boolean) {
  state.playing = p;
  playBtn.textContent = p ? "⏸" : "▶";
}

/// ---------------------------------------------------------------------------
/// 聊天与流式事件
/// ---------------------------------------------------------------------------
function addMessage(role: "user" | "assistant" | "error", content: string): HTMLDivElement {
  document.getElementById("empty-hint")?.remove();
  const div = document.createElement("div");
  // CSS 中 AI 气泡的类名是 .ai（历史原因），这里做一层映射
  div.className = `msg ${role === "error" ? "error" : role === "assistant" ? "ai" : role}`;
  if (role === "assistant") div.classList.add("pending");
  if (role === "assistant") {
    // 正文放 .text span：流式 delta 与历史收集都只针对正文
    const span = document.createElement("span");
    span.className = "text";
    span.textContent = content;
    div.appendChild(span);
  } else {
    div.textContent = content;
  }
  messagesEl.appendChild(div);
  messagesEl.scrollTop = messagesEl.scrollHeight;
  return div;
}

let unlisten: (() => void) | null = null;

async function ensureListening() {
  if (unlisten) return;
  unlisten = await listen<StreamEvent>("stream", (ev) => handleStreamEvent(ev.payload));
}

/** 工具活动行：附到当前 AI 气泡（或新建一条），展示 agent 正在做什么。 */
function appendToolActivity(detail: string, ok = true) {
  const last = messagesEl.lastElementChild as HTMLElement | null;
  const host =
    last && last.classList.contains("ai") && !last.dataset.done
      ? last
      : addMessage("assistant", "");
  const line = document.createElement("span");
  line.className = toolActivityClass(ok);
  line.textContent = toolActivityText(detail, ok);
  host.appendChild(line);
  host.classList.remove("pending");
  ensureTextSpan(host);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** 取出（或补建）气泡内的正文 span：流式文本只写这里，不破坏工具活动等子元素。 */
function ensureTextSpan(bubble: HTMLElement): HTMLSpanElement {
  let span = bubble.querySelector<HTMLSpanElement>(":scope > .text");
  if (!span) {
    span = document.createElement("span");
    span.className = "text";
    // 仅收纳开头的散文本节点，工具活动等附加元素保持在外
    while (bubble.firstChild && bubble.firstChild.nodeType === Node.TEXT_NODE) {
      span.appendChild(bubble.firstChild);
    }
    bubble.prepend(span);
  }
  return span;
}

function handleStreamEvent(e: StreamEvent) {
  const last = messagesEl.lastElementChild as HTMLDivElement | null;
  switch (e.type) {
    case "delta": {
      if (last?.classList.contains("ai") && !last.dataset.done) {
        const span = ensureTextSpan(last);
        span.textContent += e.text;
        last.classList.remove("pending");
        messagesEl.scrollTop = messagesEl.scrollHeight;
      }
      break;
    }
    case "tool": {
      appendToolActivity(e.detail, e.ok);
      break;
    }
    case "filesChanged": {
      void refreshFiles();
      break;
    }
    case "scene": {
      loadScene(e.scene, e.frame, e.time);
      if (last?.classList.contains("ai")) {
        const link = document.createElement("span");
        link.className = "scene-link";
        link.textContent = `\n🎬 场景「${e.scene.name}」已更新（${e.scene.durationSec}s），点击播放`;
        link.addEventListener("click", () => setPlaying(true));
        last.appendChild(link);
      }
      break;
    }
    case "error": {
      state.busy = false;
      sendBtn.disabled = false;
      // 错误信息附在当前 AI 气泡后面，方便和需求对应
      if (last?.classList.contains("ai") && !last.dataset.done) {
        last.classList.remove("pending");
        last.dataset.done = "1";
        const err = document.createElement("span");
        err.style.color = "#ff9aa8";
        err.textContent = `\n⚠ ${e.message}`;
        last.appendChild(err);
      } else {
        addMessage("error", e.message);
      }
      break;
    }
    case "done": {
      if (last?.classList.contains("ai")) last.dataset.done = "1";
      state.busy = false;
      sendBtn.disabled = false;
      break;
    }
  }
}

/**
 * 气泡正文的只读视图：collectHistory 用。
 * 仅取正文 span（delta / 初始文本），跳过工具活动、场景链接、错误附注等附加节点。
 */
function bubbleText(bubble: HTMLElement): string {
  return (bubble.querySelector<HTMLSpanElement>(":scope > .text")?.textContent ?? bubble.textContent ?? "").trim();
}

/** 标记气泡为「已完成且不进入后续对话历史」（导出提示、文件预览等系统消息）。 */
function markNoHistory(bubble: HTMLElement | null) {
  if (!bubble) return;
  bubble.dataset.done = "1";
  bubble.dataset.noHistory = "1";
}

function loadScene(scene: SceneMeta, frame: DrawCmd[], time: number) {
  const meta = sanitizeSceneMeta(scene);
  state.scene = meta;
  state.frame = sanitizeCmds(frame);
  state.time = time;
  overlay.textContent = `scene: ${meta.name}`;
  renderer.resize(meta.width, meta.height);
  paint();
  setPlaying(true);
}

async function send() {
  if (state.busy) return;
  const text = inputEl.value.trim();
  if (!text) return;
  inputEl.value = "";
  addMessage("user", text);
  // 先创建 AI 气泡占位，后续 delta 事件填充内容
  addMessage("assistant", "");
  state.busy = true;
  sendBtn.disabled = true;
  try {
    await ensureListening();
    const config = getAiConfig();
    const history = collectHistory(text);
    await invoke("chat", { userMessage: text, config, history });
  } catch (e) {
    addMessage("error", `调用失败: ${e}`);
    state.busy = false;
    sendBtn.disabled = false;
  }
}

/** 组装对话历史（不含刚发送的这条，避免重复）。 */
function collectHistory(latestUser: string): ChatMessage[] {
  const out: ChatMessage[] = [];
  const nodes = messagesEl.querySelectorAll(".msg.user, .msg.ai");
  nodes.forEach((n) => {
    const el = n as HTMLElement;
    const isUser = el.classList.contains("user");
    if (el.dataset.noHistory) return; // 系统提示类气泡（导出/文件预览），不进入历史
    if (isUser && !el.dataset.done && el.textContent === latestUser) return;
    if (!isUser && !el.dataset.done) return; // 流式中的气泡
    // 只取正文：排除工具活动、场景链接等附加节点，避免污染对话历史
    const content = bubbleText(el);
    if (content) out.push({ role: isUser ? "user" : "assistant", content });
  });
  return out;
}

/// ---------------------------------------------------------------------------
/// 导出：从开头实时播放一遍，用 MediaRecorder 录制 canvas 流，
/// 完成后把 WebM 数据交给 Rust 写入系统下载目录。
/// ---------------------------------------------------------------------------
const exportBtn = $<HTMLButtonElement>("btn-export");

let exporting = false;

async function exportVideo() {
  if (!state.scene || exporting) return;
  if (typeof canvas.captureStream !== "function") {
    addMessage("error", "当前 WebView 不支持 canvas.captureStream，无法录制。");
    return;
  }
  const mime = ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"].find((m) =>
    MediaRecorder.isTypeSupported(m),
  );
  if (!mime) {
    addMessage("error", "当前 WebView 不支持 WebM 录制。");
    return;
  }

  exporting = true;
  exportBtn.disabled = true;
  const label = timeLabel.textContent;
  // 导出提示气泡（不进对话历史），随进度更新
  const progress = addMessage("assistant", "🎬 正在录制导出…");
  markNoHistory(progress);

  try {
    const stream = canvas.captureStream(60);
    const rec = new MediaRecorder(stream, { mimeType: mime, videoBitsPerSecond: 12_000_000 });
    const chunks: Blob[] = [];
    rec.ondataavailable = (ev) => ev.data.size > 0 && chunks.push(ev.data);

    const finished = new Promise<void>((resolve) => {
      rec.onstop = () => resolve();
    });

    state.time = 0;
    setPlaying(true);
    // 强制立即重绘首帧，避免录制开头黑屏
    await refreshFrame();
    paint();
    rec.start(200);

    // 等待播放到结尾（播放循环会在到尾时自动 setPlaying(false)），
    // 期间把进度写进提示气泡
    const dur = state.scene.durationSec;
    await new Promise<void>((resolve) => {
      const check = () => {
        if (!state.playing) {
          resolve();
          return;
        }
        progress.querySelector<HTMLSpanElement>(":scope > .text")!.textContent =
          `🎬 正在录制导出… ${Math.min(100, Math.round((state.time / dur) * 100))}%`;
        requestAnimationFrame(check);
      };
      requestAnimationFrame(check);
    });
    rec.stop();
    await finished;

    const blob = new Blob(chunks, { type: "video/webm" });
    if (blob.size === 0) throw new Error("录制内容为空");

    const buf = await blob.arrayBuffer();
    // 分块转 base64，避免大视频撑爆调用栈
    const bytes = new Uint8Array(buf);
    let bin = "";
    const CHUNK = 0x8000;
    for (let i = 0; i < bytes.length; i += CHUNK) {
      bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
    }
    const b64 = btoa(bin);
    const sceneName = (state.scene.name || "motion").trim();
    progress.querySelector<HTMLSpanElement>(":scope > .text")!.textContent = "💾 正在保存…";
    const path = await invoke<string>("save_export", {
      filename: sceneName,
      dataBase64: b64,
    });
    // 成功提示替换进度气泡的正文，保持界面干净
    progress.querySelector<HTMLSpanElement>(":scope > .text")!.textContent =
      `✅ 已导出 ${sceneName}.webm（${(blob.size / 1024 / 1024).toFixed(1)} MB）\n📁 ${path}`;
  } catch (e) {
    addMessage("error", `导出失败: ${e}`);
  } finally {
    exporting = false;
    exportBtn.disabled = false;
    timeLabel.textContent = label;
    setPlaying(false);
    // 播放头回到开头，便于立刻再导出/预览一遍
    if (state.scene) {
      state.time = 0;
      void refreshFrame();
      paint();
    }
  }
}

/// ---------------------------------------------------------------------------
/// 工作区文件面板：列表 / 点击打开（场景→预览，脚本→执行，其他→弹窗预览）
/// ---------------------------------------------------------------------------
const filesListEl = $("files-list");
const filesRootEl = $("files-root");
const refreshFilesBtn = $<HTMLButtonElement>("btn-refresh-files");

async function refreshFiles() {
  try {
    const entries = await invoke<FileEntry[]>("list_files", { path: null });
    filesListEl.textContent = "";
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = "（空）让 AI 帮你创建第一个场景吧";
      filesListEl.appendChild(empty);
    } else {
      for (const entry of entries) {
        if (entry.isDir) continue;
        const row = document.createElement("div");
        row.className = "file";
        row.title = entry.path;
        const icon = document.createElement("span");
        icon.className = "icon";
        icon.textContent = entry.path.endsWith(".js") ? "⚙" : entry.path.endsWith(".json") ? "🎬" : "📄";
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = entry.path;
        row.append(icon, name);
        row.addEventListener("click", () => void openWorkspaceFile(entry.path));
        filesListEl.appendChild(row);
      }
    }
    // 面板底部展示工作区绝对路径
    filesRootEl.textContent = await invoke<string>("workspace_root");
    filesRootEl.title = filesRootEl.textContent;
  } catch (e) {
    console.error("list_files failed", e);
  }
}

/** 点击文件：场景加载预览、脚本执行预览、其他类型展示内容。 */
async function openWorkspaceFile(path: string) {
  try {
    if (path.endsWith(".json")) {
      const name = await invoke<string>("open_scene", { path });
      overlay.textContent = `scene: ${name}`;
    } else if (path.endsWith(".js")) {
      const name = await invoke<string>("run_script", { path });
      overlay.textContent = `scene: ${name}`;
    } else {
      const content = await invoke<string>("read_file", { path });
      addMessage("assistant", `📄 ${path}\n\n${content.slice(0, 1200)}`);
      markNoHistory(messagesEl.lastElementChild as HTMLElement | null);
    }
  } catch (e) {
    addMessage("error", `打开 ${path} 失败: ${e}`);
  }
}

refreshFilesBtn.addEventListener("click", () => void refreshFiles());

/// ---------------------------------------------------------------------------
/// 模板面板：内置 MG 模板浏览 + 一键插入当前工程
/// ---------------------------------------------------------------------------
const tplPanelEl = $("templates-panel");
const tplListEl = $("templates-list");
const tplHeadingEl = $("templates-heading");
const tplToggleBtn = $<HTMLButtonElement>("btn-templates");

let templatesLoaded = false;

/** 拉取模板元数据并渲染分组列表；失败时面板里亮一行错误。 */
async function loadTemplates() {
  if (templatesLoaded) return;
  try {
    const metas = await invoke<TemplateMeta[]>("list_templates");
    tplHeadingEl.textContent = panelHeading(metas);
    tplListEl.textContent = "";
    if (metas.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = "（无内置模板）";
      tplListEl.appendChild(empty);
    }
    for (const group of groupByCategory(metas)) {
      const head = document.createElement("div");
      head.className = "tpl-cat";
      head.textContent = `${categoryIcon(group.category)} ${group.label}`;
      tplListEl.appendChild(head);
      for (const tpl of group.items) {
        const row = document.createElement("div");
        row.className = "tpl";
        row.title = tpl.description;
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = templateTitle(tpl);
        const sum = document.createElement("span");
        sum.className = "sum";
        sum.textContent = templateSummary(tpl);
        row.append(name, sum);
        row.addEventListener("click", () => void insertTemplate(tpl));
        tplListEl.appendChild(row);
      }
    }
    templatesLoaded = true;
  } catch (e) {
    console.error("list_templates failed", e);
    tplListEl.textContent = "";
    const err = document.createElement("div");
    err.className = "empty";
    err.textContent = `模板列表加载失败：${e}`;
    tplListEl.appendChild(err);
  }
}

/** 一键插入：加载模板到预览播放，不覆盖工作区任何文件。 */
async function insertTemplate(tpl: TemplateMeta) {
  try {
    const name = await invoke<string>("load_template", { id: tpl.id });
    overlay.textContent = `scene: ${name}（模板）`;
  } catch (e) {
    addMessage("error", `加载模板「${tpl.name}」失败: ${e}`);
  }
}

tplToggleBtn.addEventListener("click", () => {
  const open = tplPanelEl.classList.toggle("hidden") === false;
  if (open) void loadTemplates();
});

const tplCloseBtn = $<HTMLButtonElement>("btn-close-templates");
tplCloseBtn.addEventListener("click", () => tplPanelEl.classList.add("hidden"));

/// ---------------------------------------------------------------------------
/// AI 配置（localStorage 持久化）
/// ---------------------------------------------------------------------------
const PROVIDER_KEY = "lonicera.provider";
const CONFIG_KEY = "lonicera.ai-config";

function getAiConfig(): AiConfig {
  const provider = providerSelect.value as AiConfig["provider"];
  const base: AiConfig = { provider, baseUrl: "", apiKey: "", model: "" };
  const raw = localStorage.getItem(CONFIG_KEY);
  if (!raw) return base;
  try {
    return { ...base, ...(JSON.parse(raw) as Partial<AiConfig>), provider };
  } catch {
    return base;
  }
}

function updateProviderStatus() {
  const p = providerSelect.value;
  providerStatus.textContent =
    p === "mock" ? "AI: Mock（离线演示）" : `AI: ${providerSelect.selectedOptions[0].text}（未验证）`;
}

function openSettings() {
  const cfg = getAiConfig();
  settingsBaseUrl.value = cfg.baseUrl;
  settingsApiKey.value = cfg.apiKey;
  settingsModel.value = cfg.model;
  settingsDialog.showModal();
  settingsBaseUrl.focus();
}

settingsSaveBtn.addEventListener("click", () => {
  localStorage.setItem(
    CONFIG_KEY,
    JSON.stringify({
      baseUrl: settingsBaseUrl.value.trim(),
      apiKey: settingsApiKey.value.trim(),
      model: settingsModel.value.trim(),
    }),
  );
  settingsDialog.close();
  updateProviderStatus();
});
settingsCancelBtn.addEventListener("click", () => settingsDialog.close());
// 点击背景遮罩关闭
settingsDialog.addEventListener("click", (e) => {
  if (e.target === settingsDialog) settingsDialog.close();
});
settingsDialog.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
    e.preventDefault();
    settingsSaveBtn.click();
  }
});

/// ---------------------------------------------------------------------------
/// 事件绑定与启动
/// ---------------------------------------------------------------------------
sendBtn.addEventListener("click", () => void send());
exportBtn.addEventListener("click", () => void exportVideo());
inputEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    void send();
  }
});
playBtn.addEventListener("click", () => {
  if (!state.scene) return;
  if (state.playing) {
    setPlaying(false);
    void refreshFrame();
  } else {
    if (state.time >= state.scene.durationSec) state.time = 0;
    setPlaying(true);
  }
});
restartBtn.addEventListener("click", () => {
  state.time = 0;
  void refreshFrame();
  paint();
});
// 点击/拖拽进度条 scrub：按住拖动时暂停播放、实时预览，松开后恢复原播放状态
let scrubbing = false;
let playingBeforeScrub = false;

/** 把指针位置换算成时间轴时刻并刷新预览。 */
function seekToClientX(clientX: number) {
  if (!state.scene) return;
  const rect = timelineEl.getBoundingClientRect();
  const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
  state.time = ratio * state.scene.durationSec;
  void refreshFrame();
  paint();
}

timelineEl.addEventListener("pointerdown", (e) => {
  if (!state.scene) return;
  scrubbing = true;
  playingBeforeScrub = state.playing;
  setPlaying(false);
  timelineEl.setPointerCapture(e.pointerId);
  seekToClientX(e.clientX);
});
timelineEl.addEventListener("pointermove", (e) => {
  if (!scrubbing) return;
  seekToClientX(e.clientX);
});
timelineEl.addEventListener("pointerup", (e) => {
  if (!scrubbing) return;
  scrubbing = false;
  timelineEl.releasePointerCapture(e.pointerId);
  if (playingBeforeScrub && state.scene && state.time < state.scene.durationSec) {
    setPlaying(true);
  }
});
providerSelect.addEventListener("change", () => {
  localStorage.setItem(PROVIDER_KEY, providerSelect.value);
  updateProviderStatus();
});
settingsBtn.addEventListener("click", openSettings);

providerSelect.value = localStorage.getItem(PROVIDER_KEY) ?? "mock";
updateProviderStatus();
requestAnimationFrame(tick);

// 首次启动拉默认场景与工作区文件列表，预览区/文件面板不空白
void (async () => {
  try {
    const scene = await invoke<SceneMeta>("get_default_scene");
    const frame = await invoke<DrawCmd[]>("get_frame", { time: 0 });
    loadScene(scene, frame, 0);
  } catch (e) {
    console.error("初始场景加载失败", e);
  }
  await refreshFiles();
})();
