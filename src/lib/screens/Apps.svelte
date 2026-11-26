<script>
  /**
   * Apps: the phone's apps on this computer. The list comes from ewe Mobile
   * for Android (C1), is kept while the phone is away (C2), and each app
   * opens with one click — in its own window here when the debugging link
   * is up, on the phone otherwise; the ⋯ menu offers both. A phone running
   * the stock KDE Connect app sends no list; with adb connected, scrcpy's
   * list stands in (names only, so letter tiles).
   */
  import { onMount } from "svelte";
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import Menu from "../components/ui/Menu.svelte";
  import Toggle from "../components/ui/Toggle.svelte";
  import {
    app, view, loadApps, refreshApps, loadScrcpyApps, openApp, loadAdb, loadWindows, setLauncherApps
  } from "../state.svelte.js";
  import { filterApps, letterOf, readWhen } from "../apps.js";

  const d = $derived(view.device);
  const name = $derived(d?.name ?? "the phone");
  const on = $derived(view.usable);
  const adbReady = $derived(view.adbReady);
  const list = $derived(app.apps.list);
  let query = $state("");
  let hideSys = $state(false);
  const shown = $derived(filterApps(list, query).filter((a) => !hideSys || !a.sys));
  const sysCount = $derived(list.filter((a) => a.sys).length);
  const openHere = $derived(new Set(app.windows.map((w) => w.package).filter(Boolean)));
  let busy = $state(""); // the package being opened

  async function open(a, where = "auto") {
    if (busy) return;
    busy = a.package;
    await openApp(a, where);
    busy = "";
  }
  const items = (a) => [
    { label: adbReady ? "Open here" : "Open here (needs the debugging link)", icon: "appWindow", run: () => open(a, "here"), disabled: !adbReady },
    { label: `Open on ${name}`, icon: "phone", run: () => open(a, "phone"), disabled: !on || app.apps.fallback }
  ];

  let refreshing = $state(false);
  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    if (app.apps.fallback) await loadScrcpyApps(true);
    else if (on && view.eweOnPhone) await refreshApps();
    await loadApps();
    refreshing = false;
  }

  onMount(() => {
    loadAdb();
    loadWindows();
    const w = setInterval(loadWindows, 3000);
    return () => clearInterval(w);
  });
</script>

<div class="scroll">
  <div class="page page--wide">
    <header class="ewe-headerbar ewe-headerbar--page">
      <div class="ewe-headerbar__titles">
        <h1 class="ewe-headerbar__title">Apps</h1>
        <p class="ewe-headerbar__subtitle">
          {#if adbReady}Open {name}’s apps in their own windows here, or on the phone.{:else}Open {name}’s apps on the phone from here — or here, once the Screen page’s link is set up.{/if}
        </p>
      </div>
      <div class="ewe-headerbar__actions">
        <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost" title="Read the list again" aria-label="Read the list again" disabled={refreshing || (!on && !app.apps.fallback)} onclick={refresh}>
          {#if refreshing}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--neutral"></span>{:else}<Icon name="refresh" />{/if}
        </button>
      </div>
    </header>

    {#if !d?.paired}
      <Empty icon="grid" title="Link a phone first" desc="Its apps show here once it is paired under Devices." />
    {:else if !app.apps.ready}
      <div class="listnote"><span class="ewe-spinner ewe-spinner--sm"></span> {app.apps.fallback || app.apps.source === "scrcpy" ? "Asking scrcpy for the apps — about twelve seconds…" : "Reading the apps…"}</div>
    {:else if app.apps.error && !list.length}
      <Empty icon="alert" title="Couldn’t read the apps" desc={app.apps.error}>
        <button type="button" class="ewe-btn ewe-btn--secondary" onclick={() => loadApps()}>Try again</button>
      </Empty>
    {:else if !list.length && !on}
      <Empty icon="grid" title="Nothing read yet" desc="The phone’s apps arrive when it is in reach; after that they stay here while it is away." />
    {:else if !list.length}
      <Empty icon="phone" title="Install ewe Mobile on your phone" desc="{name} runs the stock KDE Connect app, which sends no list of its apps. ewe Mobile for Android does — with icons, and it opens them on the phone from here.">
        {#if adbReady}
          <button type="button" class="ewe-btn ewe-btn--secondary" onclick={() => loadScrcpyApps(false)}>List the apps with scrcpy instead</button>
        {:else}
          <button type="button" class="ewe-btn ewe-btn--secondary" onclick={() => (app.route = "screen")}>Set up the debugging link</button>
        {/if}
      </Empty>
    {:else}
      <div class="appsbar">
        <label class="ewe-input appsbar__search">
          <Icon name="search" />
          <input type="search" placeholder="Search {list.length} apps" aria-label="Search apps" bind:value={query} spellcheck="false" />
        </label>
        {#if sysCount}
          <label class="appsbar__count"><input type="checkbox" bind:checked={hideSys} /> Hide {sysCount} system apps</label>
        {/if}
        <span class="appsbar__count">
          {#if app.apps.fallback}From scrcpy{:else if app.apps.source === "cache"}Kept from last time{:else}From {name}{/if}{app.apps.updated ? ` · ${readWhen(app.apps.updated, app.now)}` : ""}
        </span>
      </div>

      {#if shown.length}
        <div class="appgrid" role="list">
          {#each shown as a (a.package)}
            <div class="appcell" class:is-sys={a.sys} role="listitem">
              <button
                type="button"
                class="appcell__btn"
                title={adbReady ? `Open ${a.name} here` : `Open ${a.name} on ${name}`}
                disabled={busy === a.package || (!adbReady && (!on || app.apps.fallback))}
                onclick={() => open(a)}
              >
                {#if a.icon}
                  <img class="appcell__img" src={a.icon} alt="" draggable="false" />
                {:else}
                  <span class="appcell__letter" aria-hidden="true">{letterOf(a.name)}</span>
                {/if}
                <span class="appcell__name">{a.name}</span>
              </button>
              {#if openHere.has(a.package)}<span class="appcell__here" title="Open here"></span>{/if}
              <span class="appcell__more"><Menu label="More for {a.name}" items={items(a)} /></span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="ewe-list"><Empty compact icon="search" title="No app matches" desc="Try another word, or the package name." /></div>
      {/if}

      <section class="group">
        <div class="ewe-list">
          {#if !adbReady}
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="appWindow" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Apps open on the phone for now</div>
                <div class="ewe-row__desc">With the debugging link set up, each app opens in its own window on this computer.</div>
              </div>
              <div class="ewe-row__trail"><button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" onclick={() => (app.route = "screen")}>Set it up</button></div>
            </div>
          {/if}
          {#if app.windows.some((w) => w.package)}
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="appWindow" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Open here now</div>
                <div class="ewe-row__desc">{app.windows.filter((w) => w.package).map((w) => w.title).join(", ")}</div>
              </div>
            </div>
          {/if}
          {#if !app.apps.fallback}
            <div class="ewe-row">
              <span class="ewe-row__lead"><Icon name="rocket" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">Show the phone’s apps in the launcher</div>
                <div class="ewe-row__desc">Each one as “WhatsApp · On {name}” in Super+D, and in the dock when it is open here.</div>
              </div>
              <div class="ewe-row__trail"><Toggle on={app.launcherApps} label="Show the phone’s apps in the launcher" toggled={setLauncherApps} /></div>
            </div>
          {/if}
        </div>
      </section>
    {/if}
  </div>
</div>
