// Everything the window knows, and everything it can do.
//
// The daemon is the source of truth; this is a reading of it. Commands ask
// (api.js), the `kdc` event says when to ask again (onEvent). Per-device
// data is dropped and read again whenever the chosen phone changes or comes
// back in reach — nothing here outlives the window.

import * as api from "./api.js";
import { numberKey } from "./format.js";
import { toast, fail } from "./toast.svelte.js";

/** The pages. The ids are `ewe-mobile --page <id>` (public) — never rename. */
export const PAGES = [
  { id: "phone", label: "Phone", icon: "phone" },
  { id: "messages", label: "Messages", icon: "messages", needs: ["sms"] },
  { id: "contacts", label: "Contacts", icon: "contacts", needs: ["contacts"] },
  { id: "notifications", label: "Notifications", icon: "bell", needs: ["notifications"] },
  { id: "media", label: "Media", icon: "music", needs: ["mprisremote"] },
  { id: "files", label: "Files", icon: "folder", needs: ["share", "sftp"] },
  { id: "screen", label: "Screen", icon: "screenShare" },
  { id: "apps", label: "Apps", icon: "grid" },
  { id: "remote", label: "Remote", icon: "touchpad", needs: ["remotecontrol", "remotekeyboard"] },
  { id: "devices", label: "Devices", icon: "devices" }
];

const FIRST_LOAD = 40; // messages asked for when a thread opens
const MORE = 60; // … and with each "Show earlier messages"

export const app = $state({
  boot: "starting", // starting | ready | failed
  error: "",
  installed: true,
  running: false,
  self: { name: "", id: "" },
  devices: [],
  deviceId: "",
  route: "phone",
  now: Date.now(), // ticks so "5 min" stays true

  notifications: [],
  notificationsReady: false,

  threads: [], // the newest message of each thread, newest thread first
  threadsReady: false,
  messages: {}, // thread id → its messages, oldest first
  loading: {}, // thread id → the phone is sending its messages
  asked: {}, // thread id → how many messages were asked for so far
  pending: [], // sent from here, not yet confirmed by the phone
  thread: null, // the open thread
  compose: null, // { addresses } — a new message that is not a thread yet

  contacts: [],
  contactsReady: false,
  photos: {}, // contact uid → data URL ("" = it has none)

  media: null,
  mediaAt: 0, // when media.position was read
  storage: { mounted: false, sshfs: true, busy: false },
  received: [], // files the phone sent while the window was open
  keyboard: false, // the phone says it takes keys (its accessibility service, or its keyboard, is on)
  keyAcks: 0, // keys the phone said it typed, counted
  call: null, // { event, number, name }

  // the phone's apps (C1 live, C2 cached, or scrcpy's list as the fallback)
  apps: { source: "", list: [], ready: false, updated: 0, fallback: false, error: "" },
  adb: null, // the debugging link: { tools: { adb, scrcpy }, paired, connected, serial, model, services }
  adbAt: 0,
  windows: [], // scrcpy windows this app opened: { class, title, package }
  launcherApps: true // "Show the phone's apps in the launcher"
});

const current = $derived(app.devices.find((d) => d.id === app.deviceId) ?? null);

// number → contact, by what two spellings of a number share
const byNumber = $derived.by(() => {
  const m = new Map();
  for (const c of app.contacts) {
    for (const p of c.phones) {
      const k = numberKey(p.number);
      if (k && !m.has(k)) m.set(k, c);
    }
  }
  return m;
});

const addressesKey = (addresses) => [...new Set((addresses || []).map(numberKey))].sort().join(",");

