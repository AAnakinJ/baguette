// Shareable animated demo of a finished task: the request, the quote, the result on a
// computer and a phone, and the proof. Drawn on a canvas and encoded as a looping GIF,
// entirely on this computer.
import { applyPalette, GIFEncoder, quantize } from "gifenc";

export const DEMO_W = 880;
export const DEMO_H = 550;

export interface DemoInput {
  /** Product name for the watermark (from the backend, so a rename needs no change here). */
  product: string;
  watermark: boolean;
  rtl: boolean;
  request: string;
  title: string;
  quote: string;
  verdict: string;
  verdictOk: boolean;
  stats: string;
  criteria: { text: string; ok: boolean | null }[];
  shots: { device: string; url: string }[];
  labels: { request: string; desktop: string; mobile: string; proof: string; tagline: string; criteria: string };
}

const C = { bg: "#0b0b0d", surface: "#141417", border: "#2a2a30", text: "#ececee", muted: "#a1a1aa", ok: "#3fb950", bad: "#f85149", warn: "#d29922" };
const FONT = `"Inter", "Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "Noto Sans", "Noto Sans Devanagari", "Noto Sans Arabic", "Noto Sans JP", "Noto Sans KR", sans-serif`;

/** Splits text into at most `maxLines` lines that fit `maxWidth` (ellipsis on the last one). */
export function wrapText(measure: (s: string) => number, text: string, maxWidth: number, maxLines: number): string[] {
  const words = text.replace(/\s+/g, " ").trim().split(" ");
  const lines: string[] = [];
  let cur = "";
  for (const w of words) {
    const next = cur ? `${cur} ${w}` : w;
    if (measure(next) <= maxWidth || !cur) {
      cur = next;
    } else {
      lines.push(cur);
      cur = w;
    }
  }
  if (cur) lines.push(cur);
  const shorten = (line: string) => {
    let l = line;
    while (l.length > 1 && measure(`${l}…`) > maxWidth) l = l.slice(0, -1);
    return `${l.trimEnd()}…`;
  };
  const kept = lines.slice(0, maxLines).map((l) => (measure(l) > maxWidth ? shorten(l) : l));
  if (lines.length > maxLines && !kept[maxLines - 1].endsWith("…")) kept[maxLines - 1] = shorten(kept[maxLines - 1]);
  return kept;
}

