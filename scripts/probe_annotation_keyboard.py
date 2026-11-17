"""Exercise the bundled editor's rendered theme and early keyboard isolation in CEF."""
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import sys
import time
import urllib.parse


class Cdp:
    def __init__(self, endpoint, authorization, target):
        url = urllib.parse.urlsplit(endpoint)
        self.socket = socket.create_connection((url.hostname, url.port), timeout=8)
        self.stream = self.socket.makefile("rb")
        key = base64.b64encode(os.urandom(16)).decode()
        self.socket.sendall((f"GET {url.path or '/'} HTTP/1.1\r\nHost: {url.netloc}\r\n"
                             "Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\n"
                             f"Sec-WebSocket-Key: {key}\r\nAuthorization: {authorization}\r\n\r\n").encode())
        assert b" 101 " in self.stream.readline(), "private CDP handshake failed"
        headers = {}
        while (line := self.stream.readline()) not in (b"\r\n", b""):
            name, value = line.decode().split(":", 1)
            headers[name.lower()] = value.strip()
        expected = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
        assert headers.get("sec-websocket-accept") == expected
        self.sequence = 0
        self.session = None
        self.session = self.call("Target.attachToTarget", {"targetId": target, "flatten": True})["sessionId"]

    def close(self):
        self.stream.close()
        self.socket.close()

    def send(self, payload, opcode=1):
        mask = os.urandom(4)
        length = len(payload)
        header = bytes([0x80 | opcode])
        header += bytes([0x80 | length]) if length < 126 else (b"\xfe" + struct.pack("!H", length) if length < 65536 else b"\xff" + struct.pack("!Q", length))
        self.socket.sendall(header + mask + bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))

    def read(self, size):
        data = self.stream.read(size)
        assert len(data) == size, "private CDP closed unexpectedly"
        return data

    def call(self, method, params=None):
        self.sequence += 1
        command = {"id": self.sequence, "method": method, "params": params or {}}
        if self.session:
            command["sessionId"] = self.session
        self.send(json.dumps(command).encode())
        deadline = time.monotonic() + 8
        fragments = bytearray()
        while time.monotonic() < deadline:
            self.socket.settimeout(max(0.01, deadline - time.monotonic()))
            first, second = self.read(2)
            length = second & 127
            if length == 126:
                length = struct.unpack("!H", self.read(2))[0]
            elif length == 127:
                length = struct.unpack("!Q", self.read(8))[0]
            assert length + len(fragments) < 8 * 1024 * 1024, "CDP response exceeded probe bound"
            assert not second & 128, "server unexpectedly masked a frame"
            payload = self.read(length)
            if first & 15 == 9:
                self.send(payload, 10)
                continue
            assert first & 15 in (0, 1), "private CDP closed or sent a non-text frame"
            fragments.extend(payload)
            if not first & 128:
                continue
            message = json.loads(fragments)
            fragments.clear()
            if message.get("id") == self.sequence:
                assert "error" not in message, f"{method} failed: {message.get('error')}"
                return message["result"]
        raise AssertionError(f"{method} timed out")

    def evaluate(self, expression, context=None):
        params = {"expression": expression, "returnByValue": True, "awaitPromise": True, "timeout": 5000}
        if context is not None:
            params["contextId"] = context
        result = self.call("Runtime.evaluate", params)
        assert "exceptionDetails" not in result, "annotation probe script failed"
        return result["result"].get("value")

    def node(self, tag, attribute=None, value=None):
        root = self.call("DOM.getDocument", {"depth": -1, "pierce": True})["root"]
        def visit(node):
            attrs = dict(zip(node.get("attributes", [])[::2], node.get("attributes", [])[1::2]))
            if node.get("nodeName") == tag and (attribute is None or attrs.get(attribute) == value):
                return node["nodeId"]
            for child in node.get("children", []) + node.get("shadowRoots", []):
                found = visit(child)
                if found is not None:
                    return found
            return None
        found = visit(root)
        assert found is not None, f"annotation control absent: {tag} {attribute}"
        return found

    def click(self, node):
        quad = self.call("DOM.getBoxModel", {"nodeId": node})["model"]["content"]
        point = {"x": sum(quad[::2]) / 4, "y": sum(quad[1::2]) / 4, "button": "left", "clickCount": 1}
        self.call("Input.dispatchMouseEvent", {"type": "mousePressed", **point})
        self.call("Input.dispatchMouseEvent", {"type": "mouseReleased", **point})

    def drag(self, points):
        first, last = points[0], points[-1]
        self.call("Input.dispatchMouseEvent", {"type": "mousePressed", "x": first[0], "y": first[1], "button": "left", "buttons": 1, "clickCount": 1})
        for x, y in points[1:]:
            self.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x, "y": y, "button": "left", "buttons": 1})
        self.call("Input.dispatchMouseEvent", {"type": "mouseReleased", "x": last[0], "y": last[1], "button": "left", "clickCount": 1})

    def key(self, key, text=None, modifiers=0):
        params = {"key": key, "modifiers": modifiers}
        if len(key) == 1:
            params["windowsVirtualKeyCode"] = 190 if key == "." else ord(key.upper())
        if text is not None:
            params["text"] = text
        if key in ("Enter", "Escape"):
            params["windowsVirtualKeyCode"] = 13 if key == "Enter" else 27
        if key == "a" and modifiers == 4 and sys.platform == "darwin":
            params.update(code="KeyA")
        self.call("Input.dispatchKeyEvent", {"type": "keyDown" if text is not None else "rawKeyDown", **params})
        self.call("Input.dispatchKeyEvent", {"type": "keyUp", "key": key, "modifiers": modifiers})

    def inspect(self, node, function):
        remote = self.call("DOM.resolveNode", {"nodeId": node})["object"]["objectId"]
        try:
            result = self.call("Runtime.callFunctionOn", {"objectId": remote, "functionDeclaration": function, "returnByValue": True})
            assert "exceptionDetails" not in result, "annotation computed-style probe failed"
            return result["result"]["value"]
        finally:
            self.call("Runtime.releaseObject", {"objectId": remote})

    def value(self, node):
        return self.inspect(node, "function(){return this.value}")