export const view = {
  /** the chosen device, or null */
  get device() {
    return current;
  },
  /** paired and in reach: its pages work */
  get usable() {
    return !!current && current.paired && current.reachable;
  },
  /** does the phone offer (and this computer allow) any of these features? */
  can(...plugins) {
    return !!current && plugins.some((p) => current.loaded.includes(p));
  },
  contact(address) {
    return byNumber.get(numberKey(address)) ?? null;
  },
  /** the name a number goes by, or the number */
  name(address) {
    return byNumber.get(numberKey(address))?.name ?? String(address ?? "");
  },
  /** who a thread is with: "Nino" · "Nino, Gio" · "+1 202 555 …" */
  title(addresses) {
    const names = (addresses || []).map((a) => this.name(a));
    return names.length ? names.join(", ") : "Unknown sender";
  },
  get unread() {
    return app.threads.filter((t) => t.type === 1 && !t.read).length;
  },
  get alerts() {
    return app.notifications.filter((n) => !n.silent).length;
  },
  addresses(thread) {
    return app.threads.find((t) => t.thread === thread)?.addresses ?? [];
  },
  threadFor(addresses) {
    const key = addressesKey(addresses);
    if (!key) return null;
    return app.threads.find((t) => addressesKey(t.addresses) === key)?.thread ?? null;
  },
  /** may there be older messages on the phone than the ones shown? */
  hasEarlier(thread) {
    const have = app.messages[thread]?.length ?? 0;
    return !app.loading[thread] && have >= (app.asked[thread] ?? 0) && have > 0;
  },
  pendingFor(thread, compose) {
    const key = compose ? addressesKey(compose.addresses) : null;
    return app.pending.filter((p) => (thread != null && p.thread === thread) || (key && p.key === key));
  },
  /** the launcher icon of a phone app, from its list (C2), or "" */
  appIcon(pkg) {
    if (!pkg) return "";
    return app.apps.list.find((a) => a.package === pkg)?.icon ?? "";
  },
  /** is the debugging link up — can an app open in its own window here? */
  get adbReady() {
    return !!app.adb?.connected;
  },
  /** the phone runs ewe Mobile for Android (it sent its apps list) */
  get eweOnPhone() {
    return app.apps.source === "phone";
  },
  get mirroring() {
    return app.windows.some((w) => w.class === "ewe-mobile-screen");
  }
};

// ── start ───────────────────────────────────────────────────────────────────

const REMEMBER = "ewe-mobile.device";
const remembered = () => {
  try {
    return localStorage.getItem(REMEMBER) || "";
  } catch {
    return "";
  }
};

function chooseDevice() {
  const list = app.devices;
  if (list.some((d) => d.id === app.deviceId && d.paired)) return;
  const saved = remembered();
  const pick =
    list.find((d) => d.id === saved && d.paired) ||
    list.find((d) => d.paired && d.reachable) ||
    list.find((d) => d.paired) ||
    null;
  app.deviceId = pick ? pick.id : "";
}

export function selectDevice(id) {
  if (!app.devices.some((d) => d.id === id)) return;
  app.deviceId = id;
  try {
    localStorage.setItem(REMEMBER, id);
  } catch {
    /* a private window: the choice lasts for this session */
  }
}

let devicesSeq = 0;
export async function refreshDevices() {
  const mine = ++devicesSeq;
  try {
    const st = await api.status();
    const list = st.running ? await api.devices() : [];
    if (mine !== devicesSeq) return; // a later look has already answered
    app.installed = !!st.installed;
    app.running = !!st.running;
    app.self = { name: st.name || "", id: st.id || "" };
    app.devices = list;
    chooseDevice();
  } catch (e) {
    if (mine !== devicesSeq) return;
    app.error = String(e);
    if (app.boot === "starting") app.boot = "failed";
  }
}

let devicesTimer = null;
function devicesSoon() {
  clearTimeout(devicesTimer);
  devicesTimer = setTimeout(refreshDevices, 200);
}

/** read the daemon, starting it when it is installed and not running */
export async function boot() {
  app.boot = "starting";
  app.error = "";
  await refreshDevices();
  if (app.boot === "failed") return;
  if (app.installed && !app.running) {
    try {
      await api.daemonStart();
    } catch {
      /* the page says it is not running and offers to try again */
    }
    // it announces itself (the `daemon` event); look a few times meanwhile
    for (let i = 0; i < 10 && !app.running; i++) {
      await new Promise((r) => setTimeout(r, 400));
      await refreshDevices();
    }
  }
  // a bus that failed while we waited stays the thing the page says
  if (app.boot !== "failed") app.boot = "ready";
}

