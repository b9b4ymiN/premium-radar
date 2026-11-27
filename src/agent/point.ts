// On-screen pointing for the agent `ui_point` tool (W6 P3-T3).
// circle/underline/highlight/box are hand-drawn via rough-notation; blink /
// pulse / spotlight / bubble are CSS (agent.css). Nothing here edits a
// target's text or content — only classes, an inline color var, and fixed
// overlay nodes appended to document.body, all undone by clearPointing().

import { annotate } from "rough-notation";
import type { PointColor, PointStep } from "./types";
import "./agent.css";

const STEP_GAP_MS = 700;
// The agent ack must land within 4 s, so squeeze the gap when many steps.
const MAX_SEQUENCE_MS = 3300;
const AUTO_CLEAR_MS = 20_000;
const SCROLL_CLEAR_PX = 200;
const VIEWPORT_MARGIN = 8;

type RoughAnnotation = ReturnType<typeof annotate>;
type RoughEffect = "circle" | "underline" | "highlight" | "box";

interface Repositionable {
  anchor: HTMLElement;
  node: HTMLElement;
  kind: "bubble" | "rect";
}

let runToken = 0;
let active = false;
let annotations: RoughAnnotation[] = [];
let nodes: HTMLElement[] = [];
let marks: HTMLElement[] = [];
// rough-notation's highlight writes inline position:relative on static
// elements and never reverts it; snapshot so clearPointing can restore.
const positionSnapshots = new Map<HTMLElement, string>();
let repositionables: Repositionable[] = [];
let abort: AbortController | null = null;
let autoTimer: ReturnType<typeof setTimeout> | null = null;
let firstTarget: HTMLElement | null = null;
let baselineTop: number | null = null;
let rafPending = false;

const pendingSleeps = new Set<{ cancel: () => void }>();

function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Resolve with `true` after `ms` if the run is still current, `false` if it
 *  was cleared in the meantime, so an in-flight point() never hangs. */
function sleep(ms: number, token: number): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const entry = { cancel: () => resolve(false) };
    pendingSleeps.add(entry);
    const done = () => {
      pendingSleeps.delete(entry);
      resolve(token === runToken);
    };
    if (ms <= 0) {
      done();
      return;
    }
    const timer = setTimeout(done, ms);
    entry.cancel = () => {
      clearTimeout(timer);
      pendingSleeps.delete(entry);
      resolve(false);
    };
  });
}

// rough-notation writes the color as an SVG attribute, where var() does not
// resolve — read the token's computed value here instead.
const COLOR_TOKENS: Record<PointColor, { token: string; fallback: string }> = {
  accent: { token: "--accent", fallback: "#4b8cf7" },
  warn: { token: "--amber", fallback: "#f0bd70" },
  danger: { token: "--red", fallback: "#f4788c" },
};

function cssColor(color: PointColor | undefined): string {
  const { token, fallback } = COLOR_TOKENS[color ?? "accent"];
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  return value || fallback;
}

function appendNode(node: HTMLElement): void {
  document.body.appendChild(node);
  nodes.push(node);
}

function positionRectNode(node: HTMLElement, anchor: HTMLElement): void {
  const r = anchor.getBoundingClientRect();
  const pad = 4;
  node.style.left = `${Math.round(r.left - pad)}px`;
  node.style.top = `${Math.round(r.top - pad)}px`;
  node.style.width = `${Math.round(r.width + pad * 2)}px`;
  node.style.height = `${Math.round(r.height + pad * 2)}px`;
}

/** Anchor a bubble next to the element, flipping above/below and clamping
 *  horizontally so it always stays inside the viewport. */
function placeBubble(bubble: HTMLElement, anchor: HTMLElement): void {
  const r = anchor.getBoundingClientRect();
  const gap = 10;
  const above = r.top - bubble.offsetHeight - gap >= VIEWPORT_MARGIN;
  let left = r.left + r.width / 2 - bubble.offsetWidth / 2;
  left = Math.min(Math.max(left, VIEWPORT_MARGIN), window.innerWidth - bubble.offsetWidth - VIEWPORT_MARGIN);
  const top = above
    ? r.top - bubble.offsetHeight - gap
    : Math.min(r.bottom + gap, window.innerHeight - bubble.offsetHeight - VIEWPORT_MARGIN);
  bubble.style.left = `${Math.round(left)}px`;
  bubble.style.top = `${Math.round(top)}px`;
}

function repositionAll(): void {
  for (const { anchor, node, kind } of repositionables) {
    if (!anchor.isConnected || !node.isConnected) continue;
    if (kind === "bubble") placeBubble(node, anchor);
    else positionRectNode(node, anchor);
  }
}

function scheduleReposition(): void {
  if (rafPending) return;
  rafPending = true;
  requestAnimationFrame(() => {
    rafPending = false;
    if (active) repositionAll();
  });
}

function onScroll(): void {
  if (!active) return;
  scheduleReposition();
  if (!firstTarget?.isConnected) {
    // Target left the DOM (page change) — keep nothing stale on screen.
    clearPointing();
    return;
  }
  if (baselineTop !== null && Math.abs(firstTarget.getBoundingClientRect().top - baselineTop) > SCROLL_CLEAR_PX) {
    clearPointing();
  }
}

