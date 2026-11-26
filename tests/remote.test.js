// node --test tests/   (npm test)
import test from "node:test";
import assert from "node:assert/strict";
import {
  isTouch, buttonAction, createPad, createWheel, wheelPixels, keyFor, pasteParts, HOLD_MS, SPEED, STEP_PX, PAGE,
  GAP_MS, TEXT_MAX, PASTE_MAX
} from "../src/lib/remote.js";

/** a pad with a clock, timers and frames the test turns by hand */
function bench(touch = true) {
  const out = [];
  const looks = [];
  let t = 0;
  let timers = [];
  let frames = [];
  const pad = createPad({
    emit: (a) => out.push(a.type === "move" ? `move ${a.dx},${a.dy}` : a.action),
    change: (s) => looks.push(s),
    touch: () => touch,
    later: (fn, ms) => {
      const id = Symbol();
      timers.push({ id, fn, at: t + ms });
      return id;
    },
    never: (id) => (timers = timers.filter((x) => x.id !== id)),
    frame: (fn) => {
      const id = Symbol();
      frames.push({ id, fn });
      return id;
    },
    unframe: (id) => (frames = frames.filter((x) => x.id !== id))
  });
  return {
    pad, out, looks,
    /** let time pass: timers that are due fire */
    wait(ms) {
      t += ms;
      const due = timers.filter((x) => x.at <= t);
      timers = timers.filter((x) => x.at > t);
      due.forEach((x) => x.fn());
    },
    /** the next animation frame */
    paint() {
      const run = frames;
      frames = [];
      run.forEach((x) => x.fn());
    }
  };
}

test("phones and tablets are touch", () => {
  assert.ok(isTouch("phone"));
  assert.ok(isTouch("tablet"));
  assert.ok(!isTouch("desktop"));
  assert.ok(!isTouch("laptop"));
  assert.ok(!isTouch(""));
});

test("a mouse's buttons are the phone's own buttons", () => {
  assert.equal(buttonAction(0, true), "singleclick");
  assert.equal(buttonAction(1, true), "middleclick"); // Home
  assert.equal(buttonAction(2, true), "backclick");
  assert.equal(buttonAction(3, true), "backclick");
  assert.equal(buttonAction(4, true), "forwardclick"); // Recent apps
  // another computer keeps a mouse's own meanings
  assert.equal(buttonAction(0, false), "singleclick");
  assert.equal(buttonAction(2, false), "rightclick");
  assert.equal(buttonAction(3, false), null);
  assert.equal(buttonAction(9, true), null);
});

test("a short press that stays put is a tap", () => {
  const b = bench();
  b.pad.down(100, 100);
  b.wait(120);
  b.pad.up();
  assert.deepEqual(b.out, ["singleclick"]);
  assert.deepEqual(b.looks.at(-1), { held: false, dragging: false });
});

test("a drag moves the pointer and is never a click", () => {
  const b = bench();
  b.pad.down(100, 100);
  b.pad.move(110, 100);
  b.pad.move(120, 104);
  assert.deepEqual(b.out, [], "moves wait for the frame");
  b.paint();
  assert.deepEqual(b.out, [`move ${20 * SPEED},${4 * SPEED}`]);
  b.wait(HOLD_MS + 100); // it went somewhere: no finger down however long it lasts
  b.pad.up();
  assert.deepEqual(b.out, [`move ${20 * SPEED},${4 * SPEED}`]);
});

test("movement that did not reach a whole pixel is kept for the next frame", () => {
  const b = bench();
  b.pad.down(0, 0);
  b.pad.move(0.2, 0); // × 1.5 = 0.3
  b.paint();
  assert.deepEqual(b.out, []);
  b.pad.move(0.6, 0); // 0.9 so far
  b.paint();
  assert.deepEqual(b.out, []);
  b.pad.move(0.8, 0); // 1.2
  b.paint();
  assert.deepEqual(b.out, ["move 1,0"]);
});

test("the move that led to a click goes out before it", () => {
  const b = bench();
  b.pad.down(50, 50);
  b.pad.move(52, 50); // inside the slop: still a click
  b.pad.up(); // no frame came in between
  assert.deepEqual(b.out, ["move 3,0", "singleclick"]);
});