# Read the actual generated CSS in the real closed shadow tree. A canvas only
# normalizes computed color()/color-mix() values to portable 8-bit RGBA values.
COMPUTED_STYLE = """function(){
    const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1;
    const context = canvas.getContext('2d', {willReadFrequently:true});
    const rgba = color => {
        context.clearRect(0,0,1,1); context.fillStyle = color; context.fillRect(0,0,1,1);
        return Array.from(context.getImageData(0,0,1,1).data);
    };
    const read = node => {
        const css = getComputedStyle(node), box = node.getBoundingClientRect();
        return {background:rgba(css.backgroundColor), foreground:rgba(css.color),
            border:rgba(css.borderTopColor), borderWidth:css.borderTopWidth,
            borderStyle:css.borderTopStyle, radius:css.borderTopLeftRadius,
            padding:css.paddingLeft, gap:css.columnGap, display:css.display,
            font:css.fontFamily, fontSize:css.fontSize, width:box.width, height:box.height};
    };
    return {control:read(this), group:read(this.parentElement),
        siblings:Array.from(this.parentElement.children, read)};
}"""


def retain_annotation_capture(cdp, name):
    destination = os.environ.get("FERRITE_BROWSER_EVIDENCE")
    if destination is None:
        return
    path = Path(destination)
    path.mkdir(parents=True, exist_ok=True)
    result = cdp.call("Page.captureScreenshot", {"format": "png"})
    (path / name).write_bytes(base64.b64decode(result["data"]))