/** forget what was read from the last device */
export function resetDevice() {
  app.notifications = [];
  app.notificationsReady = false;
  app.threads = [];
  app.threadsReady = false;
  app.messages = {};
  app.loading = {};
  app.asked = {};
  app.pending = [];
  app.thread = null;
  app.compose = null;
  app.contacts = [];
  app.contactsReady = false;
  app.photos = {};
  photoAsked.clear();
  app.media = null;
  app.storage = { mounted: false, sshfs: true, busy: false };
  app.keyboard = false;
  app.keyAcks = 0;
  app.call = null;
  app.apps = { source: "", list: [], ready: false, updated: 0, fallback: false, error: "" };
  app.adb = null;
  app.adbAt = 0;
}

/** read everything the chosen phone offers (called when it becomes usable) */
export function loadDevice() {
  if (!view.usable) return;
  if (view.can("notifications")) loadNotifications();
  if (view.can("sms")) {
    loadThreads();
    // an open conversation whose messages never came (the phone dropped
    // off) is asked for again now that it is back
    if (app.thread != null && !app.asked[app.thread]) askThread(app.thread, 0, FIRST_LOAD);
  }
  if (view.can("mprisremote")) loadMedia();
  if (view.can("sftp")) loadStorage();
  if (view.can("remotekeyboard")) loadKeyboard();
  loadApps(); // live now: the cache was read while it was away
}

// ── the phone's apps, and the debugging link ────────────────────────────────

export async function loadApps() {
  const id = app.deviceId;
  if (!id) return;
  try {
    const r = await api.apps(id);
    if (id !== app.deviceId) return;
    // the scrcpy list stays while the phone has none of its own
    if (r.source === "none" && app.apps.fallback && app.apps.list.length) {
      app.apps.ready = true;
      return;
    }
    app.apps = { source: r.source, list: r.apps || [], ready: true, updated: r.updated || 0, fallback: false, error: "" };
  } catch (e) {
    if (id === app.deviceId) app.apps = { ...app.apps, ready: true, error: String(e?.message ?? e) };
  }
}

let appsTimer = null;
function appsSoon() {
  clearTimeout(appsTimer);
  appsTimer = setTimeout(loadApps, 300);
}

/** ask the phone for its list again (C1 `ewe:refresh`); the event brings it */
export async function refreshApps() {
  try {
    await api.appsRefresh(app.deviceId);
    toast("Asked the phone for its apps");
  } catch (e) {
    fail(e, "Couldn’t ask the phone for its apps.");
  }
}

/** the fallback: what scrcpy lists (no icons; 12 s the first time) */
export async function loadScrcpyApps(refresh = false) {
  const id = app.deviceId;
  app.apps = { ...app.apps, ready: false, error: "" };
  try {
    const r = await api.appsScrcpy(id, refresh);
    if (id !== app.deviceId) return;
    app.apps = { source: "scrcpy", list: r.apps || [], ready: true, updated: r.updated || 0, fallback: true, error: "" };
  } catch (e) {
    if (id === app.deviceId) app.apps = { ...app.apps, ready: true, error: String(e?.message ?? e) };
  }
}

/** open an app: here (its own window), on the phone, or wherever it can */
export async function openApp(a, where = "auto") {
  try {
    const went = await api.appOpen(app.deviceId, a.package, a.name, where);
    if (went === "here") {
      toast(`Opening **${a.name}** in its own window…`);
      setTimeout(loadWindows, 1500);
    } else toast(`**${a.name}** opened on ${current?.name ?? "the phone"}`, "success");
    return went;
  } catch (e) {
    fail(e, `Couldn’t open ${a.name}.`);
    return "";
  }
}

export async function loadAdb() {
  const id = app.deviceId;
  if (!id) return;
  try {
    const st = await api.adbStatus(id);
    if (id === app.deviceId) {
      app.adb = st;
      app.adbAt = Date.now();
    }
  } catch (e) {
    if (id === app.deviceId) app.adb = { tools: { adb: false, scrcpy: false }, paired: false, connected: false, error: String(e?.message ?? e) };
  }
}

