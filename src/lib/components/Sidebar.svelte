<script>
  // Side navigation (design/system/components/SideNav): the sheep mark and
  // the app name, the phone in use with its state written under its name,
  // the pages — Messages and Notifications with a count when something is
  // waiting — and the version. Up and Down move between pages; Ctrl+1 … 8
  // jump (App.svelte). Below 720px the rail is icons only and the labels
  // become tooltips.
  import { onMount } from "svelte";
  import sheep from "../../assets/ewe-mark.svg?raw";
  import Icon from "./ui/Icon.svelte";
  import { app, view } from "../state.svelte.js";
  import { deviceIcon } from "../format.js";

  let { pages = [] } = $props();

  let version = $state("");
  let narrow = $state(false);
  onMount(() => {
    import("@tauri-apps/api/app")
      .then((m) => m.getVersion())
      .then((v) => (version = v))
      .catch(() => {}); // the dev server outside Tauri
    const mq = window.matchMedia("(max-width: 720px)");
    const set = () => (narrow = mq.matches);
    set();
    mq.addEventListener("change", set);
    return () => mq.removeEventListener("change", set);
  });

  let navItems = $state([]);
  function navKey(e, i) {
    const d = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = (i + d + pages.length) % pages.length;
    navItems[n]?.focus();
    app.route = pages[n].id;
  }

  const d = $derived(view.device);
  const counts = $derived({ messages: view.unread, notifications: view.alerts });
  // the state in words; the dot only repeats it
  const state = $derived.by(() => {
    if (!d) return { word: "No phone linked", dot: "offline" };
    if (!d.reachable) return { word: "Not in reach", dot: "offline" };
    const b = d.battery;
    if (!b) return { word: "Connected", dot: "" };
    return { word: `Connected · ${b.charge}%${b.charging ? " charging" : ""}`, dot: "" };
  });
</script>

<nav class="ewe-sidenav" aria-label="ewe Mobile pages">
  <div class="ewe-sidenav__brand">
    <span class="ewe-sidenav__logo" aria-hidden="true">{@html sheep}</span>
    <span class="ewe-sidenav__name">ewe Mobile</span>
  </div>

  <button
    type="button"
    class="plate"
    class:is-selected={app.route === "devices"}
    title={narrow ? `${d ? d.name : "No phone linked"}${d ? ` · ${state.word}` : ""}` : "Devices"}
    onclick={() => (app.route = "devices")}
  >
    <span class="plate__glyph" aria-hidden="true">
      <Icon name={d ? deviceIcon(d.type) : "phone"} />
      <span class="ewe-avatar__status {state.dot ? `ewe-avatar__status--${state.dot}` : ''}"></span>
    </span>
    <span class="plate__text">
      <span class="plate__name">{d ? d.name : "Link a phone"}</span>
      <span class="plate__state">{state.word}</span>
    </span>
  </button>

  <div class="ewe-sidenav__group">
    {#each pages as it, i (it.id)}
      {@const n = counts[it.id] || 0}
      <button
        bind:this={navItems[i]}
        class="ewe-navitem"
        class:is-selected={app.route === it.id}
        aria-current={app.route === it.id ? "page" : undefined}
        data-route={it.id}
        title={narrow ? it.label : undefined}
        onclick={() => (app.route = it.id)}
        onkeydown={(e) => navKey(e, i)}
      >
        <Icon name={it.icon} />
        <span class="ewe-navitem__label">{it.label}</span>
        {#if n}
          <span
            class="ewe-badge ewe-badge--solid ewe-badge--accent nav-count"
            aria-label={it.id === "messages" ? `${n} unread` : `${n} waiting`}
          ><span class="ewe-badge__label">{n > 99 ? "99+" : n}</span></span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="ewe-sidenav__foot">
    {#if version}<div class="ewe-sidenav__version">ewe Mobile {version}</div>{/if}
  </div>
</nav>
