function () {
  const element = this;
  const fiberKey = Object.keys(element).find(key => key.startsWith('__reactFiber$') || key.startsWith('__reactInternalInstance$'));
  let fiber = fiberKey ? element[fiberKey] : null;
  const stack = [];
  const seen = new Set();
  let componentName = null;
  for (let depth = 0; fiber && depth < 64 && !seen.has(fiber); depth++) {
    seen.add(fiber);
    const type = fiber.type;
    const name = typeof type === 'function' ? type.displayName || type.name : typeof type === 'object' && type ? type.displayName || type.render?.displayName || type.render?.name : null;
    if (name && !componentName) componentName = String(name).slice(0, 256);
    const source = fiber._debugSource;
    if (source && typeof source.fileName === 'string') {
      stack.push({ functionName: name ? String(name).slice(0, 256) : null,
        fileName: source.fileName.slice(0, 4096),
        lineNumber: Number.isSafeInteger(source.lineNumber) && source.lineNumber > 0 ? source.lineNumber : null,
        columnNumber: Number.isSafeInteger(source.columnNumber) && source.columnNumber > 0 ? source.columnNumber : null });
    } else if (fiber._debugStack?.stack) {
      const lines = String(fiber._debugStack.stack).split('\n').slice(1, 33);
      for (const line of lines) {
        const match = line.match(/^\s*at\s+(?:(.*?)\s+\()?(.+?):(\d+):(\d+)\)?$/);
        if (match) stack.push({ functionName: match[1] || name || null, fileName: match[2].slice(0,4096), lineNumber: Number(match[3]), columnNumber: Number(match[4]) });
      }
    }
    if (stack.length >= 32) break;
    fiber = fiber._debugOwner || fiber.return;
  }
  return { componentName, stack: stack.slice(0, 32) };
}