function armAutoClear(first: HTMLElement): void {
  abort = new AbortController();
  firstTarget = first;
  const base = { capture: true, signal: abort.signal } as AddEventListenerOptions;
  window.addEventListener("pointerdown", () => clearPointing(), { ...base, passive: true });
  window.addEventListener("keydown", () => clearPointing(), base);
  window.addEventListener("resize", () => clearPointing(), base);
  // capture so any scrollable container inside the app is caught too
  window.addEventListener("scroll", onScroll, { ...base, passive: true });
  autoTimer = setTimeout(() => clearPointing(), AUTO_CLEAR_MS);
}

function markElement(el: HTMLElement, color: string): void {
  el.style.setProperty("--agent-mark", color);
  el.classList.add("agent-mark");
  marks.push(el);
}

function addAnnotation(el: HTMLElement, effect: RoughEffect, color: string, reduced: boolean): void {
  if (effect === "highlight") positionSnapshots.set(el, el.style.position);
  const a = annotate(el, {
    type: effect,
    color,
    strokeWidth: 2,
    padding: 4,
    animate: !reduced,
    animationDuration: reduced ? 0 : 500,
    multiline: effect === "underline" || effect === "highlight",
  });
  a.show();
  annotations.push(a);
}

function addBubble(el: HTMLElement, message: string, color: string): void {
  const bubble = document.createElement("div");
  bubble.className = "agent-bubble";
  bubble.textContent = message; // model text is never rendered as HTML
  bubble.style.setProperty("--agent-mark", color);
  appendNode(bubble);
  placeBubble(bubble, el); // measured and positioned in the same tick, before paint
  repositionables.push({ anchor: el, node: bubble, kind: "bubble" });
}

function applyStep(step: PointStep, el: HTMLElement, reduced: boolean): void {
  const color = cssColor(step.color);
  switch (step.effect) {
    case "circle":
    case "underline":
    case "highlight":
    case "box":
      addAnnotation(el, step.effect, color, reduced);
      break;
    case "blink":
      markElement(el, color);
      if (!reduced) el.classList.add("agent-blink");
      break;
    case "pulse": {
      const ring = document.createElement("div");
      ring.className = reduced ? "agent-pulse-ring" : "agent-pulse-ring agent-anim";
      ring.style.setProperty("--agent-mark", color);
      appendNode(ring);
      positionRectNode(ring, el);
      repositionables.push({ anchor: el, node: ring, kind: "rect" });
      break;
    }
    case "spotlight": {
      const hole = document.createElement("div");
      hole.className = "agent-spotlight-overlay";
      appendNode(hole);
      positionRectNode(hole, el);
      repositionables.push({ anchor: el, node: hole, kind: "rect" });
      break;
    }
    case "bubble":
      // A bubble step with no message still has to point somewhere.
      if (!step.message) addAnnotation(el, "highlight", color, reduced);
      break;
  }
  if (step.message) addBubble(el, step.message, color);
}

/**
 * Show pointing steps in order (each stays visible until cleared). Resolves
 * once the last step has started — dismissal is the user's business.
 * Any missing anchor shows nothing and fails with the missing ids.
 */
export async function point(
  steps: PointStep[],
  resolve: (anchorId: string) => HTMLElement | null,
): Promise<{ ok: boolean; detail?: string }> {
  clearPointing();
  if (steps.length === 0) return { ok: false, detail: "ไม่มีจุดที่จะชี้" };

  const els = steps.map((s) => resolve(s.anchor));
  const missing = [...new Set(steps.filter((_, i) => !els[i]).map((s) => s.anchor))];
  if (missing.length > 0) return { ok: false, detail: `ไม่พบบนจอ: ${missing.join(", ")}` };

  const token = ++runToken;
  active = true;
  const reduced = prefersReducedMotion();
  const first = els[0] as HTMLElement;

  armAutoClear(first);
  first.scrollIntoView({ block: "center", behavior: reduced ? "auto" : "smooth" });
  // Let smooth scrolling settle before pinning fixed nodes and the scroll
  // baseline; residual drift is handled by reposition-on-scroll either way.
  if (!(await sleep(reduced ? 60 : 350, token))) return { ok: false, detail: "การชี้ถูกยกเลิก" };
  baselineTop = first.isConnected ? first.getBoundingClientRect().top : null;

  const gap = reduced ? 0 : Math.min(STEP_GAP_MS, Math.floor(MAX_SEQUENCE_MS / Math.max(steps.length - 1, 1)));
  for (let i = 0; i < steps.length; i++) {
    if (i > 0 && !(await sleep(gap, token))) return { ok: false, detail: "การชี้ถูกยกเลิก" };
    if (token !== runToken) return { ok: false, detail: "การชี้ถูกยกเลิก" };
    applyStep(steps[i], els[i] as HTMLElement, reduced);
  }
  return { ok: true };
}

/** Remove every annotation, overlay, bubble and added class/attribute. Idempotent. */
export function clearPointing(): void {
  runToken++; // cancels any in-flight sequence
  active = false;
  for (const a of annotations) a.remove();
  annotations = [];
  for (const n of nodes) n.remove();
  nodes = [];
  repositionables = [];
  for (const el of marks) {
    el.classList.remove("agent-mark", "agent-blink");
    el.style.removeProperty("--agent-mark");
  }
  marks = [];
  for (const [el, position] of positionSnapshots) el.style.position = position;
  positionSnapshots.clear();
  for (const sleepEntry of pendingSleeps) sleepEntry.cancel();
  pendingSleeps.clear();
  abort?.abort();
  abort = null;
  if (autoTimer) {
    clearTimeout(autoTimer);
    autoTimer = null;
  }
  baselineTop = null;
  firstTarget = null;
}
