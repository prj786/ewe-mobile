<script>
  /**
   * Remote: this computer's mouse and keyboard, on the phone.
   *
   * The pad moves the phone's pointer while you hold and drag; a click taps,
   * holding still puts the finger down for a swipe, the wheel turns the page,
   * and the other mouse buttons are the phone's Back, Home and Recent apps.
   * The typing box sends keys to whatever field is open on the phone.
   *
   * None of it works until the phone allows it — KDE Connect has to be an
   * accessibility service there, and until it is the phone drops everything
   * without a word. Two things do come back, and the page is built on them:
   * `app.keyboard` (the phone says it takes input) and `app.keyAcks` (it
   * typed a key). What a gesture means is in lib/remote.js.
   */
  import * as api from "../api.js";
  import Icon from "../components/ui/Icon.svelte";
  import { app, view, loadKeyboard } from "../state.svelte.js";
  import { fail } from "../toast.svelte.js";
  import { isTouch, createPad, createWheel, wheelPixels, keyFor, pasteParts } from "../remote.js";

  const on = $derived(view.usable);
  const name = $derived(view.device?.name ?? "the phone");
  const touch = $derived(isTouch(view.device?.type ?? "phone"));
  const canPoint = $derived(view.can("remotecontrol"));
  const canType = $derived(view.can("remotekeyboard"));
  // the phone has not said it takes input: on Android that is one switch
  const setup = $derived(on && touch && (canPoint || canType) && !app.keyboard);
  const padOn = $derived(on && canPoint);
  const keysOn = $derived(on && canType && app.keyboard);

  // One at a time, in the order they happened: each call to the backend is
  // its own task there, and two sent together can arrive swapped — "ab"
  // typed as "ba", a click before the move that led to it.
  let line = Promise.resolve();
  function inOrder(send) {
    line = line.then(send).catch(report);
    return line;
  }
  // one message for a pad that fails, not one per movement
  let warned = false;
  function report(e) {
    if (warned) return;
    warned = true;
    fail(e, "The phone didn’t take that.");
    setTimeout(() => (warned = false), 5000);
  }
  const point = (action) => {
    const dev = app.deviceId;
    inOrder(() => api.pointerDo(dev, action));
  };

  // ── the pad ───────────────────────────────────────────────────────────────
  let look = $state({ held: false, dragging: false });
  const pad = createPad({
    emit(a) {
      if (a.type === "move") {
        const dev = app.deviceId;
        inOrder(() => api.pointerMove(dev, a.dx, a.dy));
      } else point(a.action);
    },
    change: (s) => (look = s),
    touch: () => touch
  });
  const turns = createWheel({ touch: () => touch });

  function down(e) {
    if (!padOn) return;
    e.preventDefault();
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* a pointer that is already gone: the press ends with it */
    }
    pad.down(e.clientX, e.clientY, e.button);
  }
  function wheel(e) {
    if (!padOn) return;
    e.preventDefault();
    const { x, y } = wheelPixels(e);
    const dev = app.deviceId;
    for (const s of turns.add(x, y)) inOrder(() => api.pointerDo(dev, "scroll", s.dx, s.dy));
  }
  // a finger left down on the phone when this page goes away would stay down
  $effect(() => () => pad.cancel());

  // ── the keyboard ──────────────────────────────────────────────────────────
  let typing = $state(false);
  let echo = $state(""); // the last few keys, so typing has an answer
  let untaken = $state(false); // keys went out and the phone answered none
  let composing = false;
  const tail = (s) => [...s].slice(-32).join("");

  // The phone answers each key it typed. One that gets no answer went
  // nowhere: no text field is open there.
  function expectAnswer() {
    const before = app.keyAcks;
    const dev = app.deviceId;
    setTimeout(() => {
      if (dev === app.deviceId && app.keyAcks === before) untaken = true;
    }, 1500);
  }
  $effect(() => {
    void app.keyAcks;
    untaken = false;
  });

  function sendText(text) {
    const parts = pasteParts(text);
    if (!parts.length) return;
    const dev = app.deviceId;
    for (const p of parts) inOrder(() => (p.text != null ? api.textSend(dev, p.text) : api.keySend(dev, p.key)));
    echo = tail(echo + parts.map((p) => p.text ?? "⏎").join("").replace(/\s+/g, " "));
    expectAnswer();
  }
  function key(e) {
    if (!keysOn) return;
    const k = keyFor(e);
    if (!k) return;
    e.preventDefault();
    if (!k.send) return;
    if (k.key === "Backspace" && !k.mods.ctrl) echo = [...echo].slice(0, -1).join("");
    else if (k.shown) echo = tail(echo + k.shown);
    const dev = app.deviceId;
    inOrder(() => api.keySend(dev, k.key, k.mods));
    expectAnswer();
  }
  // What the keydown did not take arrives here, as text the box was given:
  // a paste (Ctrl+V, Shift+Insert, the middle button — however it came), a
  // letter finished with a dead key or an input method, an emoji from the
  // picker. The paste is let happen rather than read off its event: WebKitGTK
  // hands that event no text for a pasted selection. The box itself stays
  // empty — the text is on the phone.
  function input(e) {
    if (composing) return;
    const text = e.currentTarget.value;
    e.currentTarget.value = "";
    if (keysOn && text) sendText(text);
  }
  function composed(e) {
    composing = false;
    e.currentTarget.value = "";
    if (keysOn && e.data) sendText(e.data);
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

    {#if setup}
      <div class="ewe-alert ewe-alert--accent" role="status">
        <Icon name="accessibility" />
        <div class="ewe-alert__body">
          <span class="ewe-alert__title">Allow it on {name} first</span>
          <span class="ewe-alert__desc">
            Android lets an app move a pointer and type only as an accessibility service. On the phone, open KDE
            Connect, tap <strong>Mouse receiver</strong> under “Some plugins need permissions”, and turn KDE Connect on
            in Accessibility. Until then the phone ignores what this page sends. The page notices by itself when you
            are done.
          </span>
          <span class="ewe-alert__desc setup__more">
            Is the switch greyed out? The app came from outside the Play Store: open its App info, then ⋮ and “Allow
            restricted settings”. Typing this way needs KDE Connect 1.36 or newer on the phone.
          </span>
        </div>
      </div>
    {/if}

    {#if canPoint || !on}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Pointer</h2></div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="pad"
          class:is-held={look.held}
          class:is-dragging={look.dragging}
          class:is-disabled={!padOn}
          role="application"
          aria-label="Touchpad for {name}"
          onpointerdown={down}
          onpointermove={(e) => pad.move(e.clientX, e.clientY)}
          onpointerup={() => pad.up()}
          onpointercancel={() => pad.cancel()}
          onlostpointercapture={() => pad.cancel()}
          onwheel={wheel}
          oncontextmenu={(e) => e.preventDefault()}
        >
          <Icon name="pointer" />
          <span class="pad__title">
            {#if look.dragging}
              {touch ? "Finger down. Drag to swipe" : "Button held. Drag"}
            {:else if look.held}
              Moving the pointer…
            {:else}
              Hold and drag to move the pointer
            {/if}
          </span>
          <span class="pad__hint">
            {#if touch}
              Click to tap · hold still, then drag, to swipe or long-press · scroll to turn the page · right-click to go back
            {:else}
              Click · right-click · hold still, then drag, to drag · scroll
            {/if}
          </span>
        </div>
        {#if touch}
          <div class="padkeys">
            <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!padOn} onclick={() => point("backclick")}>
              <Icon name="arrowLeft" /><span>Back</span>
            </button>
            <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!padOn} onclick={() => point("middleclick")}>
              <Icon name="home" /><span>Home</span>
            </button>
            <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!padOn} onclick={() => point("forwardclick")}>
              <Icon name="recents" /><span>Recents</span>
            </button>
          </div>
          <p class="note">The pointer shows on the phone while it moves, and hides again a few seconds later.</p>
        {/if}
      </section>
    {/if}

    {#if canType || !on}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Keyboard</h2></div>
        <div class="keys" class:is-typing={typing} class:is-disabled={!keysOn}>
          <Icon name="keyboard" />
          {#if typing}
            <span class="keys__echo">{echo || "Typing on the phone…"}</span>
          {:else}
            <span class="keys__hint">{keysOn ? "Click here, then type" : on ? `Waiting for ${name}` : "The phone isn’t in reach"}</span>
          {/if}
          <textarea
            class="keys__input"
            rows="1"
            aria-label="Type on {name}"
            disabled={!keysOn}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            onfocus={() => (typing = true)}
            onblur={() => (typing = false)}
            onkeydown={key}
            oninput={input}
            oncompositionstart={() => (composing = true)}
            oncompositionend={composed}
          ></textarea>
        </div>
        {#if untaken && keysOn}
          <p class="note note--warning" role="status">
            {name} didn’t take that. Tap a text field on the phone first: keys go to the field that is open there.
          </p>
        {:else}
          <p class="note">
            What you type goes to the field that is open on the phone, not into this window. Paste works too, and
            Ctrl+Tab moves to the phone’s next field.
          </p>
        {/if}
      </section>
    {/if}
  </div>
</div>
