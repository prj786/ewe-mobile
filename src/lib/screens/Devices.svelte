<script module>
  // What KDE Connect calls a plugin, in the words of what it lets happen.
  // In reading order: what you came for first, the rarely-touched last. A
  // feature the list doesn't know still shows, under its own name.
  const FEATURES = [
    ["sms", "Messages", "Read and send text messages from this computer."],
    ["contacts", "Contacts", "Put names and faces to numbers."],
    ["notifications", "Notifications", "Show the phone’s notifications here."],
    ["telephony", "Calls", "Say who is calling."],
    ["battery", "Battery", "Show the phone’s battery level."],
    ["connectivity_report", "Mobile signal", "Show the phone’s network and signal."],
    ["share", "Share", "Send files, links and text both ways."],
    ["clipboard", "Clipboard", "Share what you copy between the two."],
    ["sftp", "Phone storage", "Browse the phone’s files from this computer."],
    ["mprisremote", "The phone’s player", "Control what plays on the phone."],
    ["findmyphone", "Ring the phone", "Make the phone ring so you can find it."],
    ["ping", "Ping", "Send a test notification either way."],
    ["remotecontrol", "Pointer on the phone", "Move the phone’s pointer with this computer’s mouse."],
    ["remotekeyboard", "Type on the phone", "Type into the phone with this computer’s keyboard."],
    ["mpriscontrol", "This computer’s player", "Let the phone control what plays here."],
    ["pausemusic", "Pause for calls", "Pause what plays here while you are on a call."],
    ["systemvolume", "Volume", "Let the phone change this computer’s volume."],
    ["mousepad", "Phone as touchpad", "Let the phone move the pointer and type here."],
    ["presenter", "Presentation remote", "Let the phone flip slides and point at the screen."],
    ["runcommand", "Run commands", "Let the phone run the commands you set up here."],
    ["sendnotifications", "Send notifications", "Show this computer’s notifications on the phone."],
    ["findthisdevice", "Find this computer", "Let the phone make this computer ring."],
    ["lockdevice", "Lock", "Let either lock the other."],
    ["screensaver_inhibit", "Stay awake", "Keep this computer awake while the phone is near."],
    ["virtualmonitor", "Second screen", "Use the phone as another display."],
    ["digitizer", "Drawing tablet", "Draw here with the phone’s pen."],
    ["shareinputdevices", "Share mouse and keyboard", "Move this computer’s pointer across to the other device."],
    ["shareinputdevicesremote", "Take a shared mouse and keyboard", "Let the other device’s pointer come across to here."],
    ["mmtelephony", "Calls through a modem", "Say who is calling on a modem this computer has."],
    ["remotecommands", "Commands on the other device", "Run commands the other device set up."],
    ["remotesystemvolume", "The other device’s volume", "Change the volume of the other device."],
    ["bigscreen", "TV remote", "Drive a Plasma Bigscreen TV."]
  ];
  const ORDER = new Map(FEATURES.map((f, i) => [f[0], i]));
  const WORDS = new Map(FEATURES.map((f) => [f[0], { title: f[1], desc: f[2] }]));
</script>

