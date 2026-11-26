<script>
  /**
   * Screen: the phone's screen in a window on this computer (scrcpy over
   * adb). It says what this costs before it asks for anything — developer
   * options on the phone, a paired computer that can do anything on it, and
   * the banking apps that refuse while developer options is on — then walks
   * through the four steps once, and from then on is one button: Mirror.
   * The window is scrcpy's own (class ewe-mobile-screen, the phone's name
   * as its title); Hyprland floats it at the phone's shape.
   */
  import { onMount, untrack } from "svelte";
  import Icon from "../components/ui/Icon.svelte";
  import Toggle from "../components/ui/Toggle.svelte";
  import {
    app, view, loadAdb, pairAdb, connectAdb, offerTools, startScreen, stopWindow, loadWindows
  } from "../state.svelte.js";

  const d = $derived(view.device);
  const name = $derived(d?.name ?? "the phone");
  const adb = $derived(app.adb);
  const tools = $derived(adb?.tools ?? null);
  const haveTools = $derived(!!tools && tools.adb && tools.scrcpy);
  const pairingSvc = $derived((adb?.services ?? []).find((s) => s.kind === "pairing") ?? null);
  const connectSvc = $derived((adb?.services ?? []).find((s) => s.kind === "connect") ?? null);
  const seen = $derived(!!pairingSvc || !!connectSvc); // Wireless debugging is on and found
  const paired = $derived(!!adb?.paired);
  const connected = $derived(!!adb?.connected);
  const mirroring = $derived(view.mirroring);
  const ready = $derived(haveTools && connected);

  // which step is "now": the first one not done
  const step = $derived(!haveTools ? 1 : !seen && !connected ? 2 : !paired ? 3 : !connected ? 4 : 5);

  let code = $state("");
  let pairing = $state(false);
  let connecting = $state(false);
  let starting = $state(false);
  let looking = $state(false);

  // the mirror's options — honest ones, each a scrcpy flag
  const OPTS = "ewe-mobile.screen";
  let opts = $state({ screenOff: false, audio: true, stayAwake: false, quality: "balanced" });
  try {
    const saved = JSON.parse(localStorage.getItem(OPTS) || "null");
    if (saved && typeof saved === "object") opts = { ...opts, ...saved };
  } catch {
    /* defaults */
  }
  $effect(() => {
    const snapshot = JSON.stringify(opts);
    try {
      localStorage.setItem(OPTS, snapshot);
    } catch {
      /* a private window */
    }
  });

  async function look() {
    if (looking) return;
    looking = true;
    await Promise.all([loadAdb(), loadWindows()]);
    looking = false;
  }
  async function pair() {
    if (!pairingSvc || pairing || code.trim().length !== 6) return;
    pairing = true;
    const ok = await pairAdb(pairingSvc.host, pairingSvc.port, code.trim());
    pairing = false;
    if (ok) code = "";
  }
  async function connect() {
    if (!connectSvc || connecting) return;
    connecting = true;
    await connectAdb(connectSvc.host, connectSvc.port);
    connecting = false;
  }
  async function mirror() {
    if (starting) return;
    starting = true;
    await startScreen(opts);
    starting = false;
  }

  // The link is looked at while this page is open: every 3 s until it is
  // up, every 10 s after. Windows of ours every 2 s, so Stop and "closed"
  // are honest.
  onMount(() => {
    look();
    let t = null;
    const tick = () => {
      untrack(() => loadAdb());
      t = setTimeout(tick, untrack(() => (ready ? 10000 : 3000)));
    };
    t = setTimeout(tick, 3000);
    const w = setInterval(loadWindows, 2000);
    return () => {
      clearTimeout(t);
      clearInterval(w);
    };
  });
</script>

