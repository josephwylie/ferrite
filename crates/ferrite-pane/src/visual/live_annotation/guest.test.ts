import test from "node:test";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import assert from "node:assert/strict";
import { startStroke, addStrokePoint, openStrokePath, strokeBounds } from "./AnnotationStroke.ts";
import { resolveAnnotationSubmission } from "./AnnotationKeyboard.ts";
import { annotationTheme, applyAnnotationTheme } from "./AnnotationTheme.ts";
import { previewAnnotationStyles } from "./AnnotationStyles.generated.ts";
import { installBridge, ipcRenderer, ELEMENT_PICKED_CHANNEL, CANCEL_PICK_CHANNEL } from "./bridge.ts";

test("host theme resolves every token consumed by the compiled editor CSS before mounting", () => {
  const values = new Map<string, string>();
  const host = { style: { colorScheme: "", setProperty(name: string, value: string) { values.set(name, value); } } };
  applyAnnotationTheme(host as unknown as HTMLElement, annotationTheme);
  assert.equal(host.style.colorScheme, "dark");
  for (const [, token] of previewAnnotationStyles.matchAll(/var\((--t3-[\w-]+)/g)) {
    assert.ok(values.get(token), `host theme omitted ${token}`);
    assert.doesNotMatch(values.get(token)!, /var\(/, `host theme depends on page variables: ${token}`);
  }
  assert.equal(values.get("--t3-background"), "#000000");
  assert.equal(values.get("--t3-popover"), "#111111");
  assert.equal(values.get("--t3-primary"), "#346bf1");
});

function keyboardFixture() {
  const listeners = new Map<string, Array<(event: any) => void>>();
  const window: any = {
    addEventListener(type: string, listener: (event: any) => void) {
      if (!listeners.has(type)) listeners.set(type, []);
      listeners.get(type)!.push(listener);
    },
  };
  window.top = window;
  const context: any = { window };
  runInNewContext(readFileSync(new URL("keyboard-guard.js", import.meta.url), "utf8"), context);
  return {
    guard: context.__ferriteAnnotationKeyboard,
    listeners,
    window,
    dispatch(target: object, trusted = true, type = "keydown") {
      const event = { target, type, isTrusted: trusted, stopped: false, defaultPrevented: false,
        preventDefault() { this.defaultPrevented = true; },
        stopImmediatePropagation() { this.stopped = true; } };
      for (const listener of listeners.get(type)!) {
        listener(event);
        if (event.stopped) break;
      }
      return event;
    },
  };
}

test("document-start guard takes overlay keys before later page capture without cancelling text defaults", () => {
  const fixture = keyboardFixture();
  const host = {};
  let page = 0;
  let overlay = 0;
  fixture.window.addEventListener("keydown", () => page++);
  fixture.guard.bind(host, () => overlay++, () => false);
  const event = fixture.dispatch(host);
  assert.equal(overlay, 1);
  assert.equal(page, 0);
  assert.equal(event.defaultPrevented, false);
  fixture.dispatch({});
  fixture.dispatch(host, false);
  assert.equal(overlay, 1);
  assert.equal(page, 2);
});

test("guard teardown releases exact host and repeated sessions retain only the three inert listeners", () => {
  const fixture = keyboardFixture();
  const first = {};
  const second = {};
  let calls = 0;
  const releaseFirst = fixture.guard.bind(first, () => calls++, () => false);
  const releaseSecond = fixture.guard.bind(second, () => calls++, () => false);
  releaseFirst();
  fixture.dispatch(first);
  fixture.dispatch(second);
  assert.equal(calls, 1);
  releaseSecond();
  assert.equal(fixture.dispatch(second).stopped, false);
  for (let index = 0; index < 20; index++) fixture.guard.bind({}, () => {}, () => false)();
  assert.deepEqual([...fixture.listeners.values()].map(value => value.length), [1, 1, 1]);
});

test("pending capture owns keys after focus moves to the page and teardown restores page input", () => {
  const fixture = keyboardFixture();
  const host = {};
  const page = {};
  let capturing = false;
  const pageEvents: string[] = [];
  for (const type of ["keydown", "keypress", "keyup"])
    fixture.window.addEventListener(type, () => pageEvents.push(type));
  const release = fixture.guard.bind(host, () => { capturing = true; }, () => capturing);
  fixture.dispatch(page);
  assert.deepEqual(pageEvents, ["keydown"]);
  pageEvents.length = 0;
  fixture.dispatch(host); // Submission hides the editor before the key is released.
  for (const type of ["keyup", "keydown", "keypress"]) {
    const event = fixture.dispatch(page, true, type);
    assert.equal(event.stopped, true);
    assert.equal(event.defaultPrevented, true);
  }
  assert.deepEqual(pageEvents, []);
  assert.equal(fixture.dispatch(page, false, "keyup").stopped, false);
  pageEvents.length = 0;
  release(); // The old callback still returns true; release must remove its authority.
  for (const type of ["keydown", "keypress", "keyup"]) {
    const event = fixture.dispatch(page, true, type);
    assert.equal(event.stopped, false);
    assert.equal(event.defaultPrevented, false);
  }
  assert.deepEqual(pageEvents, ["keydown", "keypress", "keyup"]);
});

test("long strokes keep settled segments while bounds cover every point", () => {
  const stroke = startStroke({ x: 0, y: 0 });
  const segments: string[] = [];
  for (let x = 1; x <= 250; x++) {
    const closed = addStrokePoint(stroke, { x, y: x % 4 });
    if (closed) segments.push(closed);
  }
  assert.equal(segments.length, 2);
  assert.equal(stroke.points.length, 251);
  assert.match(openStrokePath(stroke), /L 250 2$/);
  assert.deepEqual(strokeBounds(stroke, 4), { x: -7, y: -7, width: 264, height: 17 });
});

test("Enter attaches while Shift, composition, and unavailable Send preserve text", () => {
  const event = { key: "Enter", metaKey: false, ctrlKey: false, shiftKey: false, isComposing: false };
  assert.equal(resolveAnnotationSubmission(event, false), "attach");
  assert.equal(resolveAnnotationSubmission({ ...event, shiftKey: true }, false), null);
  assert.equal(resolveAnnotationSubmission({ ...event, isComposing: true }, false), null);
  assert.equal(resolveAnnotationSubmission({ ...event, metaKey: true }, false), null);
});

test("private event drain accepts one terminal event and teardown resolves pending host work", async () => {
  installBridge(() => {});
  const bridge = (globalThis as any).__ferriteAnnotation;
  const pending = bridge.next();
  Object.assign(globalThis, { innerWidth: 100, innerHeight: 80, devicePixelRatio: 2 });
  ipcRenderer.send(ELEMENT_PICKED_CHANNEL, { id: "a" }, { x: 1, y: 2, width: 3, height: 4 }, "send");
  ipcRenderer.send(ELEMENT_PICKED_CHANNEL, null);
  assert.deepEqual(await pending, { kind: "attach", feedback: { id: "a" }, bounds: { x: 1, y: 2, width: 3, height: 4 }, submission: "send", viewport: { width_css: 100, height_css: 80, device_scale: 2 } });
  let restored = false;
  ipcRenderer.on(CANCEL_PICK_CHANNEL, () => { restored = true; });
  const stopping = bridge.next();
  bridge.stop();
  assert.equal(restored, true);
  assert.equal((globalThis as any).__ferriteAnnotation, undefined);
  assert.deepEqual(bridge.targets(), []);
  assert.deepEqual(await stopping, { kind: "cancelled" });
});


test("source metadata comes from the retained node, not a replacement with the same selector", () => {
  const inspect = runInNewContext(`(${readFileSync(new URL("source.js", import.meta.url), "utf8")})`);
  function OriginalCard() {}
  function ReplacementCard() {}
  const retained = { id: "card", __reactFiber$test: { type: OriginalCard, _debugSource: { fileName: "/src/Original.tsx", lineNumber: 12, columnNumber: 3 } } };
  const replacement = { id: "card", __reactFiber$test: { type: ReplacementCard, _debugSource: { fileName: "/src/Replacement.tsx", lineNumber: 99, columnNumber: 1 } } };
  assert.equal(retained.id, replacement.id);
  const context = inspect.call(retained);
  assert.equal(context.componentName, "OriginalCard");
  assert.equal(context.stack[0].fileName, "/src/Original.tsx");
  assert.equal(context.stack[0].lineNumber, 12);
  assert.equal(inspect.call({}).componentName, null);
});