export async function pairAdb(host, port, code) {
  try {
    await api.adbPair(host, port, code);
    toast("Paired with the phone for debugging", "success");
    await loadAdb();
    return true;
  } catch (e) {
    fail(e, "The phone didn’t take the code.");
    return false;
  }
}

export async function connectAdb(host, port) {
  try {
    await api.adbConnect(host, port);
    await loadAdb();
    return true;
  } catch (e) {
    fail(e, "Couldn’t connect to the phone.");
    return false;
  }
}

export async function offerTools() {
  try {
    if (!(await api.toolsOffer())) toast("Komble isn’t installed. Install android-tools and scrcpy with pacman.", "warning");
  } catch (e) {
    fail(e, "Couldn’t open Komble.");
  }
}

export async function loadWindows() {
  try {
    const st = await api.windowsState();
    app.windows = st.windows || [];
    for (const d of st.died || []) {
      if (!d.ok || d.words) toast(`**${d.title}** closed${d.words ? `: ${d.words}` : "."}`, d.ok ? "info" : "warning");
    }
  } catch {
    /* the page keeps what it had */
  }
}

export async function startScreen(opts) {
  try {
    await api.screenStart(app.deviceId, opts);
    toast(`Mirroring **${current?.name ?? "the phone"}** in its own window`, "success");
    setTimeout(loadWindows, 800);
    return true;
  } catch (e) {
    fail(e, "Couldn’t start the mirror.");
    return false;
  }
}

export async function stopWindow(cls) {
  try {
    await api.windowStop(cls);
  } catch {
    /* it may have closed by itself */
  }
  app.windows = app.windows.filter((w) => w.class !== cls);
  loadWindows();
}

export async function loadLauncherSetting() {
  try {
    app.launcherApps = !!(await api.launcherSetting());
  } catch {
    /* the default (on) stands */
  }
}

export async function setLauncherApps(on) {
  const was = app.launcherApps;
  app.launcherApps = on;
  try {
    await api.setLauncherSetting(on);
    toast(on ? "The phone’s apps show in the launcher" : "The phone’s apps are hidden from the launcher", "success");
  } catch (e) {
    app.launcherApps = was;
    fail(e, "Couldn’t change that setting.");
  }
}

// ── notifications ───────────────────────────────────────────────────────────

export async function loadNotifications() {
  const id = app.deviceId;
  try {
    const list = await api.notifications(id);
    if (id !== app.deviceId) return;
    app.notifications = list;
  } catch {
    if (id === app.deviceId) app.notifications = [];
  } finally {
    if (id === app.deviceId) app.notificationsReady = true;
  }
}

let notesTimer = null;
function notificationsSoon() {
  clearTimeout(notesTimer);
  notesTimer = setTimeout(loadNotifications, 150);
}

export async function dismissNotification(n) {
  // gone at once here; the daemon's "removed" signal confirms it
  app.notifications = app.notifications.filter((x) => x.id !== n.id);
  try {
    await api.notificationDismiss(app.deviceId, n.id);
  } catch (e) {
    fail(e, "Couldn’t dismiss that notification.");
    loadNotifications();
  }
}

export async function dismissAll(list) {
  const ids = new Set(list.filter((n) => n.dismissable).map((n) => n.id));
  app.notifications = app.notifications.filter((x) => !ids.has(x.id));
  const dev = app.deviceId;
  await Promise.allSettled([...ids].map((id) => api.notificationDismiss(dev, id)));
  loadNotifications();
}

export async function replyNotification(n, text) {
  try {
    await api.notificationReply(app.deviceId, n.replyId, text);
    toast(`Reply sent to **${n.title || n.app}**`, "success");
    return true;
  } catch (e) {
    fail(e, "Couldn’t send the reply.");
    return false;
  }
}

// ── messages ────────────────────────────────────────────────────────────────