test("holding still puts the finger down; the drag swipes; letting go lifts it", () => {
  const b = bench();
  b.pad.down(100, 100);
  b.wait(HOLD_MS);
  assert.deepEqual(b.out, ["singlehold"]);
  assert.deepEqual(b.looks.at(-1), { held: true, dragging: true });
  b.pad.move(100, 60);
  b.paint();
  b.pad.up();
  assert.deepEqual(b.out, ["singlehold", `move 0,${-40 * SPEED}`, "singlerelease"]);
  assert.ok(!b.pad.dragging);
});

test("a long press that never moves is finger down, finger up — never a tap as well", () => {
  const b = bench();
  b.pad.down(10, 10);
  b.wait(HOLD_MS + 600);
  b.pad.up();
  assert.deepEqual(b.out, ["singlehold", "singlerelease"]);
});

test("an unhurried click is still a click, right up to the hold", () => {
  const b = bench();
  b.pad.down(10, 10);
  b.wait(HOLD_MS - 1);
  b.pad.up();
  assert.deepEqual(b.out, ["singleclick"]);
  b.wait(HOLD_MS); // and the hold it nearly became never fires
  assert.deepEqual(b.out, ["singleclick"]);
});

test("a press that is taken away lifts the finger and clicks nothing", () => {
  const b = bench();
  b.pad.down(10, 10);
  b.pad.cancel();
  assert.deepEqual(b.out, []);
  b.pad.down(10, 10);
  b.wait(HOLD_MS);
  b.pad.cancel(); // the window lost the pointer mid-swipe
  assert.deepEqual(b.out, ["singlehold", "singlerelease"]);
  b.wait(HOLD_MS * 2); // and no timer is left to fire later
  assert.deepEqual(b.out, ["singlehold", "singlerelease"]);
});

test("the other buttons click; only the main one holds", () => {
  const b = bench();
  b.pad.down(10, 10, 2);
  b.wait(50);
  b.pad.up();
  assert.deepEqual(b.out, ["backclick"]);
  b.pad.down(10, 10, 2);
  b.wait(HOLD_MS + 50);
  b.pad.up();
  assert.deepEqual(b.out, ["backclick", "backclick"], "a held right button is no finger: it is Back, late");
  const pc = bench(false);
  pc.pad.down(1, 1, 2);
  pc.pad.up();
  assert.deepEqual(pc.out, ["rightclick"]);
});

test("moves outside a press are not the pad's", () => {
  const b = bench();
  b.pad.move(5, 5);
  b.paint();
  b.pad.up();
  assert.deepEqual(b.out, []);
});

test("a wheel is measured in pixels whatever it reports", () => {
  assert.deepEqual(wheelPixels({ deltaMode: 0, deltaX: 0, deltaY: 40 }), { x: 0, y: 40 });
  assert.deepEqual(wheelPixels({ deltaMode: 1, deltaX: 0, deltaY: 3 }), { x: 0, y: 120 });
  assert.deepEqual(wheelPixels({ deltaMode: 2, deltaX: 0, deltaY: -1 }), { x: 0, y: -800 });
});

test("on a phone the wheel turns whole pages, forward when it goes down", () => {
  let t = 0;
  const w = createWheel({ touch: () => true, now: () => t });
  assert.deepEqual(w.add(0, STEP_PX / 2), []);
  assert.deepEqual(w.add(0, STEP_PX / 2), [{ dx: 0, dy: PAGE }]);
  assert.ok(PAGE >= 500, "one message is enough for the phone to turn the page");
  assert.ok(111 >= STEP_PX, "one notch of a wheel (111px in WebKitGTK) is one page");
  // the other way starts from nothing
  t += GAP_MS;
  assert.deepEqual(w.add(0, -STEP_PX), [{ dx: 0, dy: -PAGE }]);
  // a wheel that rested forgets its half step
  t += GAP_MS;
  assert.deepEqual(w.add(0, STEP_PX - 1), []);
  t += 1000;
  assert.deepEqual(w.add(0, STEP_PX - 1), []);
});

