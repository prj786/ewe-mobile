<script>
  /**
   * Messages: the phone's SMS and MMS. Conversations on the left, the open
   * one on the right — on a narrow tile, one at a time. Names and faces come
   * from the phone's contacts; a message you send shows at once as
   * "Sending…" and settles when the phone reports it sent. The phone does
   * the sending: this is its SMS, at a keyboard.
   */
  import { tick } from "svelte";
  import * as api from "../api.js";
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import Avatar from "../components/Avatar.svelte";
  import Composer from "../components/Composer.svelte";
  import NewMessage from "../components/NewMessage.svelte";
  import {
    app, view, openThread, closeThread, loadEarlier, sendMessage, dial, copyText, openAttachment,
    dropPending, retryPending
  } from "../state.svelte.js";
  import { fail } from "../toast.svelte.js";
  import { when, dayLabel, dayKey, clock, linkify, snippet, isNumber, kindWord, numberKey } from "../format.js";

  let query = $state("");
  let picking = $state(false);
  let searchField = $state(null);

  const threads = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return app.threads;
    return app.threads.filter(
      (t) =>
        view.title(t.addresses).toLowerCase().includes(q) ||
        t.addresses.some((a) => a.toLowerCase().includes(q)) ||
        String(t.body || "").toLowerCase().includes(q)
    );
  });

  const open = $derived(app.thread != null || !!app.compose);
  const addresses = $derived(app.compose ? app.compose.addresses : view.addresses(app.thread));
  const group = $derived(addresses.length > 1);
  const contact = $derived(!group && addresses[0] ? view.contact(addresses[0]) : null);
  const msgs = $derived(app.thread != null ? (app.messages[app.thread] ?? []) : []);
  const pending = $derived(view.pendingFor(app.thread, app.compose));
  const loading = $derived(app.thread != null && !!app.loading[app.thread]);

  // under the name: the number as the phone has it, and what kind it is
  const subtitle = $derived.by(() => {
    if (group) return `${addresses.length} people`;
    const a = addresses[0] ?? "";
    if (!contact) return isNumber(a) ? "Not in your contacts" : "";
    const p = contact.phones.find((x) => numberKey(x.number) === numberKey(a));
    return p ? `${kindWord(p.kind)} · ${p.number}` : a;
  });

  // the thread as days, oldest first
  const days = $derived.by(() => {
    const out = [];
    let day = null;
    for (const m of msgs) {
      const k = dayKey(m.date);
      if (!day || day.key !== k) {
        day = { key: k, date: m.date, items: [] };
        out.push(day);
      }
      day.items.push(m);
    }
    return out;
  });

  const mine = (m) => m.type !== 1;
  // a run of messages from one side within five minutes reads as one turn
  const sameTurn = (a, b) => !!a && !!b && mine(a) === mine(b) && Math.abs(b.date - a.date) < 300000 &&
    (mine(a) || a.addresses[0] === b.addresses[0]);
  const stateWord = (m) => (m.type === 5 ? "Not sent" : m.type === 4 || m.type === 6 ? "Sending…" : "");

  // ── keeping the place while messages arrive ───────────────────────────────
  let scroller = $state(null);
  let atEnd = true;
  let lastThread = null;
  let lastFirst = null;
  let lastEl = null;
  let lastPending = 0;

  function onScroll() {
    if (!scroller) return;
    atEnd = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  }

  $effect.pre(() => {
    // what moves the list: the thread, its messages, the ones still sending
    const thread = app.thread;
    const first = msgs[0] ? `${msgs[0].uid}:${msgs[0].date}` : null;
    void msgs.length;
    void pending.length;
    void app.compose;
    const el = scroller;
    // a list that was just built (the pane was closed) is a new conversation
    // to the eye, whatever was open before; so is one you just wrote into
    const switched = thread !== lastThread || el !== lastEl;
    const wrote = pending.length > lastPending;
    const prepended = !switched && first !== lastFirst && lastFirst !== null;
    lastPending = pending.length;
    if (!el) return; // not on screen yet: decide when it is
    lastThread = thread;
    lastFirst = first;
    lastEl = el;
    const fromEnd = el.scrollHeight - el.scrollTop;
    const stick = atEnd || switched || wrote;
    tick().then(() => {
      if (stick) {
        el.scrollTop = el.scrollHeight;
        atEnd = true;
      } else if (prepended) {
        // earlier messages came in above: stay on the one being read
        el.scrollTop = el.scrollHeight - fromEnd;
      }
    });
  });

  // a picture takes its height when it has loaded: stay at the end if we were
  function settled() {
    if (atEnd && scroller) scroller.scrollTop = scroller.scrollHeight;
  }

  function pageKey(e) {
    if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      if (view.usable) picking = true;
    } else if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      findInList();
    }
  }
  // in one-pane mode the search field is behind the open conversation: the
  // list has to be back on screen before it can take the focus
  async function findInList() {
    if (searchField && searchField.offsetParent === null) {
      closeThread();
      await tick();
    }
    searchField?.focus();
    searchField?.select();
  }

  function follow(e, url) {
    e.preventDefault();
    api.openLink(url).catch((err) => fail(err, "Couldn’t open that link."));
  }
