<script>
  /**
   * Files: both directions. Send — drop files anywhere on the window or pick
   * them, or hand over a line of text or a link. Browse — the phone's storage
   * opens in ewe Files like any folder. Received — what the phone sent while
   * this window was open, one click from opening.
   */
  import * as api from "../api.js";
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import { app, view, chooseAndSendFiles, sendText, openStorage, closeStorage } from "../state.svelte.js";
  import { fail } from "../toast.svelte.js";
  import { baseName, when } from "../format.js";

  const on = $derived(view.usable);
  const name = $derived(view.device?.name ?? "the phone");
  let text = $state("");
  let busy = $state(false);
  const isLink = $derived(/^https?:\/\/\S+$/.test(text.trim()));

  async function submit() {
    if (!text.trim() || busy) return;
    busy = true;
    const ok = await sendText(text);
    busy = false;
    if (ok) text = "";
  }
  const open = (f, reveal) => api.openFile(f.path, reveal).catch((e) => fail(e, "Couldn’t open that file."));
</script>

<div class="scroll">
  <div class="page">
    <header class="ewe-headerbar ewe-headerbar--page">
      <div class="ewe-headerbar__titles">
        <h1 class="ewe-headerbar__title">Files</h1>
        <p class="ewe-headerbar__subtitle">Move things between this computer and {name}.</p>
      </div>
    </header>

    {#if view.can("share") || !on}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Send to the phone</h2></div>
        <button type="button" class="ewe-drop dropzone" disabled={!on} onclick={chooseAndSendFiles}>
          <Icon name="upload" />
          <span class="ewe-drop__title">Drop files here, or choose them…</span>
          <span class="ewe-drop__hint">Dropping them anywhere on this window works too.</span>
        </button>
        <form
          class="sendline"
          onsubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <label class="ewe-input sendline__field">
            <Icon name={isLink ? "link" : "clipboard"} />
            <input bind:value={text} type="text" placeholder="Text or a link" aria-label="Text or a link to send" disabled={!on} spellcheck="false" />
          </label>
          <button type="submit" class="ewe-btn ewe-btn--secondary" disabled={!on || !text.trim() || busy}>
            {isLink ? "Open on the phone" : "Send to its clipboard"}
          </button>
        </form>
      </section>
    {/if}

    {#if view.can("sftp") || !on}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">The phone’s storage</h2></div>
        <div class="ewe-list">
          <div class="ewe-row ewe-row--tall">
            <span class="ewe-row__lead ewe-row__lead--tile"><Icon name="folderOpen" /></span>
            <div class="ewe-row__text">
              <div class="ewe-row__title">Browse {name}</div>
              <div class="ewe-row__desc">
                {#if app.storage.busy}
                  Connecting to the phone…
                {:else if app.storage.mounted}
                  Connected. It shows in ewe Files like any folder.
                {:else}
                  Opens its photos, downloads and documents in ewe Files.
                {/if}
              </div>
            </div>
            <div class="ewe-row__trail">
              {#if app.storage.mounted}
                <button type="button" class="ewe-btn ewe-btn--ghost" disabled={!on} onclick={closeStorage}>Disconnect</button>
              {/if}
              <button type="button" class="ewe-btn ewe-btn--secondary" disabled={!on || app.storage.busy || !app.storage.sshfs} onclick={openStorage}>
                {#if app.storage.busy}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--neutral"></span>{/if}
                Open
              </button>
            </div>
          </div>
        </div>
        {#if !app.storage.sshfs}
          <div class="ewe-alert ewe-alert--warning" role="status">
            <Icon name="warning" />
            <div class="ewe-alert__body">
              <span class="ewe-alert__title">Browsing needs sshfs</span>
              <span class="ewe-alert__desc">Install sshfs with Komble, then open this page again. Sending files works without it.</span>
            </div>
          </div>
        {/if}
      </section>
    {/if}

    <section class="group">
      <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Received</h2></div>
      {#if app.received.length}
        <div class="ewe-list">
          {#each app.received as f (f.path + f.at)}
            <div class="ewe-row">
              <span class="ewe-row__lead ewe-row__lead--tile"><Icon name="file" /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">{baseName(f.path)}</div>
                <div class="ewe-row__desc">{when(f.at, app.now)} · {f.path.replace(/\/[^/]*$/, "")}</div>
              </div>
              <div class="ewe-row__trail">
                <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => open(f, true)}>Show in folder</button>
                <button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" onclick={() => open(f, false)}>Open</button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="ewe-list">
          <Empty compact icon="download" title="Nothing received yet" desc="Share something to this computer from the phone and it shows up here." />
        </div>
      {/if}
    </section>
  </div>
</div>
