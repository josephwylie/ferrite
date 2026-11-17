// T3 PickPreload registers keyboard capture before page scripts. This small
// counterpart runs in Ferrite's isolated world at document creation.
(() => {
  if (window !== window.top || globalThis.__ferriteAnnotationKeyboard) return;
  let active = null;
  for (const type of ["keydown", "keypress", "keyup"]) {
    window.addEventListener(type, (event) => {
      const session = active;
      if (!session || !event.isTrusted) return;
      // Capture hides the editor before the host retains its pixels. Focus can
      // return to the page in that interval, but the protected capture still
      // owns its keyboard sequence (including the submission key's keyup).
      if (session.isCapturing()) {
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      if (event.target !== session.host) return;
      session.handle(event);
      event.stopImmediatePropagation();
    }, true);
  }
  Object.defineProperty(globalThis, "__ferriteAnnotationKeyboard", {
    value: Object.freeze({
      bind(host, handle, isCapturing) {
        const session = { host, handle, isCapturing };
        active = session;
        return () => { if (active === session) active = null; };
      },
    }),
  });
})();
