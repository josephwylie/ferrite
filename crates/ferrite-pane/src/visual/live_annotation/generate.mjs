import { readFileSync, writeFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
const files = ["AnnotationIds.ts", "AnnotationKeyboard.ts", "AnnotationStroke.ts", "AnnotationStyles.generated.ts", "bridge.ts", "guest.ts"];
const source = files.map(file => readFileSync(new URL(file, import.meta.url), "utf8")
  .replace(/^import .*?;\n/gms, "")
  .replace(/^export /gm, "")).join("\n");
const license = readFileSync(new URL("LICENSE", import.meta.url), "utf8");
writeFileSync(new URL("guest.js", import.meta.url), `/*! ${license} */\n(() => {\n${stripTypeScriptTypes(source)}\n})()`);
