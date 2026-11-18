import test from "node:test";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import assert from "node:assert/strict";
import { startStroke, addStrokePoint, openStrokePath, strokeBounds } from "./AnnotationStroke.ts";
import { resolveAnnotationSubmission } from "./AnnotationKeyboard.ts";
import { installBridge, ipcRenderer, ELEMENT_PICKED_CHANNEL, CANCEL_PICK_CHANNEL } from "./bridge.ts";

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