def verify_toolbar_style(cdp, selected):
    control = cdp.node("BUTTON", "title", selected)
    cdp.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": 1, "y": 599})
    style = cdp.inspect(control, COMPUTED_STYLE)
    toolbar = style["group"]
    assert toolbar["background"] == [17, 17, 17, 242], toolbar
    assert toolbar["foreground"] == [245, 245, 245, 255], toolbar
    assert toolbar["border"] == [25, 25, 25, 255], toolbar
    assert (toolbar["borderWidth"], toolbar["borderStyle"], toolbar["radius"]) == ("1px", "solid", "8px"), toolbar
    assert (toolbar["display"], toolbar["padding"], toolbar["gap"], toolbar["height"]) == ("flex", "4px", "2px", 42), toolbar
    buttons = style["siblings"]
    assert len(buttons) == 4 and all(button["height"] == 32 for button in buttons), buttons
    assert abs(toolbar["width"] - sum(button["width"] for button in buttons) - 16) < 0.1, toolbar
    active = style["control"]
    assert active["foreground"] == [52, 107, 241, 255], active
    assert active["background"][3] == 26, active
    # Low-alpha canvas readback quantizes RGB by a few units.
    assert all(abs(value - expected) <= 4 for value, expected in zip(active["background"][:3], [52, 107, 241])), active
    assert active["font"].startswith("-apple-system") and active["fontSize"] == "12px", active
    assert sum(button["background"][3] != 0 for button in buttons) == 1, buttons


def verify_style_input(cdp, node):
    style = cdp.inspect(node, COMPUTED_STYLE)["control"]
    assert style["background"] == [0, 0, 0, 255], style
    assert style["foreground"] == [245, 245, 245, 255], style
    assert style["border"] == [30, 30, 30, 255], style
    assert (style["borderWidth"], style["borderStyle"], style["radius"], style["height"], style["padding"]) == ("1px", "solid", "6px", 28, "8px"), style
    assert style["font"].startswith("ui-monospace") and style["fontSize"] == "12px", style