test("a flick turns a few pages, not the whole list", () => {
  let t = 0;
  const w = createWheel({ touch: () => true, now: () => t });
  let pages = 0;
  // a touchpad flick: 3000px of travel in half a second
  for (let i = 0; i < 100; i++) {
    pages += w.add(0, 30).length;
    t += 5;
  }
  assert.equal(pages, Math.ceil(500 / GAP_MS));
  // still going when the gap is over: the next page comes at once
  t += GAP_MS;
  assert.equal(w.add(0, 1).length, 1);
  // one notch at a time, unhurried: a page each
  t += 1000;
  assert.equal(w.add(0, 111).length, 1);
  t += GAP_MS;
  assert.equal(w.add(0, 111).length, 1);
});

test("another computer gets the wheel's own travel", () => {
  const w = createWheel({ touch: () => false, now: () => 0 });
  assert.deepEqual(w.add(0, 120), [{ dx: 0, dy: -30 }]);
  assert.deepEqual(w.add(0, 0), []);
});

const key = (k, mods = {}) => keyFor({ key: k, ...mods });

test("letters and named keys go to the phone", () => {
  assert.deepEqual(key("a"), { send: true, key: "a", mods: { shift: false, ctrl: false, alt: false, meta: false }, shown: "a" });
  assert.equal(key("A", { shiftKey: true }).mods.shift, true);
  assert.equal(key("ქ").shown, "ქ");
  assert.equal(key("😀").send, true, "one character, however many code units");
  assert.equal(key("Enter").shown, "⏎");
  assert.equal(key("Backspace").shown, "⌫");
  assert.equal(key("ArrowLeft").send, true);
});

test("a combination is sent but not echoed as text", () => {
  const k = key("c", { ctrlKey: true });
  assert.equal(k.send, true);
  assert.equal(k.mods.ctrl, true);
  assert.equal(k.shown, "");
});

test("what belongs to the window stays the window's", () => {
  assert.equal(key("Shift"), null);
  assert.equal(key("Dead"), null);
  assert.equal(key("a", { isComposing: true }), null);
  assert.equal(key("3", { ctrlKey: true }), null); // the pages
  assert.equal(key("Tab"), null); // leaves the box
  assert.equal(key("v", { ctrlKey: true }), null); // this computer's paste
  assert.equal(key("V", { ctrlKey: true, shiftKey: true }), null);
  assert.equal(key("Insert", { shiftKey: true }), null);
  assert.deepEqual(key("Insert"), { send: false });
});

test("Ctrl+Tab is the phone's Tab", () => {
  const k = key("Tab", { ctrlKey: true });
  assert.equal(k.key, "Tab");
  assert.equal(k.mods.ctrl, false);
  assert.equal(key("Tab", { ctrlKey: true, shiftKey: true }).mods.shift, true);
});

test("keys the phone has no use for are swallowed, not sent", () => {
  assert.deepEqual(key("F5"), { send: false });
  assert.deepEqual(key("AudioVolumeUp"), { send: false });
});

test("a paste is runs of text and Enter keys", () => {
  assert.deepEqual(pasteParts("hello"), [{ text: "hello" }]);
  assert.deepEqual(pasteParts("a\r\nb\rc\n"), [{ text: "a" }, { key: "Enter" }, { text: "b" }, { key: "Enter" }, { text: "c" }, { key: "Enter" }]);
  assert.deepEqual(pasteParts("\n\n"), [{ key: "Enter" }, { key: "Enter" }]);
  assert.deepEqual(pasteParts("a\tb\u0007c"), [{ text: "a bc" }]);
  assert.deepEqual(pasteParts(""), []);
  assert.deepEqual(pasteParts(null), []);
});

test("a long paste is cut into packets, never through a character", () => {
  const parts = pasteParts("😀".repeat(TEXT_MAX + 5));
  assert.equal(parts.length, 2);
  assert.equal([...parts[0].text].length, TEXT_MAX);
  assert.equal(parts[1].text, "😀".repeat(5));
  const all = pasteParts("x".repeat(PASTE_MAX + 100));
  assert.equal(all.reduce((n, p) => n + p.text.length, 0), PASTE_MAX);
});