function canvas(): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const c = document.createElement("canvas");
  c.width = DEMO_W;
  c.height = DEMO_H;
  const ctx = c.getContext("2d", { willReadFrequently: true })!;
  return [c, ctx];
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function logo(ctx: CanvasRenderingContext2D, x: number, y: number, size: number, color: string) {
  const k = size / 24;
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(k, k);
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = 1.6;
  roundRect(ctx, 1, 1, 22, 22, 5);
  ctx.stroke();
  ctx.lineWidth = 1.8;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.moveTo(7.5, 16.5);
  ctx.lineTo(16.5, 7.5);
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(7, 17, 2.1, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

function base(i: DemoInput): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const [c, ctx] = canvas();
  ctx.fillStyle = C.bg;
  ctx.fillRect(0, 0, DEMO_W, DEMO_H);
  ctx.direction = i.rtl ? "rtl" : "ltr";
  ctx.textBaseline = "alphabetic";
  if (i.watermark) {
    ctx.save();
    ctx.globalAlpha = 0.9;
    ctx.font = `600 14px ${FONT}`;
    ctx.direction = "ltr";
    const w = ctx.measureText(i.product).width;
    const x = DEMO_W - 28 - w;
    logo(ctx, x - 26, DEMO_H - 40, 18, C.text);
    ctx.fillStyle = C.text;
    ctx.textAlign = "left";
    ctx.fillText(i.product, x, DEMO_H - 26);
    ctx.restore();
  }
  return [c, ctx];
}

/** x for text start in the current direction. */
const startX = (i: DemoInput, margin: number) => (i.rtl ? DEMO_W - margin : margin);

function label(ctx: CanvasRenderingContext2D, i: DemoInput, text: string, y: number) {
  ctx.font = `500 13px ${FONT}`;
  ctx.fillStyle = C.muted;
  ctx.textAlign = "start";
  ctx.fillText(text.toUpperCase(), startX(i, 48), y);
}

function sceneRequest(i: DemoInput): HTMLCanvasElement {
  const [c, ctx] = base(i);
  ctx.font = `600 34px ${FONT}`;
  const lines = wrapText((s) => ctx.measureText(s).width, `“${i.request}”`, DEMO_W - 96, 4);
  // Block: label, request lines, gap, quote. Centred vertically.
  const blockH = 30 + lines.length * 46 + 30 + (i.title ? 30 : 0);
  const top = Math.round((DEMO_H - blockH) / 2);
  label(ctx, i, i.labels.request, top);
  ctx.font = `600 34px ${FONT}`;
  ctx.fillStyle = C.text;
  ctx.textAlign = "start";
  lines.forEach((l, n) => ctx.fillText(l, startX(i, 48), top + 50 + n * 46));
  let y = top + 50 + lines.length * 46 + 16;
  ctx.font = `400 18px ${FONT}`;
  ctx.fillStyle = C.muted;
  if (i.title) {
    ctx.fillText(`→ ${i.title}`, startX(i, 48), y);
    y += 30;
  }
  ctx.fillText(i.quote, startX(i, 48), y);
  return c;
}

async function image(url: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.src = url;
  await img.decode();
  return img;
}

function fit(w: number, h: number, maxW: number, maxH: number): [number, number] {
  const k = Math.min(maxW / w, maxH / h, 1);
  return [Math.round(w * k), Math.round(h * k)];
}

function sceneDesktop(i: DemoInput, img: HTMLImageElement): HTMLCanvasElement {
  const [c, ctx] = base(i);
  label(ctx, i, i.labels.desktop, 52);
  const [w, h] = fit(img.naturalWidth, img.naturalHeight, DEMO_W - 96, DEMO_H - 130);
  const x = (DEMO_W - w) / 2;
  const y = 72;
  ctx.save();
  roundRect(ctx, x, y, w, h, 10);
  ctx.clip();
  ctx.drawImage(img, x, y, w, h);
  ctx.restore();
  ctx.strokeStyle = C.border;
  ctx.lineWidth = 1;
  roundRect(ctx, x + 0.5, y + 0.5, w - 1, h - 1, 10);
  ctx.stroke();
  return c;
}

function criteriaList(ctx: CanvasRenderingContext2D, i: DemoInput, x0: number, y0: number, width: number, max: number) {
  ctx.textAlign = "start";
  i.criteria.slice(0, max).forEach((cr, n) => {
    const y = y0 + n * 38;
    ctx.font = `700 18px ${FONT}`;
    ctx.fillStyle = cr.ok === true ? C.ok : cr.ok === false ? C.bad : C.warn;
    const mark = cr.ok === true ? "✓" : cr.ok === false ? "✗" : "•";
    const markX = i.rtl ? x0 + width : x0;
    ctx.fillText(mark, markX, y);
    ctx.font = `400 17px ${FONT}`;
    ctx.fillStyle = C.text;
    const [line] = wrapText((s) => ctx.measureText(s).width, cr.text, width - 30, 1);
    ctx.fillText(line, i.rtl ? markX - 28 : markX + 28, y);
  });
}

function scenePhone(i: DemoInput, img: HTMLImageElement): HTMLCanvasElement {
  const [c, ctx] = base(i);
  const [w, h] = fit(img.naturalWidth, img.naturalHeight, 230, DEMO_H - 110);
  const px = i.rtl ? DEMO_W - 80 - w : 80;
  const py = (DEMO_H - h) / 2 - 10;
  ctx.fillStyle = "#26262b";
  roundRect(ctx, px - 8, py - 8, w + 16, h + 16, 26);
  ctx.fill();
  ctx.save();
  roundRect(ctx, px, py, w, h, 18);
  ctx.clip();
  ctx.drawImage(img, px, py, w, h);
  ctx.restore();
  ctx.font = `500 13px ${FONT}`;
  ctx.fillStyle = C.muted;
  ctx.textAlign = "center";
  ctx.fillText(i.labels.mobile, px + w / 2, py + h + 30);
  const listX = i.rtl ? 80 : px + w + 70;
  const listW = DEMO_W - w - 230;
  const shown = Math.min(i.criteria.length, 7);
  const listTop = Math.round(DEMO_H / 2 - (shown * 38) / 2);
  ctx.textAlign = "start";
  ctx.fillText(i.labels.proof.toUpperCase(), i.rtl ? listX + listW : listX, listTop - 18);
  criteriaList(ctx, i, listX, listTop + 22, listW, 7);
  return c;
}

function sceneResult(i: DemoInput, withCriteria: boolean): HTMLCanvasElement {
  const [c, ctx] = base(i);
  const listN = withCriteria ? Math.min(i.criteria.length, 5) : 0;
  const blockH = 44 + 48 + 36 + (listN ? 40 + listN * 38 : 0);
  const top = Math.round((DEMO_H - blockH) / 2) + 36;
  ctx.font = `700 44px ${FONT}`;
  ctx.fillStyle = i.verdictOk ? C.ok : C.warn;
  ctx.textAlign = "start";
  ctx.fillText(i.verdict, startX(i, 48), top);
  ctx.font = `500 22px ${FONT}`;
  ctx.fillStyle = C.text;
  ctx.fillText(i.labels.criteria, startX(i, 48), top + 48);
  ctx.font = `400 18px ${FONT}`;
  ctx.fillStyle = C.muted;
  ctx.fillText(i.stats, startX(i, 48), top + 84);
  if (listN) criteriaList(ctx, i, 48, top + 140, DEMO_W - 96, 5);
  ctx.font = `400 16px ${FONT}`;
  ctx.fillStyle = C.muted;
  ctx.fillText(`${i.product} · ${i.labels.tagline}`, startX(i, 48), DEMO_H - 26);
  return c;
}

/** Draws the scenes and encodes the GIF. Returns the file bytes. */
export async function buildDemo(i: DemoInput): Promise<Uint8Array> {
  const scenes: [HTMLCanvasElement, number][] = [[sceneRequest(i), 2600]];
  const desktop = i.shots.find((s) => s.device === "desktop");
  const mobile = i.shots.find((s) => s.device === "mobile");
  if (desktop) scenes.push([sceneDesktop(i, await image(desktop.url)), 2600]);
  if (mobile) scenes.push([scenePhone(i, await image(mobile.url)), 3000]);
  scenes.push([sceneResult(i, !mobile), 3400]);

  const gif = GIFEncoder();
  const [, mctx] = canvas();
  const write = (src: HTMLCanvasElement | null, delay: number, first: boolean) => {
    const data = (src ? src.getContext("2d")! : mctx).getImageData(0, 0, DEMO_W, DEMO_H).data;
    const palette = quantize(data, 256);
    gif.writeFrame(applyPalette(data, palette), DEMO_W, DEMO_H, first ? { palette, delay, repeat: 0 } : { palette, delay });
  };
  scenes.forEach(([scene, delay], n) => {
    write(scene, delay, n === 0);
    // Short cross-fade to the next scene (back to the first one at the end of the loop).
    const next = scenes[(n + 1) % scenes.length][0];
    for (const a of [0.33, 0.66]) {
      mctx.globalAlpha = 1;
      mctx.drawImage(scene, 0, 0);
      mctx.globalAlpha = a;
      mctx.drawImage(next, 0, 0);
      write(null, 60, false);
    }
  });
  gif.finish();
  return gif.bytes();
}