function ingest(m) {
  if (!m || m.thread == null || m.thread < 0) return;
  app.threadsReady = true;
  const i = app.threads.findIndex((t) => t.thread === m.thread);
  if (i < 0) app.threads.push(m);
  else if (m.date >= app.threads[i].date) app.threads[i] = m;
  app.threads.sort((a, b) => b.date - a.date);

  // read the list back after creating it: the assignment itself would hand
  // over the plain array, and changes to that one are not seen
  if (!app.messages[m.thread]) app.messages[m.thread] = [];
  const list = app.messages[m.thread];
  const j = list.findIndex((x) => x.uid === m.uid && x.date === m.date);
  if (j >= 0) list[j] = m;
  else {
    // oldest first; most arrivals belong at an end
    let at = list.length;
    while (at > 0 && list[at - 1].date > m.date) at--;
    list.splice(at, 0, m);
  }
  settle(m);
}

/** a message of ours the phone now reports: its "Sending…" copy is done */
function settle(m) {
  if (m.type === 1) return;
  const key = addressesKey(m.addresses);
  const body = String(m.body ?? "").trim();
  // Ours when it says the same, to the same people, and is newer than the
  // thread was when we sent — measured on the phone's clock alone, so a
  // phone that is minutes off still settles, and an older message that
  // happens to say the same ("ok") never does.
  const i = app.pending.findIndex(
    (p) => p.body === body && (p.thread === m.thread || p.key === key) && m.date > p.after
  );
  if (i >= 0) app.pending.splice(i, 1);
  // a new message became a thread: show the thread
  if (app.compose && addressesKey(app.compose.addresses) === key) {
    app.compose = null;
    openThread(m.thread);
  }
}

export async function loadThreads() {
  const id = app.deviceId;
  try {
    const list = await api.conversations(id);
    if (id !== app.deviceId) return;
    for (const m of list) ingest(m);
    // A daemon that has not asked the phone yet answers with nothing and the
    // conversations follow as events: "no messages" is only true once the
    // phone has had a moment to answer (the first message says so sooner).
    if (!list.length) {
      setTimeout(() => {
        if (id === app.deviceId) app.threadsReady = true;
      }, 4000);
    }
  } catch (e) {
    if (id === app.deviceId) {
      app.threadsReady = true;
      fail(e, "Couldn’t read the messages.");
    }
  }
}

async function askThread(thread, start, end) {
  app.asked[thread] = end;
  app.loading[thread] = true;
  // not asked after all: the same window can be asked for again
  const unask = () => {
    if (app.asked[thread] === end) app.asked[thread] = start;
    app.loading[thread] = false;
  };
  try {
    await api.conversationLoad(app.deviceId, thread, start, end);
  } catch (e) {
    unask();
    fail(e, "Couldn’t load that conversation.");
    return;
  }
  // "loaded" normally says so; a phone that never answers must not spin
  // forever — and must be asked again the next time the thread opens
  setTimeout(() => {
    if (app.loading[thread] && app.asked[thread] === end) unask();
  }, 8000);
}

export function openThread(thread) {
  app.thread = thread;
  app.compose = null;
  if (!app.asked[thread]) askThread(thread, 0, FIRST_LOAD);
}

export function closeThread() {
  app.thread = null;
  app.compose = null;
}

export function loadEarlier(thread) {
  const from = app.asked[thread] ?? 0;
  askThread(thread, from, from + MORE);
}

/** write to these numbers: their thread when there is one, else a new message */
export function startMessage(addresses) {
  const list = (addresses || []).map((a) => String(a).trim()).filter(Boolean);
  if (!list.length) return;
  app.route = "messages";
  const t = view.threadFor(list);
  if (t != null) openThread(t);
  else {
    app.thread = null;
    app.compose = { addresses: list };
  }
}

