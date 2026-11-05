/// 类型定义：与 src-tauri 侧的 Rust 结构一一对应（serde camelCase 序列化）。

/** 绘制指令：Rust 每帧求值后下发，前端只负责光栅化。 */
export type DrawCmd =
  | { kind: "rect"; x: number; y: number; w: number; h: number; fill: string; radius?: number; opacity?: number; rotation?: number; stroke?: string; strokeWidth?: number }
  | { kind: "circle"; x: number; y: number; r: number; fill: string; opacity?: number; rotation?: number; stroke?: string; strokeWidth?: number }
  | { kind: "ellipse"; x: number; y: number; rx: number; ry: number; fill: string; opacity?: number; rotation?: number; stroke?: string; strokeWidth?: number }
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number; stroke: string; width: number; opacity?: number }
  | { kind: "text"; x: number; y: number; text: string; size: number; fill: string; opacity?: number; align?: string; baseline?: string; rotation?: number; stroke?: string; strokeWidth?: number }
  | { kind: "path"; d: string; fill: string; opacity?: number; rotation?: number; stroke?: string; strokeWidth?: number; cx?: number; cy?: number };

export interface SceneMeta {
  name: string;
  durationSec: number;
  width: number;
  height: number;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface AiConfig {
  provider: "mock" | "openai" | "anthropic" | "ollama";
  baseUrl: string;
  apiKey: string;
  model: string;
}

/** 工作区文件条目。 */
export interface FileEntry {
  path: string;
  isDir: boolean;
  size: number;
}

export type StreamEvent =
  | { type: "delta"; text: string }
  | { type: "tool"; name: string; detail: string; ok?: boolean }
  | { type: "filesChanged" }
  | { type: "scene"; scene: SceneMeta; frame: DrawCmd[]; time: number }
  | { type: "done" }
  | { type: "error"; message: string };
