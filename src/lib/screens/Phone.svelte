<script>
  /**
   * Phone: the device at a glance. Who it is and how it is doing (battery,
   * mobile signal), the things you do to a phone from a desk — ring it, hand
   * it the clipboard or a file, open its storage — and the newest of what is
   * waiting on it. Every block leads to the page that has the rest.
   */
  import Icon from "../components/ui/Icon.svelte";
  import Avatar from "../components/Avatar.svelte";
  import {
    app, view, ring, ping, sendClipboard, chooseAndSendFiles, openStorage, openThread, mediaDo
  } from "../state.svelte.js";
  import { batteryIcon, signalIcon, deviceIcon, when, snippet } from "../format.js";

  const d = $derived(view.device);
  const on = $derived(view.usable);
  const battery = $derived(d?.battery ?? null);
  const signal = $derived(d?.signal ?? null);
  const signalWord = (s) => ["No signal", "Weak signal", "Fair signal", "Good signal", "Strong signal"][Math.max(0, Math.min(4, s))];

  const threads = $derived(app.threads.slice(0, 4));
  const notes = $derived(app.notifications.filter((n) => !n.silent).slice(0, 4));
  const playing = $derived(app.media && (app.media.title || app.media.playing) ? app.media : null);

  const actions = $derived(
    [
      view.can("findmyphone") && { id: "ring", icon: "ring", title: "Ring it", desc: "Plays a sound so you can find it", run: ring },
      view.can("clipboard") && { id: "clip", icon: "clipboard", title: "Send clipboard", desc: "What you copied here, on the phone", run: sendClipboard },
      view.can("share") && { id: "files", icon: "upload", title: "Send files…", desc: "Pictures, documents, anything", run: chooseAndSendFiles },
      view.can("sftp") && { id: "browse", icon: "folderOpen", title: "Browse its files", desc: "Opens the phone in ewe Files", run: openStorage },
      view.can("ping") && { id: "ping", icon: "ping", title: "Ping", desc: "A test notification on the phone", run: ping }
    ].filter(Boolean)
  );

  function go(route) {
    app.route = route;
  }
  function toThread(t) {
    app.route = "messages";
    openThread(t.thread);
  }
</script>

