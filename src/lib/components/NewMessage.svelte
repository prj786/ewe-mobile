<script>
  /**
   * New message: who is it to? Type a name or a number; the phone's contacts
   * answer as you type, and a number nobody owns can be used as it is.
   * Choosing opens the thread with that person when there is one.
   */
  import { onMount } from "svelte";
  import Icon from "./ui/Icon.svelte";
  import Avatar from "./Avatar.svelte";
  import { app, startMessage } from "../state.svelte.js";
  import { digits, isNumber, kindWord } from "../format.js";

  let { close = () => {} } = $props();

  let query = $state("");
  let field = $state(null);
  let box = $state(null);
  let at = $state(0);
  let opener = null;

  // one row per number: someone with two phones is two choices
  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const qd = digits(q);
    const rows = [];
    for (const c of app.contacts) {
      const byName = !q || c.name.toLowerCase().includes(q);
      for (const p of c.phones) {
        if (byName || (qd.length >= 2 && digits(p.number).includes(qd))) rows.push({ contact: c, phone: p });
      }
      if (rows.length >= 40) break;
    }
    return rows.slice(0, 40);
  });
  // a typed number that is nobody's yet
  const typed = $derived(
    isNumber(query) && digits(query).length >= 3 && !matches.some((m) => digits(m.phone.number) === digits(query))
      ? query.trim()
      : ""
  );
  const count = $derived(matches.length + (typed ? 1 : 0));

  $effect(() => {
    void query;
    at = 0;
  });

  function choose(number) {
    close();
    startMessage([number]);
  }
  function pick(i) {
    if (typed && i === 0) return choose(typed);
    const m = matches[i - (typed ? 1 : 0)];
    if (m) choose(m.phone.number);
  }

  function onKey(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (count) at = (at + 1) % count;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (count) at = (at - 1 + count) % count;
    } else if (e.key === "Enter") {
      // Enter on a button presses that button; mid-composition it commits text
      if (e.isComposing || e.target instanceof HTMLButtonElement) return;
      e.preventDefault();
      pick(at);
    } else if (e.key === "Tab") {
      // modal: Tab stays between the field and the close button
      const f = [...box.querySelectorAll("input, button.ewe-iconbtn")];
      const i = f.indexOf(document.activeElement);
      e.preventDefault();
      f[(i + (e.shiftKey ? -1 : 1) + f.length) % f.length]?.focus();
    }
  }

  $effect(() => {
    void at;
    box?.querySelector(".is-active")?.scrollIntoView({ block: "nearest" });
  });

  onMount(() => {
    opener = document.activeElement;
    field?.focus();
    return () => {
      if (opener && document.contains(opener)) opener.focus();
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={close}></div>
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={box}
  class="ewe-dialog is-floating picker"
  role="dialog"
  aria-modal="true"
  aria-label="New message"
  tabindex="-1"
  data-state="open"
  onkeydown={onKey}
>
  <div class="ewe-dialog__head">
    <div class="ewe-dialog__titles">
      <h2 class="ewe-dialog__title">New message</h2>
    </div>
    <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm" title="Close" aria-label="Close" onclick={close}>
      <Icon name="x" />
    </button>
  </div>
  <label class="ewe-input ewe-input--lg">
    <Icon name="search" />
    <input
      bind:this={field}
      bind:value={query}
      type="text"
      placeholder="Name or number"
      aria-label="To"
      autocomplete="off"
      spellcheck="false"
      role="combobox"
      aria-expanded="true"
      aria-controls="picker-list"
      aria-activedescendant={count ? `picker-${at}` : undefined}
    />
  </label>
  <div class="picker__list" id="picker-list" role="listbox" aria-label="People">
    {#if typed}
      <button
        type="button"
        id="picker-0"
        role="option"
        aria-selected={at === 0}
        class="ewe-row ewe-row--interactive picker__row"
        class:is-active={at === 0}
        class:is-hover={at === 0}
        onclick={() => choose(typed)}
        onpointermove={() => (at = 0)}
      >
        <span class="ewe-row__lead"><Avatar name="" size="sm" /></span>
        <span class="ewe-row__text">
          <span class="ewe-row__title">{typed}</span>
          <span class="ewe-row__desc">Send to this number</span>
        </span>
      </button>
    {/if}
    {#each matches as m, i (m.contact.uid + m.phone.number)}
      {@const n = i + (typed ? 1 : 0)}
      <button
        type="button"
        id="picker-{n}"
        role="option"
        aria-selected={at === n}
        class="ewe-row ewe-row--interactive picker__row"
        class:is-active={at === n}
        class:is-hover={at === n}
        onclick={() => choose(m.phone.number)}
        onpointermove={() => (at = n)}
      >
        <span class="ewe-row__lead"><Avatar contact={m.contact} size="sm" /></span>
        <span class="ewe-row__text">
          <span class="ewe-row__title">{m.contact.name}</span>
          <span class="ewe-row__desc">{kindWord(m.phone.kind)} · {m.phone.number}</span>
        </span>
      </button>
    {/each}
    {#if !count}
      <div class="picker__none">
        {#if !app.contacts.length && !query.trim()}
          Type a number. The phone’s contacts show up here once it shares them.
        {:else}
          Nobody by that name. Type the full number to send to it.
        {/if}
      </div>
    {/if}
  </div>
</div>