{#snippet stepRow(n, title, done, now, desc, trail)}
  <div class="ewe-row ewe-row--tall step" class:is-done={done} class:is-now={now}>
    <span class="ewe-row__lead step__num" aria-hidden="true">{#if done}<Icon name="check" />{:else}{n}{/if}</span>
    <div class="ewe-row__text">
      <div class="ewe-row__title">{title}</div>
      <div class="ewe-row__desc">{@render desc()}</div>
    </div>
    {#if trail}<div class="ewe-row__trail">{@render trail()}</div>{/if}
  </div>
{/snippet}

<div class="scroll">
  <div class="page">
    <header class="ewe-headerbar ewe-headerbar--page">
      <div class="ewe-headerbar__titles">
        <h1 class="ewe-headerbar__title">Screen</h1>
        <p class="ewe-headerbar__subtitle">{name}’s screen in a window on this computer.</p>
      </div>
      <div class="ewe-headerbar__actions">
        <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost" title="Look again" aria-label="Look again" disabled={looking} onclick={look}>
          <Icon name="refresh" />
        </button>
      </div>
    </header>

    {#if !d?.paired}
      <div class="ewe-empty">
        <span class="ewe-empty__icon"><Icon name="screenShare" /></span>
        <div class="ewe-empty__title">Link a phone first</div>
        <div class="ewe-empty__desc">The screen is mirrored from a phone that is paired under Devices.</div>
      </div>
    {:else}
      {#if ready}
        <section class="group">
          <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Mirror</h2></div>
          <div class="ewe-list">
            <div class="ewe-row ewe-row--tall">
              <span class="ewe-row__lead ewe-row__lead--tile"><Icon name="screenShare" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">{name}</div>
                <div class="ewe-row__desc">
                  {#if mirroring}
                    Mirroring in its own window. Your mouse and keyboard work in it; close it or stop it here.
                  {:else}
                    Opens in its own window, at the phone’s shape. The lag is the Wi-Fi’s.
                  {/if}
                </div>
              </div>
              <div class="ewe-row__trail">
                {#if mirroring}
                  <span class="ewe-badge ewe-badge--success"><span class="ewe-badge__label">Mirroring</span></span>
                  <button type="button" class="ewe-btn ewe-btn--secondary" onclick={() => stopWindow("ewe-mobile-screen")}>
                    <Icon name="stop" /> Stop
                  </button>
                {:else}
                  <button type="button" class="ewe-btn ewe-btn--primary" disabled={starting} onclick={mirror}>
                    {#if starting}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--on-accent"></span>{/if}
                    Mirror screen
                  </button>
                {/if}
              </div>
            </div>
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="moon" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Phone screen off while mirroring</div>
                <div class="ewe-row__desc">The phone goes dark and saves its battery; the window stays live.</div>
              </div>
              <div class="ewe-row__trail"><Toggle on={opts.screenOff} label="Phone screen off while mirroring" toggled={(v) => (opts.screenOff = v)} /></div>
            </div>
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="volume" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Sound on this computer</div>
                <div class="ewe-row__desc">What the phone plays comes out here, not on the phone.</div>
              </div>
              <div class="ewe-row__trail"><Toggle on={opts.audio} label="Sound on this computer" toggled={(v) => (opts.audio = v)} /></div>
            </div>
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="power" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Stay awake</div>
                <div class="ewe-row__desc">The phone doesn’t lock while the window is open (when it is plugged in).</div>
              </div>
              <div class="ewe-row__trail"><Toggle on={opts.stayAwake} label="Stay awake" toggled={(v) => (opts.stayAwake = v)} /></div>
            </div>
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="gauge" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Quality</div>
                <div class="ewe-row__desc">Lower is smoother on a slow Wi-Fi; higher is sharper.</div>
              </div>
              <div class="ewe-row__trail">
                <div class="ewe-seg ewe-seg--sm" role="radiogroup" aria-label="Quality">
                  {#each [["low", "Low"], ["balanced", "Balanced"], ["high", "High"]] as [id, label] (id)}
                    <button type="button" role="radio" class="ewe-seg__item" class:is-selected={opts.quality === id} aria-checked={opts.quality === id} onclick={() => (opts.quality = id)}>{label}</button>
                  {/each}
                </div>
              </div>
            </div>
          </div>
          <p class="note">Apps that mark their screens secure (banking, video with DRM) come out black — that is Android’s doing.</p>
        </section>
      {:else}
        <div class="ewe-alert ewe-alert--info" role="note">
          <Icon name="shieldAlert" />
          <div class="ewe-alert__body">
            <span class="ewe-alert__title">What this takes, honestly</span>
            <span class="ewe-alert__desc">
              <ul class="costs">
                <li><strong>Developer options</strong> and <strong>Wireless debugging</strong> on the phone.</li>
                <li>A computer paired this way <strong>can do anything</strong> on the phone that you can. Pair only your own.</li>
                <li>Some banking apps refuse to run while Developer options is on — TBC and Liberty do, Bank of Georgia doesn’t. Switch Developer options off when you need them.</li>
              </ul>
            </span>
          </div>
        </div>
      {/if}

      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">{ready ? "The link" : "Set up once"}</h2></div>
        <div class="ewe-list">
          {#snippet toolsDesc()}
            {#if haveTools}adb and scrcpy are installed.{:else if tools}{tools.adb ? "scrcpy" : tools.scrcpy ? "android-tools" : "android-tools and scrcpy"} missing. Both come from Komble; nothing else is needed.{:else}Looking…{/if}
          {/snippet}
          {#snippet toolsTrail()}
            {#if tools && !haveTools}<button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" onclick={offerTools}>Install with Komble</button>{/if}
          {/snippet}
          {@render stepRow(1, "The tools on this computer", haveTools, step === 1, toolsDesc, toolsTrail)}

          {#snippet wdDesc()}
            {#if connected}On, and this computer is connected.{:else if seen}On — {name} is on this Wi-Fi.{:else}On the phone: Settings → System → Developer options → <strong>Wireless debugging</strong>, on the same Wi-Fi as this computer. (Developer options appears after tapping the build number seven times under About phone.){/if}
          {/snippet}
          {@render stepRow(2, "Wireless debugging on the phone", seen || connected, step === 2, wdDesc, null)}

          {#snippet pairDesc()}
            {#if paired}Paired. The phone remembers this computer until you forget it there.{:else if pairingSvc}
              The phone is showing a six-digit code. Type it here — nothing else is asked.
            {:else}
              Under Wireless debugging, tap <strong>Pair device with pairing code</strong>. The code it shows is typed here.
              {#if step === 3}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--neutral"></span>{/if}
            {/if}
          {/snippet}
          {#snippet pairTrail()}
            {#if !paired && pairingSvc}
              <form class="codefield" onsubmit={(e) => { e.preventDefault(); pair(); }}>
                <label class="ewe-input">
                  <Icon name="key" />
                  <input type="text" inputmode="numeric" pattern="[0-9]*" maxlength="6" placeholder="000000" aria-label="Pairing code" bind:value={code} autocomplete="one-time-code" />
                </label>
                <button type="submit" class="ewe-btn ewe-btn--primary ewe-btn--sm" disabled={pairing || !/^\d{6}$/.test(code.trim())}>
                  {#if pairing}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--on-accent"></span>{/if}
                  Pair
                </button>
              </form>
            {/if}
          {/snippet}
          {@render stepRow(3, "Pair this computer", paired, step === 3, pairDesc, pairTrail)}

          {#snippet connDesc()}
            {#if connected}Connected{adb?.model ? ` · ${adb.model}` : ""}. It reconnects by itself whenever the phone is on this Wi-Fi with Wireless debugging on.{:else if connectSvc}{name} is in reach; connecting usually happens by itself.{:else}Not connected. Switch Wireless debugging on, on this Wi-Fi.{/if}
          {/snippet}
          {#snippet connTrail()}
            {#if !connected && connectSvc && paired}
              <button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" disabled={connecting} onclick={connect}>
                {#if connecting}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--neutral"></span>{/if}
                Connect
              </button>
            {/if}
          {/snippet}
          {@render stepRow(4, "Connected", connected, step === 4, connDesc, connTrail)}
        </div>
        {#if adb?.error}
          <div class="ewe-alert ewe-alert--warning" role="status">
            <Icon name="warning" />
            <div class="ewe-alert__body"><span class="ewe-alert__title">adb didn’t answer</span><span class="ewe-alert__desc">{adb.error}</span></div>
          </div>
        {/if}
        <p class="note">Pairing keys stay with adb (<code>~/.android</code>), never in ewe’s settings. ewe Mobile only asks for the six digits.</p>
      </section>
    {/if}
  </div>
</div>
