// The Remote page's hands: what a press, a drag, a wheel and a key mean,
// before anything is sent. Plain functions and two small machines — no DOM,
// no Svelte — so `npm test` holds them to their word (tests/remote.test.js).
//
// What the other end makes of the words is KDE Connect's, not ours. On a
// phone (KDE Connect for Android 1.36+, MouseReceiverPlugin and
// KdeConnectAccessibilityService):
//   singleclick    a tap where the pointer is
//   singlehold     the finger goes down, and follows the pointer
//   singlerelease  the finger lifts
//   backclick      Back          middleclick   Home
//   forwardclick   Recent apps   rightclick    the menu key (Android 16+)
//   scroll         `dy` is added up; past 500 the page turns once, forward
//                  when it is positive

/** A phone or a tablet: its pointer is a finger, its buttons are Android's. */
export const isTouch = (type) => type === "phone" || type === "tablet";

export const SLOP = 6; // px a press may wander and still count as staying put
export const HOLD_MS = 450; // a press that stays put this long puts the finger down
export const SPEED = 1.5; // a small phone under a big desk mouse: faster than 1:1

/**
 * What a mouse button means on the pad — the word to send, or null.
 * On a phone the other buttons are the phone's own, as in scrcpy: right is
 * Back, the wheel button is Home, the thumb buttons are Back and Recent apps.
 */
export function buttonAction(button, touch) {
  if (button === 0) return "singleclick";
  if (button === 1) return "middleclick";
  if (touch) return button === 2 || button === 3 ? "backclick" : button === 4 ? "forwardclick" : null;
  return button === 2 ? "rightclick" : null;
}

/**
 * The pad: presses and drags in, words out.
 *
 *   emit({ type: "move", dx, dy })      the pointer moved (whole pixels)
 *   emit({ type: "do", action })        one of the words above
 *   change({ held, dragging })          what the pad should look like
 *
 * A press that stays put turns into the finger going down after HOLD_MS —
 * that one gesture is both the long press and the start of a swipe. Moves
 * are gathered until the next frame, and always go out before the click or
 * the lift that ended them.
 */
export function createPad({
  emit,
  change = () => {},
  touch = () => true,
  later = (fn, ms) => setTimeout(fn, ms),
  never = (id) => clearTimeout(id),
  frame = (fn) => requestAnimationFrame(fn),
  unframe = (id) => cancelAnimationFrame(id)
}) {
  let held = false;
  let dragging = false;
  let button = 0;
  let from = { x: 0, y: 0 }; // where the press began
  let last = { x: 0, y: 0 };
  let far = 0; // the farthest the press got from where it began
  let acc = { x: 0, y: 0 }; // movement not sent yet
  let frameId = null;
  let holdId = null;

  const tell = () => change({ held, dragging });
  const noHold = () => {
    if (holdId != null) never(holdId);
    holdId = null;
  };

  function flush() {
    if (frameId != null) unframe(frameId);
    frameId = null;
    const dx = Math.trunc(acc.x);
    const dy = Math.trunc(acc.y);
    if (!dx && !dy) return;
    acc.x -= dx;
    acc.y -= dy;
    emit({ type: "move", dx, dy });
  }

  function end(click) {
    if (!held) return;
    noHold();
    flush();
    if (dragging) emit({ type: "do", action: "singlerelease" });
    else if (click && far < SLOP) {
      // it stayed put and ended before the hold: a click, however unhurried
      const action = buttonAction(button, touch());
      if (action) emit({ type: "do", action });
    }
    held = false;
    dragging = false;
    acc = { x: 0, y: 0 };
    tell();
  }

  return {
    get held() {
      return held;
    },
    get dragging() {
      return dragging;
    },
    down(x, y, b = 0) {
      if (held) return; // a second button while one is down changes nothing
      held = true;
      dragging = false;
      button = b;
      from = { x, y };
      last = { x, y };
      far = 0;
      acc = { x: 0, y: 0 };
      if (b === 0) {
        holdId = later(() => {
          holdId = null;
          if (!held || far >= SLOP) return;
          dragging = true;
          emit({ type: "do", action: "singlehold" });
          tell();
        }, HOLD_MS);
      }
      tell();
    },
    move(x, y) {
      if (!held) return;
      acc.x += (x - last.x) * SPEED;
      acc.y += (y - last.y) * SPEED;
      last = { x, y };
      far = Math.max(far, Math.hypot(x - from.x, y - from.y));
      if (far >= SLOP) noHold(); // it went somewhere: this press is a move
      if (frameId == null) frameId = frame(flush);
    },
    /** the button came up: a click, the end of a swipe, or just the end */
    up() {
      end(true);
    },
    /** the press was taken away (the window lost it): never a click */
    cancel() {
      end(false);
    },
    flush
  };
}