<script>
  /**
   * Devices: which phone this is about, and what it may do. Pair a phone
   * (both sides show the same key — compare them), pick the one in use when
   * there are several, name this computer as phones see it, and switch
   * single features on or off. With no phone linked yet, this page is the
   * welcome: three steps and the list of what is nearby.
   */
  import sheep from "../../assets/ewe-mark.svg?raw";
  import * as api from "../api.js";
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import Toggle from "../components/ui/Toggle.svelte";
  import Dialog from "../components/ui/Dialog.svelte";
  import Menu from "../components/ui/Menu.svelte";
  import { app, view, selectDevice, pairing, lookAgain, refreshDevices } from "../state.svelte.js";
  import { toast, fail } from "../toast.svelte.js";
  import { deviceIcon } from "../format.js";

  let { welcome = false } = $props();

  const mine = $derived(app.devices.filter((x) => x.paired));
  const nearby = $derived(app.devices.filter((x) => !x.paired && (x.reachable || x.pairState !== 0)));
  const d = $derived(view.device);

  let looking = $state(false);
  async function look() {
    if (looking) return;
    looking = true;
    await lookAgain();
    // the phones answer over a second or two
    setTimeout(async () => {
      await refreshDevices();
      looking = false;
    }, 1500);
  }

  // ── this computer's name ──────────────────────────────────────────────────
  let renaming = $state(false);
  let newName = $state("");
  function startRename() {
    newName = app.self.name;
    renaming = true;
  }
  async function saveName() {
    const n = newName.trim();
    if (!n || n === app.self.name) {
      renaming = false;
      return;
    }
    try {
      await api.setName(n);
      app.self.name = n;
      renaming = false;
      toast(`Phones now see this computer as **${n}**`, "success");
    } catch (e) {
      fail(e, "Couldn’t rename this computer.");
    }
  }
  const focusOnMount = (node) => {
    node.focus();
    node.select();
  };

  // ── unpairing asks first ──────────────────────────────────────────────────
  let leaving = $state(null);
  async function unpair() {
    const dev = leaving;
    leaving = null;
    if (dev) await pairing(dev.id, "unpair");
  }

  // ── what the chosen device may do ─────────────────────────────────────────
  let features = $state([]);
  let featuresFor = "";
  const featureKey = $derived(d?.paired ? `${d.id}:${d.supported.join(",")}` : "");
  $effect(() => {
    const key = featureKey;
    const id = d?.id;
    if (!key) {
      features = [];
      featuresFor = "";
      return;
    }
    if (key === featuresFor) return;
    featuresFor = key;
    api
      .plugins(id)
      .then((list) => {
        if (featuresFor !== key) return;
        features = [...list].sort((a, b) => (ORDER.get(a.id) ?? 999) - (ORDER.get(b.id) ?? 999));
      })
      .catch(() => {
        if (featuresFor === key) features = [];
      });
  });
  const words = (id) => WORDS.get(id) ?? { title: id.replace(/_/g, " "), desc: "" };

  async function setFeature(f, enabled) {
    const before = f.enabled;
    f.enabled = enabled;
    try {
      await api.setPlugin(d.id, f.id, enabled);
    } catch (e) {
      f.enabled = before;
      fail(e, "Couldn’t change that.");
    }
  }

  const stateWord = (x) => {
    if (!x.reachable) return "Not in reach";
    const b = x.battery;
    return b ? `Connected · ${b.charge}%${b.charging ? " charging" : ""}` : "Connected";
  };
</script>