let pendingSeq = 0;
const pendingById = (id) => app.pending.find((p) => p.id === id);
// the phone reports a sent message within seconds; say so when it doesn't
function expectReport(id) {
  setTimeout(() => {
    const p = pendingById(id);
    if (p && p.state === "sending") p.state = "unconfirmed";
  }, 30000);
}
async function hand(id) {
  const p = pendingById(id);
  if (!p) return;
  try {
    await api.sendMessage(app.deviceId, p.thread, [...p.addresses], p.body);
    expectReport(id);
  } catch (e) {
    const q = pendingById(id);
    if (q) {
      q.state = "failed";
      q.error = String(e?.message ?? e);
    }
  }
}

/** send into the open thread (or the new message); true when it was handed over */
export async function sendMessage(text) {
  const body = String(text ?? "").trim();
  if (!body) return false;
  const composing = app.compose;
  const thread = composing ? view.threadFor(composing.addresses) : app.thread;
  const addresses = composing ? composing.addresses : view.addresses(thread);
  if (thread == null && !addresses.length) return false;

  const id = ++pendingSeq;
  // `after`: how new the thread was, by the phone's clock, when this was sent
  const after = thread != null ? (app.threads.find((t) => t.thread === thread)?.date ?? 0) : 0;
  app.pending.push({
    id, thread, key: addressesKey(addresses), addresses: [...addresses], body, after, date: Date.now(),
    state: "sending", error: ""
  });
  // From here the message is in the thread — as "Sending…", or as "Not sent"
  // with Try again — so the box does not get the words back as well.
  await hand(id);
  return true;
}

export function dropPending(id) {
  const i = app.pending.findIndex((p) => p.id === id);
  if (i >= 0) app.pending.splice(i, 1);
}

export async function retryPending(id) {
  const p = pendingById(id);
  if (!p) return;
  p.state = "sending";
  p.error = "";
  p.date = Date.now();
  await hand(id);
}

// attachments asked for by a click: opened when the daemon has saved them
const wanted = new Set();
export async function openAttachment(a) {
  if (!a?.uid) return;
  wanted.add(a.uid);
  toast("Getting the file from the phone…");
  try {
    await api.attachmentFetch(app.deviceId, a.part, a.uid);
  } catch (e) {
    wanted.delete(a.uid);
    fail(e, "Couldn’t get that file from the phone.");
  }
}

// ── contacts ────────────────────────────────────────────────────────────────

export async function loadContacts() {
  const id = app.deviceId;
  try {
    const list = await api.contacts(id);
    if (id !== app.deviceId) return;
    app.contacts = list;
  } catch {
    if (id === app.deviceId) app.contacts = [];
  } finally {
    if (id === app.deviceId) app.contactsReady = true;
  }
}

export async function syncContacts() {
  try {
    await api.contactsSync(app.deviceId);
    toast("Asked the phone for its contacts");
  } catch (e) {
    fail(e, "Couldn’t reach the phone’s contacts.");
  }
}

const photoAsked = new Set();
/** fetch a contact's photo once (Avatar asks when it scrolls into view) */
export async function needPhoto(uid) {
  const id = app.deviceId;
  const key = `${id}/${uid}`;
  if (!uid || photoAsked.has(key)) return;
  photoAsked.add(key);
  try {
    const url = await api.contactPhoto(id, uid);
    if (id === app.deviceId) app.photos[uid] = url || "";
  } catch {
    photoAsked.delete(key); // asked again the next time the face is on screen
  }
}

// ── what is playing on the phone ────────────────────────────────────────────

export async function loadMedia() {
  const id = app.deviceId;
  try {
    const m = await api.media(id);
    if (id !== app.deviceId) return;
    app.media = m;
    app.mediaAt = Date.now();
  } catch {
    if (id === app.deviceId) app.media = null;
  }
}

let mediaTimer = null;
function mediaSoon() {
  clearTimeout(mediaTimer);
  mediaTimer = setTimeout(loadMedia, 120);
}

export async function mediaDo(action) {
  if (app.media && action === "PlayPause") {
    // the button answers at once; the phone's report follows
    app.media.position = mediaPosition();
    app.mediaAt = Date.now();
    app.media.playing = !app.media.playing;
  }
  try {
    await api.mediaAction(app.deviceId, action);
  } catch (e) {
    fail(e, "The phone’s player didn’t answer.");
    loadMedia();
  }
}