export const STEP_PX = 100; // wheel travel that turns the phone's page: one notch (111 in WebKitGTK)
export const PAGE = 600; // what one turn says: past the phone's 500
export const GAP_MS = 180; // … and no sooner than this after the last, or a flick empties a list
const STALE_MS = 400; // a wheel that rested starts from nothing

/** A wheel event's travel in pixels, whatever unit it came in. */
export function wheelPixels(e) {
  const k = e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? 800 : 1;
  return { x: (e.deltaX || 0) * k, y: (e.deltaY || 0) * k };
}

/**
 * The wheel: travel in, scroll messages out — `add(x, y)` returns a list of
 * `{ dx, dy }`. A phone only turns whole pages: one per STEP_PX of travel,
 * forward when the wheel goes down, and never two within GAP_MS. Anything
 * else gets the travel itself, the way a wheel reports it (up is positive).
 */
export function createWheel({ touch = () => true, now = () => performance.now() } = {}) {
  let sum = 0;
  let at = -Infinity; // the last travel
  let turned = -Infinity; // the last page turn
  return {
    add(x, y) {
      if (!touch()) return x || y ? [{ dx: x ? -x / 4 : 0, dy: y ? -y / 4 : 0 }] : [];
      const t = now();
      if (t - at > STALE_MS || Math.sign(y) !== Math.sign(sum)) sum = 0;
      at = t;
      sum += y;
      if (Math.abs(sum) < STEP_PX) return [];
      const way = Math.sign(sum);
      if (t - turned < GAP_MS) {
        sum = way * STEP_PX; // too soon: stay ready, don't pile up
        return [];
      }
      turned = t;
      sum = 0;
      return [{ dx: 0, dy: way * PAGE }];
    }
  };
}

// keys with a name that the phone takes (kdc.rs, special_key)
const NAMED = {
  Enter: "⏎",
  Backspace: "⌫",
  Delete: "⌦",
  Tab: "⇥",
  Escape: "⎋",
  ArrowLeft: "←",
  ArrowUp: "↑",
  ArrowRight: "→",
  ArrowDown: "↓",
  PageUp: "⇞",
  PageDown: "⇟",
  Home: "⇱",
  End: "⇲"
};
// on their own these are no key at all
const SILENT = ["Shift", "Control", "Alt", "AltGraph", "Meta", "CapsLock", "NumLock", "Dead", "Compose", "Process", "Unidentified"];

/**
 * What a keydown in the typing box means:
 *   null                      not this box's: the window, or the key itself, deals with it
 *   { send: false }           swallowed — nothing the phone takes
 *   { send: true, key, mods, shown }
 * `shown` is what the echo line says ("" for a key combination).
 */
export function keyFor(e) {
  const k = e.key;
  if (e.isComposing || SILENT.includes(k)) return null;
  if (e.ctrlKey && /^[1-9]$/.test(k)) return null; // the window's pages
  if (k === "Tab" && !e.ctrlKey) return null; // Tab leaves the box, as everywhere
  // Ctrl+V and Shift+Insert are this computer's paste: left alone, the text
  // lands in the box and goes out from there
  if ((e.ctrlKey || e.metaKey) && !e.altKey && k.toLowerCase() === "v") return null;
  if (k === "Insert" && e.shiftKey) return null;
  const mods = { shift: !!e.shiftKey, ctrl: !!e.ctrlKey, alt: !!e.altKey, meta: !!e.metaKey };
  // … so Ctrl+Tab is how Tab reaches the phone: the next field there
  if (k === "Tab") return { send: true, key: k, mods: { ...mods, ctrl: false }, shown: NAMED.Tab };
  const named = Object.hasOwn(NAMED, k);
  if (!named && [...k].length !== 1) return { send: false }; // F5, the media keys …
  const combo = mods.ctrl || mods.alt || mods.meta;
  return { send: true, key: k, mods, shown: combo ? "" : named ? NAMED[k] : k };
}

export const TEXT_MAX = 500; // one packet's worth (kdc.rs, TEXT_MAX)
export const PASTE_MAX = 4000;

/**
 * Pasted text as what to send, in order: `{ text }` for a run of characters
 * (typed as one piece) and `{ key: "Enter" }` for a line break. Tabs become
 * spaces — on the phone Tab leaves the field — and what cannot be typed goes.
 */
export function pasteParts(text, max = TEXT_MAX) {
  const clean = [...String(text ?? "").replace(/\r\n?/g, "\n").replace(/\t/g, " ")]
    .filter((ch) => ch === "\n" || !/\p{Cc}/u.test(ch))
    .slice(0, PASTE_MAX)
    .join("");
  const out = [];
  clean.split("\n").forEach((line, i) => {
    if (i > 0) out.push({ key: "Enter" });
    const chars = [...line];
    for (let at = 0; at < chars.length; at += max) out.push({ text: chars.slice(at, at + max).join("") });
  });
  return out;
}