<div class="scroll">
  <div class="page">
    {#if welcome}
      <header class="welcome">
        <span class="welcome__mark" aria-hidden="true">{@html sheep}</span>
        <h1 class="welcome__title">Link your phone</h1>
        <p class="welcome__desc">Messages, contacts, notifications, music and files from your phone, on this computer.</p>
        <ol class="welcome__steps">
          <li class="ewe-step is-current">
            <span class="ewe-step__dot">1</span>
            <span>Install KDE Connect on the phone<span class="ewe-step__sub">From Google Play or F-Droid; on an iPhone, the App Store.</span></span>
          </li>
          <li class="ewe-step is-current">
            <span class="ewe-step__dot">2</span>
            <span>Open it, on the same Wi-Fi as this computer<span class="ewe-step__sub">This computer shows there as {app.self.name || "its name"}.</span></span>
          </li>
          <li class="ewe-step is-current">
            <span class="ewe-step__dot">3</span>
            <span>Choose the phone below and accept on it<span class="ewe-step__sub">Both show the same key. Check that they match.</span></span>
          </li>
        </ol>
      </header>
    {:else}
      <header class="ewe-headerbar ewe-headerbar--page">
        <div class="ewe-headerbar__titles">
          <h1 class="ewe-headerbar__title">Devices</h1>
          <p class="ewe-headerbar__subtitle">The phones and tablets linked to this computer.</p>
        </div>
      </header>
    {/if}

    {#if mine.length}
      <section class="group">
        <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">Your devices</h2></div>
        <div class="ewe-list">
          {#each mine as x (x.id)}
            <div class="ewe-row ewe-row--tall" class:is-selected={x.id === app.deviceId && mine.length > 1}>
              <span class="ewe-row__lead ewe-row__lead--tile"><Icon name={deviceIcon(x.type)} /></span>
              <div class="ewe-row__text">
                <div class="ewe-row__title">{x.name}</div>
                <div class="ewe-row__desc">{stateWord(x)}{x.id === app.deviceId && mine.length > 1 ? " · In use" : ""}</div>
              </div>
              <div class="ewe-row__trail">
                {#if x.id !== app.deviceId}
                  <button type="button" class="ewe-btn ewe-btn--secondary ewe-btn--sm" onclick={() => selectDevice(x.id)}>Use this one</button>
                {/if}
                <Menu
                  label="More for {x.name}"
                  items={[{ label: "Unpair…", icon: "unlink", danger: true, run: () => (leaving = x) }]}
                />
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <section class="group">
      <div class="ewe-section">
        <div class="ewe-section__text">
          <h2 class="ewe-section__title ewe-section__title--overline">Nearby</h2>
        </div>
        <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" disabled={looking} onclick={look}>
          {#if looking}<span class="ewe-spinner ewe-spinner--sm ewe-spinner--neutral"></span>{:else}<Icon name="refresh" />{/if}
          Look again
        </button>
      </div>
      <div class="ewe-list">
        {#each nearby as x (x.id)}
          <div class="ewe-row ewe-row--tall">
            <span class="ewe-row__lead ewe-row__lead--tile"><Icon name={deviceIcon(x.type)} /></span>
            <div class="ewe-row__text">
              <div class="ewe-row__title">{x.name}</div>
              <div class="ewe-row__desc pairline">
                {#if x.pairState === 1}
                  Accept on the device. Both should show <span class="ewe-kv__mono pairkey">{x.verificationKey}</span>
                {:else if x.pairState === 2}
                  Asks to pair. Both should show <span class="ewe-kv__mono pairkey">{x.verificationKey}</span>
                {:else}
                  Not paired
                {/if}
              </div>
            </div>
            <div class="ewe-row__trail">
              {#if x.pairState === 1}
                <span class="ewe-spinner ewe-spinner--sm"></span>
                <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => pairing(x.id, "cancel")}>Cancel</button>
              {:else if x.pairState === 2}
                <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => pairing(x.id, "cancel")}>Decline</button>
                <button type="button" class="ewe-btn ewe-btn--primary ewe-btn--sm" onclick={() => pairing(x.id, "accept")}>Accept</button>
              {:else}
                <button type="button" class="ewe-btn ewe-btn--primary ewe-btn--sm" onclick={() => pairing(x.id, "request")}>Pair</button>
              {/if}
            </div>
          </div>
        {:else}
          <Empty
            compact
            icon="devices"
            title={mine.length ? "No other devices nearby" : "No devices found yet"}
            desc={mine.length
              ? "A device shows here when KDE Connect is open on it, on the same network."
              : "Open KDE Connect on the phone, on the same Wi-Fi, then look again."}
          />
        {/each}
      </div>
    </section>

    <section class="group">
      <div class="ewe-section"><h2 class="ewe-section__title ewe-section__title--overline">This computer</h2></div>
      <div class="ewe-list">
        <div class="ewe-row ewe-row--tall">
          <span class="ewe-row__lead ewe-row__lead--tile"><Icon name="laptop" /></span>
          {#if renaming}
            <form
              class="rename"
              onsubmit={(e) => {
                e.preventDefault();
                saveName();
              }}
            >
              <label class="ewe-input rename__field">
                <input
                  use:focusOnMount
                  bind:value={newName}
                  type="text"
                  maxlength="32"
                  aria-label="This computer’s name on phones"
                  onkeydown={(e) => e.key === "Escape" && (renaming = false)}
                />
              </label>
              <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={() => (renaming = false)}>Cancel</button>
              <button type="submit" class="ewe-btn ewe-btn--primary ewe-btn--sm" disabled={!newName.trim()}>Save</button>
            </form>
          {:else}
            <div class="ewe-row__text">
              <div class="ewe-row__title">{app.self.name || "This computer"}</div>
              <div class="ewe-row__desc">The name phones see</div>
            </div>
            <div class="ewe-row__trail">
              <button type="button" class="ewe-btn ewe-btn--ghost ewe-btn--sm" onclick={startRename}>Rename</button>
            </div>
          {/if}
        </div>
      </div>
    </section>

    {#if d?.paired && features.length}
      <section class="group">
        <div class="ewe-section">
          <div class="ewe-section__text">
            <h2 class="ewe-section__title ewe-section__title--overline">What {d.name} may do</h2>
          </div>
        </div>
        <div class="ewe-list">
          {#each features as f (f.id)}
            {@const w = words(f.id)}
            <div class="ewe-row ewe-row--tall">
              <div class="ewe-row__text">
                <div class="ewe-row__title">{w.title}</div>
                {#if w.desc}<div class="ewe-row__desc">{w.desc}</div>{/if}
              </div>
              <div class="ewe-row__trail">
                <Toggle on={f.enabled} label={w.title} toggled={(v) => setFeature(f, v)} />
              </div>
            </div>
          {/each}
        </div>
        <p class="note">ewe Mobile uses KDE Connect to talk to the phone. The phone has the same list, for what this computer may do there.</p>
      </section>
    {/if}
  </div>
</div>

{#if leaving}
  <Dialog
    title="Unpair {leaving.name}?"
    icon="unlink"
    tone="danger"
    confirm="Unpair {leaving.name}"
    onConfirm={unpair}
    onCancel={() => (leaving = null)}
  >
    Its messages, notifications and files stop showing here. You can pair it again at any time.
  </Dialog>
{/if}