</script>

<svelte:window onkeydown={pageKey} />

<div class="split" class:is-detail={open}>
  <aside class="split__list" aria-label="Conversations">
    <header class="listhead">
      <h1 class="listhead__title">Messages</h1>
      <button
        type="button"
        class="ewe-iconbtn ewe-iconbtn--ghost"
        title="New message (Ctrl+N)"
        aria-label="New message"
        disabled={!view.usable}
        onclick={() => (picking = true)}
      >
        <Icon name="messagePlus" />
      </button>
    </header>
    <label class="ewe-input listhead__search">
      <Icon name="search" />
      <input bind:this={searchField} bind:value={query} type="text" placeholder="Search" aria-label="Search conversations" spellcheck="false" />
    </label>
    <div class="split__scroll">
      {#each threads as t (t.thread)}
        {@const unread = t.type === 1 && !t.read}
        <button
          type="button"
          class="thread"
          class:is-selected={app.thread === t.thread}
          aria-current={app.thread === t.thread ? "true" : undefined}
          onclick={() => openThread(t.thread)}
        >
          <Avatar address={t.addresses[0]} group={t.addresses.length > 1} />
          <span class="thread__text">
            <span class="thread__top">
              <span class="thread__name" class:is-strong={unread}>{view.title(t.addresses)}</span>
              <span class="thread__time">{when(t.date, app.now)}</span>
            </span>
            <span class="thread__bottom">
              <span class="thread__snippet" class:is-strong={unread}>{mine(t) ? "You: " : ""}{snippet(t) || "…"}</span>
              {#if unread}
                <span class="ewe-badge ewe-badge--dot ewe-badge--solid ewe-badge--accent" role="img" aria-label="Unread"></span>
              {/if}
            </span>
          </span>
        </button>
      {:else}
        {#if !app.threadsReady && view.usable}
          <div class="listnote"><span class="ewe-spinner ewe-spinner--sm"></span> Reading messages…</div>
        {:else if query.trim()}
          <div class="listnote">No conversation matches “{query.trim()}”.</div>
        {:else if !view.usable}
          <Empty compact icon="messages" title="Nothing read yet" desc="Messages are read from the phone when it is in reach." />
        {:else}
          <Empty
            compact
            icon="messages"
            title="No messages yet"
            desc="Messages on the phone show up here. If this stays empty, allow KDE Connect to read SMS on the phone."
          />
        {/if}
      {/each}
    </div>
  </aside>

  <section class="split__detail" aria-label="Conversation">
    {#if open}
      <header class="convhead">
        <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost convhead__back" title="Back to conversations" aria-label="Back to conversations" onclick={closeThread}>
          <Icon name="arrowLeft" />
        </button>
        <Avatar address={addresses[0]} {group} />
        <div class="convhead__text">
          <div class="convhead__name">{view.title(addresses)}</div>
          {#if subtitle}<div class="convhead__sub">{subtitle}</div>{/if}
        </div>
        {#if !group && addresses[0] && isNumber(addresses[0])}
          {#if view.can("share")}
            <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost" title="Call on the phone" aria-label="Call on the phone" disabled={!view.usable} onclick={() => dial(addresses[0])}>
              <Icon name="call" />
            </button>
          {/if}
          <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost" title="Copy number" aria-label="Copy number" onclick={() => copyText(addresses[0], "Number copied")}>
            <Icon name="copy" />
          </button>
        {/if}
      </header>

      <div class="conv" bind:this={scroller} onscroll={onScroll}>
        {#if app.thread != null}
          {#if loading}
            <div class="conv__note"><span class="ewe-spinner ewe-spinner--sm"></span> Getting messages from the phone…</div>
          {:else if view.hasEarlier(app.thread)}
            <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm conv__more" disabled={!view.usable} onclick={() => loadEarlier(app.thread)}>
              Show earlier messages
            </button>
          {/if}
        {:else if !pending.length}
          <div class="conv__note">This is the start of your conversation with {view.title(addresses)}.</div>
        {/if}

        {#each days as day (day.key)}
          <div class="conv__day" role="separator"><span>{dayLabel(day.date, app.now)}</span></div>
          {#each day.items as m, i (`${m.uid}:${m.date}`)}
            {@const out = mine(m)}
            {@const first = !sameTurn(day.items[i - 1], m)}
            {@const last = !sameTurn(m, day.items[i + 1])}
            {@const word = stateWord(m)}
            {#if m.body || m.attachments.length}
              <div class="msg" class:msg--out={out} class:msg--first={first} class:msg--last={last}>
                {#if group && !out && first}<span class="msg__who">{view.name(m.addresses[0])}</span>{/if}
                <div class="bubble" class:bubble--failed={m.type === 5}>
                  {#each m.attachments as a, n (n)}
                    {#if a.thumb}
                      <button type="button" class="bubble__media" title="Open" aria-label="Open the attachment" disabled={!view.usable} onclick={() => openAttachment(a)}>
                        <img src={a.thumb} alt="" draggable="false" onload={settled} />
                      </button>
                    {:else}
                      <button type="button" class="bubble__file" disabled={!view.usable} onclick={() => openAttachment(a)}>
                        <Icon name="paperclip" /> Attachment
                      </button>
                    {/if}
                  {/each}
                  {#if m.body}
                    <span class="bubble__text">{#each linkify(m.body) as p}{#if p.url}<a class="bubble__link" href={p.url} onclick={(e) => follow(e, p.url)}>{p.text}</a>{:else}{p.text}{/if}{/each}</span>
                  {/if}
                </div>
                {#if last || word}
                  <span class="msg__meta" class:is-danger={m.type === 5}>{clock(m.date)}{word ? ` · ${word}` : ""}</span>
                {/if}
              </div>
            {/if}
          {/each}
        {/each}

        {#each pending as p (p.id)}
          <div class="msg msg--out msg--first msg--last">
            <div class="bubble bubble--pending" class:bubble--failed={p.state === "failed"}>
              <span class="bubble__text">{p.body}</span>
            </div>
            <span class="msg__meta" class:is-danger={p.state === "failed"}>
              {#if p.state === "sending"}
                Sending…
              {:else if p.state === "unconfirmed"}
                Handed to the phone · not confirmed yet
                <button type="button" class="msg__act" onclick={() => dropPending(p.id)}>Hide</button>
              {:else}
                Not sent{p.error ? `: ${p.error}` : ""}
                <button type="button" class="msg__act" onclick={() => retryPending(p.id)}>Try again</button>
                <button type="button" class="msg__act" onclick={() => dropPending(p.id)}>Discard</button>
              {/if}
            </span>
          </div>
        {/each}
      </div>

      {#key app.thread != null ? `t${app.thread}` : `n${addresses.join(",")}`}
        <Composer
          send={sendMessage}
          disabled={!view.usable}
          autofocus
          placeholder={view.usable ? `Message ${view.title(addresses)}` : "The phone isn’t in reach"}
        />
      {/key}
    {:else}
      <div class="center">
        <Empty icon="messages" title="Choose a conversation" desc="Or start a new one. Your phone sends it as a text message.">
          <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!view.usable} onclick={() => (picking = true)}>
            <Icon name="messagePlus" /> New message
          </button>
        </Empty>
      </div>
    {/if}
  </section>
</div>

{#if picking}
  <NewMessage close={() => (picking = false)} />
{/if}
