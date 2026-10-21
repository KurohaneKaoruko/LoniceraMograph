import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { PreviewRenderer } from "./renderer";
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
let lastTs = 0;
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
    state.frame = await invoke<DrawCmd[]>("get_frame", { time: state.time });
  } catch (e) {
    console.error("get_frame failed", e);
  }
}

function paint() {
  if (!state.scene) return;
  renderer.draw(state.frame, state.scene.width, state.scene.height);
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
  div.textContent = content;
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
function appendToolActivity(detail: string) {
  const last = messagesEl.lastElementChild as HTMLElement | null;
  const host =
    last && last.classList.contains("ai") && !last.dataset.done
      ? last
      : addMessage("assistant", "");
  const line = document.createElement("span");
  line.className = "tool-activity";
  line.textContent = detail;
  host.appendChild(line);
  host.classList.remove("pending");
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

function handleStreamEvent(e: StreamEvent) {
  const last = messagesEl.lastElementChild as HTMLDivElement | null;
  switch (e.type) {
    case "delta": {
      if (last?.classList.contains("ai") && !last.dataset.done) {
        last.textContent += e.text;
        last.classList.remove("pending");
        messagesEl.scrollTop = messagesEl.scrollHeight;
      }
      break;
    }
    case "tool": {
      appendToolActivity(e.detail);
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

function loadScene(scene: SceneMeta, frame: DrawCmd[], time: number) {
  state.scene = scene;
  state.frame = frame;
  state.time = time;
  overlay.textContent = `scene: ${scene.name}`;
  renderer.resize(scene.width, scene.height);
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
    if (!isUser && !el.classList.contains("ai")) return;
    if (isUser && !el.dataset.done && el.textContent === latestUser) return;
    if (!isUser && !el.dataset.done) return; // 流式中的气泡
    const content = (el.textContent ?? "").trim();
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

    // 等待播放到结尾（播放循环会在到尾时自动 setPlaying(false)）
    await new Promise<void>((resolve) => {
      const check = () => (state.playing ? requestAnimationFrame(check) : resolve());
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
    const path = await invoke<string>("save_export", {
      filename: sceneName,
      dataBase64: b64,
    });
    addMessage("assistant", `✅ 已导出 ${sceneName}.webm（${(blob.size / 1024 / 1024).toFixed(1)} MB）\n📁 ${path}`);
    // 标记为完成态，避免进入后续对话历史
    const last = messagesEl.lastElementChild as HTMLElement | null;
    if (last?.classList.contains("ai")) last.dataset.done = "1";
  } catch (e) {
    addMessage("error", `导出失败: ${e}`);
  } finally {
    exporting = false;
    exportBtn.disabled = false;
    timeLabel.textContent = label;
    setPlaying(false);
  }
}

/// ---------------------------------------------------------------------------
/// 工作区文件面板：列表 / 点击打开（场景→预览，脚本→执行，其他→弹窗预览）
/// ---------------------------------------------------------------------------
const filesListEl = $("files-list");
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
      return;
    }
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
      const last = messagesEl.lastElementChild as HTMLElement | null;
      if (last?.classList.contains("ai")) last.dataset.done = "1";
    }
  } catch (e) {
    addMessage("error", `打开 ${path} 失败: ${e}`);
  }
}

refreshFilesBtn.addEventListener("click", () => void refreshFiles());

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
  const baseUrl = prompt("Base URL（如 https://api.openai.com/v1）", cfg.baseUrl);
  if (baseUrl === null) return;
  const apiKey = prompt("API Key", cfg.apiKey);
  if (apiKey === null) return;
  const model = prompt("模型名（如 gpt-4o / claude-sonnet-4 / qwen2.5:7b）", cfg.model);
  if (model === null) return;
  localStorage.setItem(CONFIG_KEY, JSON.stringify({ baseUrl, apiKey, model }));
  updateProviderStatus();
}

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
timelineEl.addEventListener("click", (e) => {
  if (!state.scene) return;
  const rect = timelineEl.getBoundingClientRect();
  const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
  state.time = ratio * state.scene.durationSec;
  void refreshFrame();
  paint();
});
providerSelect.addEventListener("change", () => {
  localStorage.setItem(PROVIDER_KEY, providerSelect.value);
  updateProviderStatus();
});
settingsBtn.addEventListener("click", openSettings);

providerSelect.value = localStorage.getItem(PROVIDER_KEY) ?? "mock";
updateProviderStatus();
requestAnimationFrame(tick);

// 首次启动拉默认场景，预览区不空白
void (async () => {
  try {
    const scene = await invoke<SceneMeta>("get_default_scene");
    const frame = await invoke<DrawCmd[]>("get_frame", { time: 0 });
    loadScene(scene, frame, 0);
  } catch (e) {
    console.error("初始场景加载失败", e);
  }
})();