def verify_annotation_keyboard(endpoint, authorization, target):
    cdp = Cdp(endpoint, authorization, target)
    context = None
    original_backgrounds = None
    try:
        assert cdp.evaluate("typeof globalThis.__ferriteAnnotationKeyboard") == "undefined", "private keyboard guard leaked into page world"
        original_backgrounds = cdp.evaluate("[document.documentElement.style.background, document.body.style.background]")
        cdp.evaluate("document.documentElement.style.background = document.body.style.background = 'white'; window.fixtureKeys = []; window.trapAnnotationKeys = true")
        frame = cdp.call("Page.getFrameTree")["frameTree"]["frame"]["id"]
        context = cdp.call("Page.createIsolatedWorld", {"frameId": frame, "worldName": "ferrite-live-annotation"})["executionContextId"]
        assert cdp.evaluate("typeof globalThis.__ferriteAnnotationKeyboard?.bind", context) == "function", "document-start guard missing"
        guest = (Path(__file__).resolve().parents[1] / "crates/ferrite-pane/src/visual/live_annotation/guest.js").read_text()
        original = cdp.evaluate("document.querySelector('#paint').style.lineHeight")
        for submit in (True, False):
            cdp.evaluate(guest, context)
            verify_toolbar_style(cdp, "Select elements (V)")
            if submit:
                retain_annotation_capture(cdp, "annotation-toolbar.png")
            cdp.click(cdp.node("BUTTON", "title", "Draw freehand (D)"))
            verify_toolbar_style(cdp, "Draw freehand (D)")
            cdp.click(cdp.node("BUTTON", "title", "Select elements (V)"))
            cdp.click(cdp.node("BUTTON", "id", "paint"))
            comment = cdp.node("TEXTAREA", "placeholder", "Describe the change…")
            cdp.click(comment)
            cdp.key("x", "x")
            assert cdp.value(comment) == "x", "earlier page capture stole comment text"
            if submit:
                retain_annotation_capture(cdp, "annotation-comment.png")
            cdp.click(cdp.node("BUTTON", "aria-label", "Expand annotation editor"))
            style = cdp.node("INPUT", "placeholder", "normal / 1.4")
            verify_style_input(cdp, style)
            if submit:
                retain_annotation_capture(cdp, "annotation-expanded-css.png")
            cdp.click(style)
            cdp.key("a", modifiers=4 if sys.platform == "darwin" else 2)
            selection = cdp.inspect(style, "function(){return [this.selectionStart,this.selectionEnd,this.value.length]}")
            assert selection == [0, selection[2], selection[2]], selection
            for character in "1.4":
                cdp.key(character, character)
            assert cdp.value(style) == "1.4", {"value": cdp.value(style), "pageKeys": cdp.evaluate("window.fixtureKeys"), "focused": cdp.inspect(style, "function(){return this.getRootNode().activeElement === this}")}
            cdp.click(cdp.node("TEXTAREA", "placeholder", "Describe the change…"))
            assert cdp.evaluate("document.querySelector('#paint').style.lineHeight") == "1.4"
            cdp.key("Enter" if submit else "Escape")
            print("Annotation key sequence", "attach" if submit else "cancel", cdp.evaluate("window.fixtureKeys"), flush=True)
            event = cdp.evaluate("globalThis.__ferriteAnnotation.next()", context)
            assert event["kind"] == ("attach" if submit else "cancelled"), event["kind"]
            if submit:
                assert event["feedback"]["comment"] == "x"
            cdp.evaluate("globalThis.__ferriteAnnotation.stop()", context)
            assert cdp.evaluate("document.querySelector('#paint').style.lineHeight") == original, "annotation CSS was not restored"
        cdp.evaluate(guest, context)
        cdp.click(cdp.node("BUTTON", "title", "Draw freehand (D)"))
        cdp.drag([(80, 350), (110, 370), (140, 390)])
        cdp.drag([(230, 270), (260, 290), (290, 310)])
        cdp.click(cdp.node("BUTTON", "title", "Draw a region or marquee-select elements (R)"))
        cdp.drag([(250, 400), (290, 430), (330, 460)])
        erase = cdp.node("BUTTON", "title", "Remove an annotation target (E)")
        target_counts = "function(){const root=this.getRootNode();return [root.querySelectorAll('[data-stroke-id]').length,root.querySelectorAll('[data-region-id]').length]}"
        assert cdp.inspect(erase, target_counts) == [2, 1], "live Draw/Region targets were not rendered"
        cdp.click(erase)
        cdp.call("Input.dispatchMouseEvent", {"type": "mousePressed", "x": 110, "y": 370, "button": "left", "clickCount": 1})
        cdp.call("Input.dispatchMouseEvent", {"type": "mouseReleased", "x": 110, "y": 370, "button": "left", "clickCount": 1})
        assert cdp.inspect(erase, target_counts) == [1, 1], "Erase did not remove only the chosen stroke"
        cdp.click(cdp.node("TEXTAREA", "placeholder", "Describe the change…"))
        cdp.key("x", "x")
        retain_annotation_capture(cdp, "annotation-draw-region-erase.png")
        cdp.key("Enter")
        event = cdp.evaluate("globalThis.__ferriteAnnotation.next()", context)
        assert event["kind"] == "attach" and len(event["feedback"]["strokes"]) == 1 and len(event["feedback"]["regions"]) == 1, event
        cdp.evaluate("globalThis.__ferriteAnnotation.stop()", context)
        print("PASS live Draw/Region/Erase: rendered targets, chosen-stroke removal, and submitted target geometry")
        # Escape removes the overlay on keydown. Its later keyup belongs to the
        # restored page; earlier comment, CSS, and submission keys stay private.
        page_keys = cdp.evaluate("window.fixtureKeys")
        assert page_keys == ["Escape"], {"leakedPageKeys": page_keys}
        cdp.click(cdp.node("INPUT", "id", "value"))
        before_page_input = cdp.evaluate("window.fixtureKeys.length")
        cdp.key("x", "x")
        assert cdp.evaluate("window.fixtureKeys.length") > before_page_input, "guard retained a stopped session"
        cdp.evaluate("window.trapAnnotationKeys = false")
        print("PASS annotation theme: Code dark toolbar fill/border/geometry, selected tools, and framed CSS inputs")
        print("PASS annotation keyboard: earlier page capture cannot steal comment/CSS text, Enter or Escape; teardown restores CSS and page keys")
    finally:
        if context is not None:
            try:
                cdp.evaluate("globalThis.__ferriteAnnotation?.stop()", context)
                cdp.evaluate("window.trapAnnotationKeys = false")
            except (AssertionError, OSError):
                pass
        if original_backgrounds is not None:
            cdp.evaluate(f"[document.documentElement.style.background, document.body.style.background] = {json.dumps(original_backgrounds)}")
        cdp.close()
