/** Anchor registry (W6 P3) — the UI side of "what is on screen": components
 *  register the elements the agent may point at, App/EasyApp publish which
 *  page is showing, and getScreenContext() folds both into the ScreenContext
 *  sent with every `agent_ask` and ui ack. */

import { useCallback, useEffect, useRef } from "react";
import type { ReactNode } from "react";
import type { AnchorInfo, ScreenContext } from "./types";

/** Caps the context sent to the model (types.ts: at most 60 anchors). */
const MAX_ANCHORS = 60;
/** Anchor labels are capped the same way (types.ts: label ≤ 40 chars). */
const MAX_LABEL = 40;

type AnchorEntry = { el: HTMLElement; label: string };
type ScreenMeta = Pick<ScreenContext, "mode" | "page" | "symbol" | "selected">;

const anchors = new Map<string, AnchorEntry>();

/** Last state published by the page owners; a sensible default until then. */
let screenMeta: ScreenMeta = { mode: "easy", page: "home", symbol: null, selected: null };

type Registration = { id: string; el: HTMLElement };

function release(reg: Registration): void {
  // Only the current owner may delete: when two components register the same
  // id the most recent one wins, so the older registration's unmount must not
  // remove it.
  if (anchors.get(reg.id)?.el === reg.el) anchors.delete(reg.id);
}

/** Register an element the agent may point at. Attach the returned callback
 *  as a `ref`; it re-registers on id/label change and unregisters on unmount. */
export function useAgentAnchor(id: string, label: string): (el: HTMLElement | null) => void {
  // What this hook currently has registered, so detach (React calls the ref
  // with null) only ever removes its own registration.
  const reg = useRef<Registration | null>(null);

  return useCallback(
    (el: HTMLElement | null) => {
      if (reg.current) release(reg.current);
      reg.current = el ? { id, el } : null;
      if (el) anchors.set(id, { el, label });
    },
    [id, label],
  );
}

interface AgentAnchorProps {
  id: string;
  label: string;
  /** Element to render — a plain inline span by default. The wrapper adds no
   *  styling of its own so wrapping a node never changes layout; style it via
   *  className instead (display:contents has no box to point at). */
  as?: "span" | "div";
  className?: string;
  children?: ReactNode;
}

/** Thin declarative wrapper around useAgentAnchor for static markup. */
export function AgentAnchor({ id, label, as = "span", className, children }: AgentAnchorProps) {
  const ref = useAgentAnchor(id, label);
  const Tag = as;
  return (
    <Tag ref={ref} className={className} data-agent-anchor={id}>
      {children}
    </Tag>
  );
}

/** The live element for an anchor id, or null when it left the document. */
export function getAnchorElement(id: string): HTMLElement | null {
  const entry = anchors.get(id);
  return entry?.el.isConnected ? entry.el : null;
}

/** Publish the current screen (App/EasyApp call this as the user navigates). */
export function setScreenState(partial: Partial<ScreenMeta>): void {
  // undefined means "not provided" — it must not wipe a published value
  const provided = Object.fromEntries(Object.entries(partial).filter(([, v]) => v !== undefined));
  screenMeta = { ...screenMeta, ...provided };
}

/** Effect form of setScreenState for the page owners — publishes whenever the
 *  page state changes, without re-running on a fresh caller object. */
export function useScreenState(state: Partial<ScreenMeta>): void {
  const { mode, page, symbol, selected } = state;
  useEffect(() => {
    setScreenState({ mode, page, symbol, selected });
  }, [mode, page, symbol, selected]);
}

function intersectsViewport(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  return r.bottom > 0 && r.right > 0 && r.top < window.innerHeight && r.left < window.innerWidth;
}

/** Snapshot of the screen for `agent_ask` / ui acks: published state plus the
 *  connected anchors, viewport-visible ones first, capped at MAX_ANCHORS. */
export function getScreenContext(): ScreenContext {
  const visible: AnchorInfo[] = [];
  const rest: AnchorInfo[] = [];
  for (const [id, { el, label }] of anchors) {
    if (!el.isConnected) continue;
    (intersectsViewport(el) ? visible : rest).push({ id, label: label.trim().slice(0, MAX_LABEL) });
  }
  return {
    mode: screenMeta.mode,
    page: screenMeta.page,
    symbol: screenMeta.symbol ?? null,
    selected: screenMeta.selected ?? null,
    anchors: visible.concat(rest).slice(0, MAX_ANCHORS),
  };
}

const ANCHOR_KIND = /^[a-z_]+$/;

/** Build an anchor id `kind:ref[/field]`, e.g. `tab:portfolio`,
 *  `whatif:price`, `idea:NVDA261218P00175000/premium`. An invalid kind is a
 *  programming error at the call site, so it fails fast. */
export function anchorId(kind: string, ref?: string, field?: string): string {
  if (!ANCHOR_KIND.test(kind)) {
    throw new Error(`anchorId: kind "${kind}" must match [a-z_]+`);
  }
  if (field !== undefined && ref === undefined) {
    throw new Error(`anchorId: field "${field}" needs a ref`);
  }
  if (ref === undefined) return kind;
  return field === undefined ? `${kind}:${ref}` : `${kind}:${ref}/${field}`;
}
