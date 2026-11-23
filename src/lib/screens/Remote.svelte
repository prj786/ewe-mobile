<script>
  /**
   * Remote: this computer's mouse and keyboard, on the phone. The pad moves
   * the phone's pointer while you hold and drag; a click taps. The keyboard
   * types into whatever field is open on the phone — once the phone's
   * keyboard is switched to KDE Connect's, which only the phone can do.
   */
  import * as api from "../api.js";
  import Icon from "../components/ui/Icon.svelte";
  import { app, view, loadKeyboard } from "../state.svelte.js";
  import { fail } from "../toast.svelte.js";

  const on = $derived(view.usable);
  const name = $derived(view.device?.name ?? "the phone");
  const canPoint = $derived(view.can("remotecontrol"));
  const canType = $derived(view.can("remotekeyboard"));

  // ── the pad ───────────────────────────────────────────────────────────────
  let held = $state(false);
  let last = null; // the pointer's last position
  let moved = 0; // how far this press travelled
  let downAt = 0;
  let acc = { x: 0, y: 0 }; // movement not sent yet
  let frame = 0;
  let warned = false;

  // One at a time, in the order they happened: each call to the backend is
  // its own task there, and two sent together can arrive swapped — "ab"
  // typed as "ba", a click before the move that led to it.
  let line = Promise.resolve();
  function inOrder(send) {
    line = line.then(send).catch(report);
    return line;
  }

  function flush() {
    frame = 0;
    const dx = Math.trunc(acc.x);
    const dy = Math.trunc(acc.y);
    if (!dx && !dy) return;
    acc.x -= dx;
    acc.y -= dy;
    const dev = app.deviceId;
    inOrder(() => api.pointerMove(dev, dx, dy));
  }
  // one message for a pad that fails, not one per movement
  function report(e) {
    if (warned) return;
    warned = true;
    fail(e, "The phone didn’t take that.");
    setTimeout(() => (warned = false), 5000);
  }
  function down(e) {
    if (!on || !canPoint) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    held = true;
    last = { x: e.clientX, y: e.clientY };
    moved = 0;
    downAt = performance.now();
  }
  function move(e) {
    if (!held || !last) return;
    const dx = e.clientX - last.x;
    const dy = e.clientY - last.y;
    last = { x: e.clientX, y: e.clientY };
    moved += Math.abs(dx) + Math.abs(dy);
    // a small phone screen under a big desk mouse: a little faster than 1:1
    acc.x += dx * 1.5;
    acc.y += dy * 1.5;
    if (!frame) frame = requestAnimationFrame(flush);
  }
  function up(e) {
    if (!held) return;
    held = false;
    last = null;
    // a press that went nowhere is a click
    if (moved < 6 && performance.now() - downAt < 400) {
      const action = e.button === 2 ? "rightclick" : e.button === 1 ? "middleclick" : "singleclick";
      const dev = app.deviceId;
      inOrder(() => api.pointerDo(dev, action));
    }
  }
  function wheel(e) {
    if (!on || !canPoint) return;
    e.preventDefault();
    const dev = app.deviceId;
    const dx = -e.deltaX / 4;
    const dy = -e.deltaY / 4;
    inOrder(() => api.pointerDo(dev, "scroll", dx, dy));
  }

  // ── the keyboard ──────────────────────────────────────────────────────────
  let typing = $state(false);
  let echo = $state(""); // the last few keys, so typing has an answer

  function key(e) {
    if (!on || !canType || !app.keyboard) return;
    if (e.isComposing || ["Shift", "Control", "Alt", "Meta", "CapsLock"].includes(e.key)) return;
    // the window's own shortcuts stay the window's
    if (e.ctrlKey && /^[1-9]$/.test(e.key)) return;
    if (e.key === "Tab" && !e.ctrlKey) return; // Tab leaves the box, as everywhere
    // Ctrl+V is this computer's paste: left alone, it becomes the paste
    // event below (stopping the key would stop that too)
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "v") return;
    e.preventDefault();
    const shown = e.key.length === 1 ? e.key : { Enter: "⏎", Backspace: "⌫", Delete: "⌦" }[e.key] || "";
    if (e.key === "Backspace") echo = echo.slice(0, -1);
    else if (shown) echo = (echo + shown).slice(-32);
    const dev = app.deviceId;
    const k = e.key;
    const mods = { shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey };
    inOrder(() => api.keySend(dev, k, mods));
  }
  function paste(e) {
    if (!on || !canType || !app.keyboard) return;
    e.preventDefault();
    const t = (e.clipboardData?.getData("text") || "").replace(/\r\n?/g, "\n").slice(0, 2000);
    const dev = app.deviceId;
    for (const ch of t) inOrder(() => api.keySend(dev, ch === "\n" ? "Enter" : ch));
    echo = (echo + t.replace(/\s+/g, " ")).slice(-32);
  }

  $effect(() => {
    if (on && canType) loadKeyboard();
  });
</script>

<div class="scroll">
  <div class="page">
    <header class="ewe-headerbar ewe-headerbar--page">
      <div class="ewe-headerbar__titles">
        <h1 class="ewe-headerbar__title">Remote</h1>
        <p class="ewe-headerbar__subtitle">Use this computer’s mouse and keyboard on {name}.</p>
      </div>
    </header>

    {#if canPoint || !on}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Pointer</h2></div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="pad"
          class:is-held={held}
          class:is-disabled={!on || !canPoint}
          role="application"
          aria-label="Touchpad for {name}"
          onpointerdown={down}
          onpointermove={move}
          onpointerup={up}
          onpointercancel={up}
          onwheel={wheel}
          oncontextmenu={(e) => e.preventDefault()}
        >
          <Icon name="pointer" />
          <span class="pad__title">{held ? "Moving the pointer…" : "Hold and drag to move the pointer"}</span>
          <span class="pad__hint">Click to tap · right-click for the secondary action · scroll to scroll</span>
        </div>
        <p class="note">If nothing moves on the phone, turn on the mouse receiver for this computer in KDE Connect on the phone.</p>
      </section>
    {/if}

    {#if canType || !on}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Keyboard</h2></div>
        {#if on && !app.keyboard}
          <div class="ewe-alert" role="status">
            <Icon name="keyboard" />
            <div class="ewe-alert__body">
              <span class="ewe-alert__title">The phone isn’t taking keys yet</span>
              <span class="ewe-alert__desc">Tap a text field on the phone and switch its keyboard to KDE Connect Remote Keyboard. This page notices by itself.</span>
            </div>
          </div>
        {/if}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div
          class="keys"
          class:is-typing={typing}
          class:is-disabled={!on || !canType || !app.keyboard}
          role="textbox"
          aria-label="Type on {name}"
          aria-disabled={!on || !app.keyboard}
          tabindex={on && canType && app.keyboard ? 0 : -1}
          onfocus={() => (typing = true)}
          onblur={() => (typing = false)}
          onkeydown={key}
          onpaste={paste}
        >
          <Icon name="keyboard" />
          {#if typing}
            <span class="keys__echo">{echo || "Typing on the phone…"}</span>
          {:else}
            <span class="keys__hint">{on && app.keyboard ? "Click here, then type" : "Waiting for the phone’s keyboard"}</span>
          {/if}
        </div>
        <p class="note">What you type goes to the phone, not into this window. Paste works too.</p>
      </section>
    {/if}
  </div>
</div>
