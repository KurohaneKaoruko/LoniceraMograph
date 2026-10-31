/// Canvas 渲染器：消费 Rust 下发的 DrawCmd 列表。
/// 逻辑（场景、插值、时间轴）全部在 Rust；前端只做光栅化，保持"薄前端"。

import type { DrawCmd } from "./types";

/// 文本对齐映射：与场景 JSON 的 align / baseline 字段对应（缺省 left / alphabetic）。
const TEXT_ALIGN: Record<string, CanvasTextAlign> = { left: "left", center: "center", right: "right" };
const TEXT_BASELINE: Record<string, CanvasTextBaseline> = {
  top: "top",
  middle: "middle",
  bottom: "bottom",
  alphabetic: "alphabetic",
};

export class PreviewRenderer {
  private ctx: CanvasRenderingContext2D;

  constructor(private canvas: HTMLCanvasElement) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("无法获取 Canvas 2D 上下文");
    this.ctx = ctx;
  }

  resize(w: number, h: number) {
    this.canvas.width = w;
    this.canvas.height = h;
  }

  draw(cmds: DrawCmd[], w: number, h: number) {
    const { ctx } = this;
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, w, h);
    for (const c of cmds) {
      ctx.save();
      ctx.globalAlpha = c.opacity ?? 1;
      switch (c.kind) {
        case "rect": {
          if (c.rotation) {
            ctx.translate(c.x + c.w / 2, c.y + c.h / 2);
            ctx.rotate(c.rotation);
            ctx.translate(-(c.x + c.w / 2), -(c.y + c.h / 2));
          }
          if (c.radius && c.radius > 0) {
            ctx.beginPath();
            ctx.roundRect(c.x, c.y, c.w, c.h, c.radius);
            ctx.fillStyle = c.fill;
            ctx.fill();
          } else {
            ctx.fillStyle = c.fill;
            ctx.fillRect(c.x, c.y, c.w, c.h);
          }
          break;
        }
        case "circle": {
          ctx.beginPath();
          ctx.arc(c.x, c.y, Math.max(0, c.r), 0, Math.PI * 2);
          ctx.fillStyle = c.fill;
          ctx.fill();
          break;
        }
        case "ellipse": {
          ctx.beginPath();
          ctx.ellipse(c.x, c.y, Math.max(0, c.rx), Math.max(0, c.ry), c.rotation ?? 0, 0, Math.PI * 2);
          ctx.fillStyle = c.fill;
          ctx.fill();
          break;
        }
        case "line": {
          ctx.beginPath();
          ctx.moveTo(c.x1, c.y1);
          ctx.lineTo(c.x2, c.y2);
          ctx.strokeStyle = c.stroke;
          ctx.lineWidth = c.width;
          ctx.stroke();
          break;
        }
        case "text": {
          ctx.fillStyle = c.fill;
          ctx.font = `${c.size}px "Segoe UI", "PingFang SC", sans-serif`;
          ctx.textAlign = TEXT_ALIGN[c.align ?? ""] ?? "left";
          ctx.textBaseline = TEXT_BASELINE[c.baseline ?? ""] ?? "alphabetic";
          ctx.fillText(c.text, c.x, c.y);
          break;
        }
        case "path": {
          if (c.rotation) {
            // path 以包围盒中心旋转
            // 简化实现：不做（MVP 阶段 path 不带 rotation）
          }
          const p = new Path2D(c.d);
          ctx.fillStyle = c.fill;
          ctx.fill(p);
          break;
        }
      }
      ctx.restore();
    }
  }
}