export async function mediaSet(what, value) {
  if (app.media) {
    if (what === "position") app.mediaAt = Date.now();
    app.media[what] = value;
  }
  try {
    await api.mediaSet(app.deviceId, what, value);
  } catch (e) {
    fail(e, "The phone’s player didn’t answer.");
    loadMedia();
  }
}

/** where the track is now: the last report plus the time since, while playing */
export function mediaPosition(now = Date.now()) {
  const m = app.media;
  if (!m) return 0;
  const p = m.playing ? m.position + (now - app.mediaAt) : m.position;
  return m.length > 0 ? Math.min(Math.max(0, p), m.length) : Math.max(0, p);
}

// ── sharing and the simple actions ──────────────────────────────────────────

export async function ring() {
  try {
    await api.act(app.deviceId, "ring");
    toast(`**${current?.name ?? "The phone"}** is ringing. Stop it on the phone.`, "success");
  } catch (e) {
    fail(e, "Couldn’t ring the phone.");
  }
}

export async function ping() {
  try {
    await api.act(app.deviceId, "ping");
    toast(`Ping sent to **${current?.name ?? "the phone"}**`, "success");
  } catch (e) {
    fail(e, "Couldn’t reach the phone.");
  }
}

export async function sendClipboard() {
  try {
    await api.act(app.deviceId, "clipboard");
    toast("Clipboard sent to the phone", "success");
  } catch (e) {
    fail(e, "Couldn’t send the clipboard.");
  }
}

const sendingWord = (n) => (n === 1 ? "Sending 1 file to the phone…" : `Sending ${n} files to the phone…`);

/** files dropped on the window (the backend sends only what was dropped) */
export async function sendFiles(paths) {
  const list = (paths || []).filter(Boolean);
  if (!list.length) return false;
  try {
    toast(sendingWord(await api.shareFiles(app.deviceId, list)), "success");
    return true;
  } catch (e) {
    fail(e, "Couldn’t send those files.");
    return false;
  }
}

/** the file picker, then send; the choice never passes through this page */
export async function chooseAndSendFiles() {
  try {
    const n = await api.sharePick(app.deviceId);
    if (n > 0) toast(sendingWord(n), "success");
    return n > 0;
  } catch (e) {
    fail(e, "Couldn’t send those files.");
    return false;
  }
}

export async function sendText(text) {
  try {
    const kind = await api.shareText(app.deviceId, text);
    toast(kind === "link" ? "Link opened on the phone" : "Text sent to the phone’s clipboard", "success");
    return true;
  } catch (e) {
    fail(e, "Couldn’t send that.");
    return false;
  }
}

export async function dial(number) {
  try {
    await api.dial(app.deviceId, number);
    toast(`**${view.name(number)}** is ready in the phone’s dialler`, "success");
  } catch (e) {
    fail(e, "Couldn’t open the phone’s dialler.");
  }
}

export async function copyText(text, what = "Copied") {
  try {
    await navigator.clipboard.writeText(String(text));
    toast(what, "success");
  } catch {
    toast("Couldn’t copy that.", "danger");
  }
}

// ── the phone's storage ─────────────────────────────────────────────────────

export async function loadStorage() {
  const id = app.deviceId;
  try {
    const s = await api.storageState(id);
    if (id === app.deviceId) app.storage = { ...app.storage, mounted: !!s.mounted, sshfs: !!s.sshfs };
  } catch {
    /* the page keeps what it had */
  }
}

export async function openStorage() {
  if (app.storage.busy) return;
  app.storage.busy = true;
  try {
    await api.storageOpen(app.deviceId);
    app.storage.mounted = true;
  } catch (e) {
    fail(e, "Couldn’t open the phone’s storage.");
  } finally {
    app.storage.busy = false;
  }
}

export async function closeStorage() {
  try {
    await api.storageClose(app.deviceId);
    app.storage.mounted = false;
    toast("The phone’s storage is disconnected");
  } catch (e) {
    fail(e, "Couldn’t disconnect the phone’s storage.");
  }
}

