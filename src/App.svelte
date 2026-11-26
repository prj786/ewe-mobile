<script>
  import { onMount, untrack } from "svelte";
  import * as api from "./lib/api.js";
  import {
    app, view, PAGES, boot, onEvent, resetDevice, loadDevice, loadContacts, loadApps, loadLauncherSetting, lookAgain, sendFiles
  } from "./lib/state.svelte.js";
  import { watchTheme } from "./lib/theme.js";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import Icon from "./lib/components/ui/Icon.svelte";
  import Empty from "./lib/components/ui/Empty.svelte";
  import Phone from "./lib/screens/Phone.svelte";
  import Messages from "./lib/screens/Messages.svelte";
  import Contacts from "./lib/screens/Contacts.svelte";
  import Notifications from "./lib/screens/Notifications.svelte";
  import Media from "./lib/screens/Media.svelte";
  import Files from "./lib/screens/Files.svelte";
  import Screen from "./lib/screens/Screen.svelte";
  import Apps from "./lib/screens/Apps.svelte";
  import Remote from "./lib/screens/Remote.svelte";
  import Devices from "./lib/screens/Devices.svelte";

  // The look, live (lib/theme.js): `ewe-theme show` at start, on focus, and
  // whenever the DE's user-theme.json changes — its whole content is the key.
  const themeKey = async () => JSON.stringify((await api.dePrefs()) ?? null);

  const d = $derived(view.device);
  // a page shows when the phone offers what it needs (what it offered last
  // time, while it is out of reach); every page shows with no phone at all
  const paired = $derived(app.devices.some((x) => x.paired));
  const pages = $derived(
    PAGES.filter((p) => {
      if (!paired) return p.id === "devices"; // nothing linked: the welcome is all there is
      if (!p.needs || !d) return true;
      const have = view.usable ? d.loaded : d.supported;
      return !have.length || p.needs.some((n) => have.includes(n));
    })
  );
  const page = $derived(PAGES.find((p) => p.id === app.route) ?? PAGES[0]);

  // the welcome is the Devices page; the first phone linked leads to Phone
  let hadPhone = null;
  $effect(() => {
    if (app.boot !== "ready" || !app.running) return;
    const has = paired;
    untrack(() => {
      if (!has && app.route !== "devices") app.route = "devices";
      else if (has && hadPhone === false) app.route = "phone";
      hadPhone = has;
    });
  });
  // the feature is one the phone has, turned off on this computer
  const turnedOff = $derived(
    !!d && view.usable && !!page.needs && !page.needs.some((n) => d.loaded.includes(n))
  );

  // The chosen phone's data follows it: read again when another phone is
  // chosen, when it comes back in reach, when a feature is turned on.
  // Strings, so a refreshed device list that says the same changes nothing.
  const deviceKey = $derived(d?.paired ? app.deviceId : "");
  const usable = $derived(view.usable);
  const featureKey = $derived(d ? d.loaded.join(",") : "");
  let lastDevice = "";
  $effect(() => {
    const id = deviceKey;
    const ok = usable;
    void featureKey;
    untrack(() => {
      if (id !== lastDevice) {
        lastDevice = id;
        resetDevice();
        if (id) {
          loadContacts(); // the address book is on disk: readable out of reach
          loadApps(); // … and so is the apps cache (C2)
        }
      }
      if (id && ok) loadDevice();
    });
  });

  function globalKey(e) {
    if (e.ctrlKey && !e.altKey && !e.shiftKey && /^[1-9]$/.test(e.key)) {
      const it = pages[Number(e.key) - 1];
      if (it) {
        e.preventDefault();
        app.route = it.id;
      }
    }
  }

  const ROUTES = PAGES.map((p) => p.id);
  const takePage = () =>
    api
      .takePage()
      .then((p) => {
        if (p && ROUTES.includes(p)) app.route = p;
      })
      .catch(() => {});

  // files dropped anywhere on the window go to the phone
  let dropping = $state(false);

  onMount(() => {
    const off = [];
    // #messages … open a page directly (the dev mock; inside Tauri, --page)
    const h = location.hash.replace(/^#/, "");
    if (ROUTES.includes(h)) app.route = h;

    off.push(watchTheme(themeKey, 4000));
    loadLauncherSetting();
    const tick = setInterval(() => (app.now = Date.now()), 30000);
    off.push(() => clearInterval(tick));

    (async () => {
      off.push(await api.listen("kdc", (e) => onEvent(e.payload)));
      off.push(await api.listen("page", takePage));
      off.push(
        await api.onFileDrop((e) => {
          if (e.type === "enter" || e.type === "over") dropping = view.usable && view.can("share");
          else if (e.type === "leave") dropping = false;
          else if (e.type === "drop") {
            const was = dropping;
            dropping = false;
            if (was) sendFiles(e.paths);
          }
        })
      );
      await boot();
      takePage();
    })();
    return () => off.forEach((u) => u && u());
  });

  $effect(() => {
    const r = app.route;
    if (ROUTES.includes(r) && location.hash !== "#" + r) history.replaceState(null, "", "#" + r);
  });
</script>

<svelte:window onkeydown={globalKey} />

<div class="ewe-appwin">
  <Sidebar {pages} />
  <main class="ewe-appwin__pane">
    {#if app.call}
      <div class="ewe-alert ewe-alert--banner ewe-alert--info" role="status">
        <Icon name="callIn" />
        <div class="ewe-alert__body">
          <span class="ewe-alert__title">
            {app.call.event === "ringing" ? "Incoming call" : "On a call"} ·
            {app.call.name || view.name(app.call.number) || "Unknown number"}
          </span>
        </div>
        <button
          type="button"
          class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm"
          title="Hide"
          aria-label="Hide"
          onclick={() => (app.call = null)}
        >
          <Icon name="x" />
        </button>
      </div>
    {/if}
    {#if app.boot === "ready" && d && d.paired && !d.reachable && app.route !== "devices"}
      <div class="ewe-alert ewe-alert--banner" role="status">
        <Icon name="wifiOff" />
        <div class="ewe-alert__body">
          <span class="ewe-alert__title">{d.name} isn’t in reach</span>
          <span class="ewe-alert__desc">Unlock it and check that it is on the same Wi-Fi as this computer.</span>
        </div>
        <button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" onclick={lookAgain}>Look again</button>
      </div>
    {/if}

    {#if app.boot === "starting"}
      <div class="center" role="status">
        <span class="ewe-spinner ewe-spinner--xl"></span>
        <span class="center__text">Connecting to the phone link…</span>
      </div>
    {:else if app.boot === "failed"}
      <div class="center">
        <Empty icon="alert" title="Couldn’t reach this computer’s session" desc={app.error || "The session bus didn’t answer."}>
          <button type="button" class="ewe-btn ewe-btn--secondary" onclick={boot}>Try again</button>
        </Empty>
      </div>
    {:else if !app.installed}
      <div class="center">
        <Empty
          icon="phone"
          title="KDE Connect isn’t installed"
          desc="ewe Mobile talks to the phone through it. Install kdeconnect with Komble, then open ewe Mobile again."
        />
      </div>
    {:else if !app.running}
      <div class="center">
        <Empty icon="phone" title="The phone link isn’t running" desc="It usually starts by itself when you sign in.">
          <button type="button" class="ewe-btn ewe-btn--primary" onclick={boot}>Start it</button>
        </Empty>
      </div>
    {:else if !paired || app.route === "devices"}
      <Devices welcome={!paired} />
    {:else if turnedOff}
      <div class="center">
        <Empty
          icon={page.icon}
          title="{page.label} is turned off for {d.name}"
          desc="Turn it back on under Devices, in what {d.name} may do."
        >
          <button type="button" class="ewe-btn ewe-btn--secondary" onclick={() => (app.route = "devices")}>
            Open Devices
          </button>
        </Empty>
      </div>
    {:else if app.route === "messages"}
      <Messages />
    {:else if app.route === "contacts"}
      <Contacts />
    {:else if app.route === "notifications"}
      <Notifications />
    {:else if app.route === "media"}
      <Media />
    {:else if app.route === "files"}
      <Files />
    {:else if app.route === "screen"}
      <Screen />
    {:else if app.route === "apps"}
      <Apps />
    {:else if app.route === "remote"}
      <Remote />
    {:else}
      <Phone />
    {/if}

    {#if dropping}
      <div class="dropveil" aria-hidden="true">
        <div class="ewe-drop is-dragover">
          <Icon name="upload" />
          <span class="ewe-drop__title">Drop to send to {d?.name ?? "the phone"}</span>
        </div>
      </div>
    {/if}
  </main>
</div>
<Toasts />