<div class="scroll">
  <div class="page page--wide">
    <header class="hero">
      <span class="hero__glyph" aria-hidden="true"><Icon name={deviceIcon(d?.type)} /></span>
      <div class="hero__text">
        <h1 class="hero__name">{d?.name}</h1>
        <div class="hero__state">
          {#if on}
            <span class="ewe-badge ewe-badge--success"><span class="ewe-badge__label">Connected</span></span>
          {:else}
            <span class="ewe-badge"><span class="ewe-badge__label">Not in reach</span></span>
          {/if}
          {#if on && signal}
            <span class="hero__fact"><Icon name={signalIcon(signal.strength)} /> {signal.type} · {signalWord(signal.strength)}</span>
          {/if}
        </div>
      </div>
      {#if battery}
        <div
          class="ewe-meter hero__battery {battery.charge <= 10 && !battery.charging ? 'ewe-meter--danger' : battery.charge <= 20 && !battery.charging ? 'ewe-meter--warning' : ''}"
          style="--value: {battery.charge}%"
        >
          <div class="ewe-meter__head">
            <Icon name={batteryIcon(battery.charge, battery.charging)} />
            <span>{battery.charging ? "Charging" : "Battery"}</span>
            <span class="ewe-meter__value">{battery.charge}%</span>
          </div>
          <div class="ewe-meter__track" role="img" aria-label="Battery {battery.charge}%"><div class="ewe-meter__fill"></div></div>
        </div>
      {/if}
    </header>

    {#if actions.length}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Do</h2></div>
        <div class="tiles">
          {#each actions as a (a.id)}
            <button
              type="button"
              class="ewe-card ewe-card--compact ewe-card--interactive tile"
              disabled={!on || (a.id === "browse" && app.storage.busy)}
              onclick={a.run}
            >
              <span class="ewe-card__icon"><Icon name={a.icon} /></span>
              <span class="tile__text">
                <span class="tile__title">{a.title}</span>
                <span class="tile__desc">{a.id === "browse" && app.storage.busy ? "Connecting…" : a.desc}</span>
              </span>
            </button>
          {/each}
        </div>
      </section>
    {/if}

    {#if playing && view.can("mprisremote")}
      <section class="group">
        <div class="ewe-section">
          <h2 class="ewe-section__title ewe-section__title--overline">Playing on the phone</h2>
          <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => go("media")}>Open Media</button>
        </div>
        <div class="ewe-list">
          <div class="ewe-row ewe-row--tall">
            <span class="ewe-row__lead">
              {#if playing.art}
                <img class="cover cover--sm" src={playing.art} alt="" />
              {:else}
                <span class="cover cover--sm cover--none"><Icon name="music" /></span>
              {/if}
            </span>
            <div class="ewe-row__text">
              <div class="ewe-row__title">{playing.title || "Nothing playing"}</div>
              <div class="ewe-row__desc">{[playing.artist, playing.player].filter(Boolean).join(" · ")}</div>
            </div>
            <div class="ewe-row__trail">
              <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost" title="Previous" aria-label="Previous" disabled={!on} onclick={() => mediaDo("Previous")}><Icon name="previous" /></button>
              <button type="button" class="ewe-iconbtn ewe-iconbtn--secondary ewe-iconbtn--round" title={playing.playing ? "Pause" : "Play"} aria-label={playing.playing ? "Pause" : "Play"} disabled={!on} onclick={() => mediaDo("PlayPause")}><Icon name={playing.playing ? "pause" : "play"} /></button>
              <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost" title="Next" aria-label="Next" disabled={!on} onclick={() => mediaDo("Next")}><Icon name="next" /></button>
            </div>
          </div>
        </div>
      </section>
    {/if}

    <div class="columns">
      {#if view.can("sms") || app.threads.length || (!on && d?.supported.includes("sms"))}
        <section class="group">
          <div class="ewe-section">
            <h2 class="ewe-section__title ewe-section__title--overline">Messages</h2>
            <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => go("messages")}>Show all</button>
          </div>
          <div class="ewe-list">
            {#each threads as t (t.thread)}
              <button type="button" class="ewe-row ewe-row--interactive ewe-row--tall rowbtn" onclick={() => toThread(t)}>
                <span class="ewe-row__lead"><Avatar address={t.addresses[0]} group={t.addresses.length > 1} /></span>
                <span class="ewe-row__text">
                  <span class="ewe-row__title" class:is-strong={t.type === 1 && !t.read}>{view.title(t.addresses)}</span>
                  <span class="ewe-row__desc">{t.type === 1 ? "" : "You: "}{snippet(t)}</span>
                </span>
                <span class="ewe-row__trail">{when(t.date, app.now)}</span>
              </button>
            {:else}
              <div class="ewe-row"><div class="ewe-row__text"><div class="ewe-row__desc">{!on ? "Shows when the phone is in reach." : app.threadsReady ? "No messages on the phone." : "Reading messages…"}</div></div></div>
            {/each}
          </div>
        </section>
      {/if}

      {#if view.can("notifications") || app.notifications.length || (!on && d?.supported.includes("notifications"))}
        <section class="group">
          <div class="ewe-section">
            <h2 class="ewe-section__title ewe-section__title--overline">Notifications</h2>
            <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => go("notifications")}>Show all</button>
          </div>
          <div class="ewe-list">
            {#each notes as n (n.id)}
              <button type="button" class="ewe-row ewe-row--interactive ewe-row--tall rowbtn" onclick={() => go("notifications")}>
                <span class="ewe-row__lead">
                  {#if n.icon}<img class="appicon" src={n.icon} alt="" />{:else}<span class="ewe-row__lead--tile appicon"><Icon name="bell" /></span>{/if}
                </span>
                <span class="ewe-row__text">
                  <span class="ewe-row__title">{n.title || n.app}</span>
                  <span class="ewe-row__desc">{[n.title ? n.app : "", n.text].filter(Boolean).join(" · ")}</span>
                </span>
              </button>
            {:else}
              <div class="ewe-row"><div class="ewe-row__text"><div class="ewe-row__desc">{!on ? "Shows when the phone is in reach." : app.notificationsReady ? "Nothing waiting on the phone." : "Reading notifications…"}</div></div></div>
            {/each}
          </div>
        </section>
      {/if}
    </div>
  </div>
</div>
