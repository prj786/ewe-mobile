<script>
  /**
   * Contacts: the phone's address book, read-only. Pick someone to write to
   * them, have the phone dial them, or copy a number. The book is the copy
   * the phone last shared — it is still here while the phone is away.
   */
  import { tick } from "svelte";
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import Avatar from "../components/Avatar.svelte";
  import { app, view, syncContacts, startMessage, dial, copyText } from "../state.svelte.js";
  import { digits, kindWord } from "../format.js";

  let query = $state("");
  let selected = $state(""); // a contact uid
  let searchField = $state(null);

  const list = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return app.contacts;
    const qd = digits(q);
    return app.contacts.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.org.toLowerCase().includes(q) ||
        (qd.length >= 2 && c.phones.some((p) => digits(p.number).includes(qd)))
    );
  });

  // Under a letter each; names that start with a digit, a sign or an emoji
  // share "#", last. Gathered by letter, not by runs: the book is sorted by
  // code point, so "#" names sit at both ends of it and "I" can come twice.
  const sections = $derived.by(() => {
    const by = new Map();
    for (const c of list) {
      const ch = Array.from(c.name.trim())[0] || "#";
      const letter = /\p{L}/u.test(ch) ? (/[ა-ჿ]/.test(ch) ? ch : ch.toLocaleUpperCase()) : "#";
      if (!by.has(letter)) by.set(letter, []);
      by.get(letter).push(c);
    }
    const out = [...by].map(([letter, items]) => ({ letter, items }));
    const other = out.findIndex((x) => x.letter === "#");
    if (other >= 0) out.push(...out.splice(other, 1));
    return out;
  });

  const person = $derived(app.contacts.find((c) => c.uid === selected) ?? null);
  const canMessage = $derived(view.usable && view.can("sms"));
  const canDial = $derived(view.usable && view.can("share"));

  async function pageKey(e) {
    if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      // in one-pane mode the list has to be back before it can take the focus
      if (searchField && searchField.offsetParent === null) {
        selected = "";
        await tick();
      }
      searchField?.focus();
      searchField?.select();
    }
  }
</script>

<svelte:window onkeydown={pageKey} />

<div class="split" class:is-detail={!!person}>
  <aside class="split__list" aria-label="Contacts">
    <header class="listhead">
      <h1 class="listhead__title">Contacts</h1>
      <button
        type="button"
        class="ewe-iconbtn ewe-iconbtn--ghost"
        title="Ask the phone for its contacts again"
        aria-label="Ask the phone for its contacts again"
        disabled={!view.usable || !view.can("contacts")}
        onclick={syncContacts}
      >
        <Icon name="refresh" />
      </button>
    </header>
    <label class="ewe-input listhead__search">
      <Icon name="search" />
      <input bind:this={searchField} bind:value={query} type="text" placeholder="Search" aria-label="Search contacts" spellcheck="false" />
    </label>
    <div class="split__scroll">
      {#each sections as s (s.letter)}
        <div class="letter" aria-hidden="true">{s.letter}</div>
        {#each s.items as c (c.uid)}
          <button
            type="button"
            class="thread thread--one"
            class:is-selected={selected === c.uid}
            aria-current={selected === c.uid ? "true" : undefined}
            onclick={() => (selected = c.uid)}
          >
            <Avatar contact={c} size="sm" />
            <span class="thread__text">
              <span class="thread__name">{c.name}</span>
            </span>
          </button>
        {/each}
      {:else}
        {#if !app.contactsReady}
          <div class="listnote"><span class="ewe-spinner ewe-spinner--sm"></span> Reading contacts…</div>
        {:else if query.trim()}
          <div class="listnote">Nobody matches “{query.trim()}”.</div>
        {:else}
          <Empty
            compact
            icon="contacts"
            title="No contacts yet"
            desc="Allow KDE Connect to read contacts on the phone, then ask again."
          >
            <button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" disabled={!view.usable} onclick={syncContacts}>
              Ask the phone
            </button>
          </Empty>
        {/if}
      {/each}
    </div>
    {#if app.contacts.length}
      <div class="listfoot">{app.contacts.length} {app.contacts.length === 1 ? "contact" : "contacts"}</div>
    {/if}
  </aside>

  <section class="split__detail" aria-label="Contact">
    {#if person}
      <header class="convhead convhead--bare">
        <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost convhead__back" title="Back to contacts" aria-label="Back to contacts" onclick={() => (selected = "")}>
          <Icon name="arrowLeft" />
        </button>
      </header>
      <div class="scroll">
        <div class="person">
          <Avatar contact={person} size="2xl" />
          <h2 class="person__name">{person.name}</h2>
          {#if person.org}<div class="person__org">{person.org}</div>{/if}
          {#if person.phones.length}
            <div class="person__actions">
              <button type="button" class="ewe-btn ewe-btn--primary" disabled={!canMessage} onclick={() => startMessage([person.phones[0].number])}>
                <Icon name="messages" /> Message
              </button>
              <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!canDial} onclick={() => dial(person.phones[0].number)}>
                <Icon name="call" /> Call on the phone
              </button>
            </div>
          {/if}

          {#if person.phones.length}
            <section class="group person__group">
              <div class="ewe-section"><h3 class="ewe-section__title ewe-section__title--overline">Phone</h3></div>
              <div class="ewe-list">
                {#each person.phones as p (p.number)}
                  <div class="ewe-row">
                    <div class="ewe-row__text">
                      <div class="ewe-row__title person__number">{p.number}</div>
                      <div class="ewe-row__desc">{kindWord(p.kind)}</div>
                    </div>
                    <div class="ewe-row__trail">
                      <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm" title="Message" aria-label="Message {p.number}" disabled={!canMessage} onclick={() => startMessage([p.number])}><Icon name="messages" /></button>
                      <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm" title="Call on the phone" aria-label="Call {p.number} on the phone" disabled={!canDial} onclick={() => dial(p.number)}><Icon name="call" /></button>
                      <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm" title="Copy number" aria-label="Copy {p.number}" onclick={() => copyText(p.number, "Number copied")}><Icon name="copy" /></button>
                    </div>
                  </div>
                {/each}
              </div>
            </section>
          {/if}

          {#if person.emails.length}
            <section class="group person__group">
              <div class="ewe-section"><h3 class="ewe-section__title ewe-section__title--overline">Email</h3></div>
              <div class="ewe-list">
                {#each person.emails as e (e)}
                  <div class="ewe-row">
                    <div class="ewe-row__text"><div class="ewe-row__title">{e}</div></div>
                    <div class="ewe-row__trail">
                      <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm" title="Copy address" aria-label="Copy {e}" onclick={() => copyText(e, "Address copied")}><Icon name="copy" /></button>
                    </div>
                  </div>
                {/each}
              </div>
            </section>
          {/if}

          {#if person.note}
            <section class="group person__group">
              <div class="ewe-section"><h3 class="ewe-section__title ewe-section__title--overline">Note</h3></div>
              <p class="person__note">{person.note}</p>
            </section>
          {/if}
        </div>
      </div>
    {:else}
      <div class="center">
        <Empty icon="contacts" title="Choose a contact" desc="Write to them, have the phone dial them, or copy a number." />
      </div>
    {/if}
  </section>
</div>