// ── typing on the phone ─────────────────────────────────────────────────────

export async function loadKeyboard() {
  const id = app.deviceId;
  try {
    const on = await api.keyboardState(id);
    if (id === app.deviceId) app.keyboard = !!on;
  } catch {
    if (id === app.deviceId) app.keyboard = false;
  }
}

// ── pairing ─────────────────────────────────────────────────────────────────

export async function pairing(device, action) {
  try {
    await api.pair(device, action);
  } catch (e) {
    fail(e, "The phone didn’t answer. Check that it is on the same network.");
  }
  devicesSoon();
}

export async function lookAgain() {
  try {
    if (!app.running) await api.daemonStart();
    await api.rescan();
  } catch {
    /* refreshDevices reports what is there */
  }
  await refreshDevices();
}

// ── what the daemon says by itself ──────────────────────────────────────────

let callTimer = null;

export function onEvent(ev) {
  if (!ev || !ev.kind) return;
  const mine = !ev.device || ev.device === app.deviceId;
  switch (ev.kind) {
    case "daemon":
      app.running = !!ev.running;
      devicesSoon();
      break;
    case "devices":
    case "self":
      devicesSoon();
      break;
    case "pairError": {
      const d = app.devices.find((x) => x.id === ev.device);
      toast(`Couldn’t pair with ${d?.name ?? "that device"}: ${ev.error || "it didn’t answer"}.`, "danger");
      devicesSoon();
      break;
    }
    case "battery": {
      const d = app.devices.find((x) => x.id === ev.device);
      if (d && ev.charge >= 0) d.battery = { charge: ev.charge, charging: !!ev.charging };
      break;
    }
    case "signal": {
      const d = app.devices.find((x) => x.id === ev.device);
      if (d) d.signal = ev.type ? { type: ev.type, strength: ev.strength } : null;
      break;
    }
    case "notifications":
      if (mine) notificationsSoon();
      break;
    case "message":
      if (mine) ingest(ev.msg);
      break;
    case "threadRemoved":
      if (mine) {
        app.threads = app.threads.filter((t) => t.thread !== ev.thread);
        delete app.messages[ev.thread];
        if (app.thread === ev.thread) app.thread = null;
      }
      break;
    case "threadLoaded":
      if (mine) app.loading[ev.thread] = false;
      break;
    case "attachment":
      if (mine && ev.path) {
        // the daemon names the file after the attachment's unique id
        const file = ev.path.split("/").pop();
        const hit = [...wanted].find((uid) => ev.name === uid || file === uid);
        if (hit || wanted.size === 1) {
          wanted.delete(hit ?? [...wanted][0]);
          api.openFile(ev.path).catch((e) => fail(e, "Couldn’t open that file."));
        }
      }
      break;
    case "contacts":
      if (mine) loadContacts();
      break;
    case "media":
      if (mine) mediaSoon();
      break;
    case "apps":
      if (mine) appsSoon();
      break;
    case "storage":
      if (mine) app.storage.mounted = !!ev.mounted;
      break;
    case "keyboard":
      if (mine) app.keyboard = !!ev.active;
      break;
    case "keyEcho":
      if (mine) app.keyAcks++;
      break;
    case "received":
      if (ev.path) {
        app.received.unshift({ path: ev.path, at: Date.now(), device: ev.device });
        if (app.received.length > 50) app.received.length = 50;
      }
      break;
    case "call":
      if (!mine) break;
      clearTimeout(callTimer);
      if (ev.event === "missedCall") {
        app.call = null;
        toast(`Missed call from **${ev.name || view.name(ev.number) || "an unknown number"}**`, "warning");
      } else if (ev.event === "ringing" || ev.event === "talking") {
        app.call = { event: ev.event, number: ev.number, name: ev.name };
        // the daemon never says when a call ends: the banner steps aside
        callTimer = setTimeout(() => (app.call = null), ev.event === "ringing" ? 45000 : 12000);
      }
      break;
  }
}
