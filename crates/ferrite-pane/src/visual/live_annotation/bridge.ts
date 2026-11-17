// This module runs only in Ferrite's private isolated world.
export const ELEMENT_PICKED_CHANNEL = "picked";
export const CANCEL_PICK_CHANNEL = "cancel";
export const ANNOTATION_CAPTURED_CHANNEL = "captured";
const listeners = new Map<string, Set<(...args: unknown[]) => void>>();
let queued: unknown = null;
let waiting: ((event: unknown) => void) | null = null;
let submitted = false;
let stopped = false;
const selectedElements: Element[] = [];

function deliver(event: unknown) {
  if (stopped || submitted) return;
  submitted = true;
  if (waiting) { const resolve = waiting; waiting = null; resolve(event); }
  else queued = event;
}

export const ipcRenderer = {
  on(channel: string, listener: (...args: unknown[]) => void) {
    if (!listeners.has(channel)) listeners.set(channel, new Set());
    listeners.get(channel)!.add(listener);
  },
  off(channel: string, listener: (...args: unknown[]) => void) { listeners.get(channel)?.delete(listener); },
  send(channel: string, annotation: unknown, bounds?: unknown, submission: "attach" | "send" = "attach") {
    if (channel !== ELEMENT_PICKED_CHANNEL) return;
    if (annotation === null) { deliver({ kind: "cancelled" }); return; }
    const event = { kind: "attach", feedback: annotation, bounds, submission,
      viewport: { width_css: innerWidth, height_css: innerHeight, device_scale: devicePixelRatio } };
    if (JSON.stringify(event).length > 1024 * 1024) {
      deliver({ kind: "failed", message: "This annotation is too large. Select fewer targets or draw shorter strokes." });
    } else deliver(event);
  },
};

export function installBridge(start: (sendEnabled: boolean) => void, updateSend: (enabled: boolean) => void = () => {}) {
  const bridge = Object.freeze({
    targets() { return selectedElements; },
    setSendEnabled(enabled: boolean) { updateSend(enabled === true); },
    next() {
      if (queued) { const event = queued; queued = null; return Promise.resolve(event); }
      if (stopped) return Promise.resolve({ kind: "cancelled" });
      return new Promise((resolve) => {
        const timer = setTimeout(() => { waiting = null; resolve(null); }, 30000);
        waiting = (event) => { clearTimeout(timer); resolve(event); };
      });
    },
    stop() {
      for (const listener of listeners.get(CANCEL_PICK_CHANNEL) ?? []) listener();
      stopped = true;
      const resolve = waiting; waiting = null;
      resolve?.({ kind: "cancelled" });
      listeners.clear();
      selectedElements.length = 0;
      queued = null;
      delete (globalThis as any).__ferriteAnnotation;
    },
  });
  Object.defineProperty(globalThis, "__ferriteAnnotation", { value: bridge, configurable: true });
  start((globalThis as any).__ferriteAnnotationSendEnabled === true);
  delete (globalThis as any).__ferriteAnnotationSendEnabled;
}

export function getElementContext(element: Element) {
  selectedElements.push(element);
  const parts: string[] = [];
  let cursor: Element | null = element;
  while (cursor && parts.length < 32) {
    if (cursor.id && document.querySelectorAll(`#${CSS.escape(cursor.id)}`).length === 1) {
      parts.unshift(`#${CSS.escape(cursor.id)}`); break;
    }
    let part = cursor.tagName.toLowerCase();
    const siblings = cursor.parentElement ? Array.from(cursor.parentElement.children).filter(node => node.tagName === cursor!.tagName) : [];
    if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(cursor) + 1})`;
    parts.unshift(part); cursor = cursor.parentElement;
  }
  const style = getComputedStyle(element);
  return {
    selector: parts.join(" > "),
    htmlPreview: element.outerHTML.slice(0, 500),
    componentName: null,
    stack: [],
    styles: ["display", "position", "font-family", "font-size", "font-weight", "line-height", "color", "background-color", "border-radius", "width", "height", "padding", "margin", "gap"]
      .map(property => `${property}: ${style.getPropertyValue(property)};`).join("\n"),
  };
}
