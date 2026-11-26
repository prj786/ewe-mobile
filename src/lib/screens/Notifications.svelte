<script>
  /**
   * Notifications: what the phone is showing right now, by app. Dismiss one
   * here and it leaves the phone too; answer the ones that take a reply
   * (a chat message) without picking the phone up. The quiet ones Android
   * marks silent wait behind a switch at the bottom.
   */
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import { app, view, dismissNotification, dismissAll, replyNotification } from "../state.svelte.js";

  let replying = $state(""); // the notification being answered
  let reply = $state("");
  let sending = $state(false);
  let showSilent = $state(false);

  const loud = $derived(app.notifications.filter((n) => !n.silent));
  const silent = $derived(app.notifications.filter((n) => n.silent));

  // the app's own launcher icon (C2, by the package in the notification),
  // else the picture the phone sent with it — never a letter
  const iconFor = (n) => view.appIcon(n.package) || n.icon || "";
  const byApp = (list) => {
    const groups = [];
    for (const n of list) {
      const name = n.app || "Phone";
      let g = groups.find((x) => x.app === name);
      if (!g) {
        g = { app: name, icon: iconFor(n), items: [] };
        groups.push(g);
      }
      if (!g.icon) g.icon = iconFor(n);
      g.items.push(n);
    }
    return groups;
  };
  const groups = $derived(byApp(loud));
  const quiet = $derived(byApp(silent));
  const clearable = $derived(loud.filter((n) => n.dismissable));

  function startReply(n) {
    replying = replying === n.id ? "" : n.id;
    reply = "";
  }
  async function sendReply(n) {
    if (!reply.trim() || sending) return;
    sending = true;
    const ok = await replyNotification(n, reply.trim());
    sending = false;
    if (ok) {
      replying = "";
      reply = "";
    }
  }
  const focusOnMount = (node) => {
    node.focus();
  };
</script>

{#snippet rows(list)}
  {#each list as g (g.app)}
    <section class="group">
      <div class="ewe-section">
        <h2 class="ewe-section__title ewe-section__title--overline apphead">
          {#if g.icon}<img class="apphead__icon" src={g.icon} alt="" />{:else}<span class="apphead__icon apphead__icon--none"><Icon name="bell" /></span>{/if}
          {g.app}
        </h2>
      </div>
      <div class="ewe-list">
        {#each g.items as n (n.id)}
          <div class="note-row">
            <div class="ewe-row ewe-row--tall">
              <div class="ewe-row__text">
                {#if n.title}<div class="ewe-row__title">{n.title}</div>{/if}
                {#if n.text}<div class="note-row__text">{n.text}</div>{/if}
                {#if !n.title && !n.text}<div class="note-row__text">{n.ticker || "No details"}</div>{/if}
              </div>
              <div class="ewe-row__trail">
                {#if n.replyId}
                  <button
                    type="button"
                    class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm"
                    class:is-selected={replying === n.id}
                    title="Reply"
                    aria-label="Reply to {n.title || n.app}"
                    aria-expanded={replying === n.id}
                    disabled={!view.usable}
                    onclick={() => startReply(n)}
                  >
                    <Icon name="reply" />
                  </button>
                {/if}
                {#if n.dismissable}
                  <button
                    type="button"
                    class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--sm"
                    title="Dismiss on the phone"
                    aria-label="Dismiss {n.title || n.app}"
                    disabled={!view.usable}
                    onclick={() => dismissNotification(n)}
                  >
                    <Icon name="x" />
                  </button>
                {/if}
              </div>
            </div>
            {#if replying === n.id}
              <form
                class="note-row__reply"
                onsubmit={(e) => {
                  e.preventDefault();
                  sendReply(n);
                }}
              >
                <label class="ewe-input note-row__field">
                  <input
                    use:focusOnMount
                    bind:value={reply}
                    type="text"
                    placeholder="Reply to {n.title || n.app}"
                    aria-label="Reply"
                    onkeydown={(e) => e.key === "Escape" && (replying = "")}
                  />
                </label>
                <button type="submit" class="ewe-btn ewe-btn--primary" disabled={!reply.trim() || sending}>Send</button>
              </form>
            {/if}
          </div>
        {/each}
      </div>
    </section>
  {/each}
{/snippet}

<div class="scroll">
  <div class="page">
    <header class="ewe-headerbar ewe-headerbar--page">
      <div class="ewe-headerbar__titles">
        <h1 class="ewe-headerbar__title">Notifications</h1>
        <p class="ewe-headerbar__subtitle">What {view.device?.name ?? "the phone"} is showing right now.</p>
      </div>
      {#if clearable.length > 1}
        <div class="ewe-headerbar__actions">
          <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!view.usable} onclick={() => dismissAll(clearable)}>
            Dismiss {clearable.length} on the phone
          </button>
        </div>
      {/if}
    </header>

    {#if !app.notificationsReady && view.usable}
      <div class="listnote"><span class="ewe-spinner ewe-spinner--sm"></span> Reading notifications…</div>
    {:else if !view.can("notifications") && view.usable}
      <Empty icon="bell" title="Notifications are turned off" desc="Turn them on under Devices, in what {view.device?.name ?? 'the phone'} may do." />
    {:else if !loud.length && !view.usable}
      <Empty icon="bell" title="Nothing read yet" desc="Notifications are read from the phone when it is in reach." />
    {:else if !loud.length}
      <Empty icon="bell" title="Nothing waiting" desc="New notifications from the phone show up here as they arrive." />
    {:else}
      {@render rows(groups)}
    {/if}

    {#if silent.length}
      <section class="group">
        <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm quiet-toggle" aria-expanded={showSilent} onclick={() => (showSilent = !showSilent)}>
          <Icon name={showSilent ? "caretDown" : "caretRight"} />
          {silent.length} silent {silent.length === 1 ? "notification" : "notifications"}
        </button>
      </section>
      {#if showSilent}
        {@render rows(quiet)}
      {/if}
    {/if}
  </div>
</div>
