// Browser-only stand-in for the Tauri backend, so the UI can be opened (and
// screenshotted) with `npm run dev`: http://localhost:5177/?mock=1#messages
// Never loaded inside Tauri. Everyone in here is made up.
//
//   scene=welcome     no phone linked yet, one nearby
//   scene=pairing     … and a pairing request waiting on the phone
//   scene=asks        … the phone asks to pair
//   scene=away        the phone is linked but out of reach
//   scene=empty       linked and in reach, with nothing on it
//   scene=nodaemon    KDE Connect is not running
//   scene=two         two linked phones
//   scheme=ewe-dark|ewe-light|a11y   the theme_tokens reply to serve — real
//                     `ewe-theme show` output in .dev-mock/ (git-ignored);
//                     without it tokens.css stands
//   thread=<id>       open that conversation (1 … 6)
//   call=1            an incoming call
//   keyboard=1        the phone takes the pointer and keys (Remote is set up)
//   deaf=1            … but no text field is open on it: keys get no answer
//   nosshfs=1         sshfs is missing on this computer
//   adb=none|tools|seen|pairing|paired|connected
//                     the debugging link: no tools / tools but the phone's
//                     Wireless debugging off / on and found / its pairing
//                     dialog open / paired, not connected / connected
//                     (default: connected)
//   stock=1           the phone runs the stock KDE Connect app: no apps list
//   mirror=1          the mirrored screen is up
//   still             no transitions or animations (screenshots)
//   slow=<command>    that command never answers (loading states)
//   trace=1           every command the page sends, on the console

const MIN = 60000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

const svg = (body, w = 96, h = 96) =>
  "data:image/svg+xml;utf8," +
  encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}">${body}</svg>`);
const face = (a, b, letter) =>
  svg(
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs>` +
      `<rect width="96" height="96" fill="url(#g)"/><text x="48" y="62" font-family="sans-serif" font-size="42" font-weight="600" fill="#fff" text-anchor="middle">${letter}</text>`
  );
const appIcon = (color, letter) =>
  svg(
    `<rect width="96" height="96" rx="22" fill="${color}"/><text x="48" y="64" font-family="sans-serif" font-size="46" font-weight="700" fill="#fff" text-anchor="middle">${letter}</text>`
  );
const picture = svg(
  `<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f6c36a"/><stop offset=".6" stop-color="#e98a5b"/><stop offset="1" stop-color="#4b3a5c"/></linearGradient></defs>` +
    `<rect width="320" height="220" fill="url(#s)"/><circle cx="230" cy="92" r="34" fill="#fff3d6"/><path d="M0 170 L70 120 L130 160 L200 110 L320 180 V220 H0Z" fill="#2c2438"/>`,
  320,
  220
);
const cover = svg(
  `<defs><linearGradient id="c" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1f4b5c"/><stop offset="1" stop-color="#e6ab09"/></linearGradient></defs>` +
    `<rect width="300" height="300" fill="url(#c)"/><circle cx="150" cy="150" r="78" fill="none" stroke="#fff" stroke-opacity=".55" stroke-width="3"/><circle cx="150" cy="150" r="18" fill="#fff" fill-opacity=".8"/>`,
  300,
  300
);

const PEOPLE = [
  { uid: "c1", name: "Nino Abashidze", phones: [{ number: "+1 202 555 0101", kind: "mobile" }, { number: "202 555 0111", kind: "home" }], emails: ["nino@example.org"], org: "", note: "", hasPhoto: true, photo: face("#8fbce0", "#3d6d99", "N") },
  { uid: "c2", name: "Giorgi Lomidze", phones: [{ number: "+1 202 555 0102", kind: "mobile" }], emails: [], org: "Lomidze Studio", note: "Prefers messages after six.", hasPhoto: false },
  { uid: "c3", name: "მარიამ კაპანაძე", phones: [{ number: "+1 202 555 0103", kind: "mobile" }], emails: ["mariam@example.org"], org: "", note: "", hasPhoto: true, photo: face("#e0a48f", "#99503d", "მ") },
  { uid: "c4", name: "Ana Tsereteli", phones: [{ number: "+1 202 555 0104", kind: "work" }], emails: [], org: "", note: "", hasPhoto: false },
  { uid: "c5", name: "Dato Gelashvili", phones: [{ number: "+1 202 555 0105", kind: "mobile" }], emails: [], org: "", note: "", hasPhoto: true, photo: face("#9bd3a4", "#3d8a55", "D") },
  { uid: "c6", name: "Elene Beridze", phones: [{ number: "+1 202 555 0106", kind: "mobile" }], emails: ["elene@example.org"], org: "", note: "", hasPhoto: false },
  { uid: "c7", name: "Luka Meskhi", phones: [{ number: "+1 202 555 0107", kind: "mobile" }], emails: [], org: "", note: "", hasPhoto: false },
  { uid: "c8", name: "Salome Kiknadze", phones: [{ number: "+1 202 555 0108", kind: "mobile" }], emails: [], org: "", note: "", hasPhoto: false },
  { uid: "c9", name: "Tamar Chkheidze", phones: [{ number: "+1 202 555 0109", kind: "mobile" }], emails: [], org: "", note: "", hasPhoto: false },
  { uid: "c10", name: "Vakho Janelidze", phones: [{ number: "+1 202 555 0110", kind: "mobile" }], emails: [], org: "Plumber", note: "", hasPhoto: false }
].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));

export function install(params) {
  const scene = params.get("scene") || "";
  const now = Date.now();
  let uid = 1000;
  const msg = (thread, addresses, type, minutesAgo, body, extra = {}) => ({
    event: 1, body, addresses, date: now - minutesAgo * MIN, type, read: true, thread, uid: ++uid, sub: 1, attachments: [], ...extra
  });

  const A = "+12025550101"; // Nino, as the phone writes numbers in messages
  const G = "+12025550102";
  const M = "+12025550103";
  const history = {
    1: [
      msg(1, [A], 1, 3 * 24 * 60 + 200, "Are you coming to the lake on Saturday?"),
      msg(1, [A], 2, 3 * 24 * 60 + 190, "Yes. I can bring the tent and the small stove."),
      msg(1, [A], 1, 3 * 24 * 60 + 185, "Perfect. Dato has the boat."),
      msg(1, [A], 1, 26 * 60, "Look at this from yesterday", { attachments: [{ part: 7, mime: "image/jpeg", thumb: picture, uid: "PART_7" }] }),
      msg(1, [A], 2, 26 * 60 - 4, "That light. Send me the full one later?"),
      msg(1, [A], 1, 25 * 60, "Uploading here: https://example.org/lake/album."),
      msg(1, [A], 2, 95, "Leaving in ten minutes"),
      msg(1, [A], 2, 94, "Do you need anything from the shop?"),
      msg(1, [A], 1, 90, "Bread, and the good cheese if they have it"),
      msg(1, [A], 1, 12, "Also — the gate code is 4812, the bell is broken.", { read: false })
    ],
    2: [
      msg(2, [G], 1, 2 * 24 * 60, "Invoice is in your mail."),
      msg(2, [G], 2, 2 * 24 * 60 - 30, "Got it, paying today."),
      msg(2, [G], 2, 5 * 60, "The drawings look right. Go ahead with the second version.")
    ],
    3: [msg(3, ["MyBank"], 1, 7 * 60, "Payment of 24.50 GEL at CAFE LITERA. Balance: 1,204.18 GEL.")],
    4: [
      msg(4, [M, A], 1, 30 * 60, "დილა მშვიდობისა! ხვალ შვიდზე ვხვდებით?", { event: 3 }),
      msg(4, [A, M], 1, 29 * 60, "Seven works for me.", { event: 3 }),
      msg(4, [M, A], 2, 28 * 60, "See you both there.", { event: 3 })
    ],
    5: [msg(5, ["+12025550147"], 1, 4 * 24 * 60, "Your parcel is at the pick-up point until Friday.")],
    6: [msg(6, ["2020"], 1, 9 * 24 * 60, "Your code is 418 220. Don’t share it with anyone.")]
  };

  const phone = {
    id: "mock_pixel",
    name: "Pixel 9",
    type: "phone",
    reachable: true,
    paired: true,
    pairState: 3,
    pairRequested: false,
    pairRequestedByPeer: false,
    verificationKey: "B2F0AC1E",
    supported: ["sms", "contacts", "notifications", "telephony", "battery", "connectivity_report", "share", "clipboard", "sftp", "mprisremote", "findmyphone", "ping", "remotecontrol", "remotekeyboard", "mpriscontrol", "pausemusic", "systemvolume", "mousepad", "presenter", "runcommand", "sendnotifications", "findthisdevice"],
    loaded: [],
    battery: { charge: 64, charging: false },
    signal: { type: "LTE", strength: 3 }
  };
  phone.loaded = [...phone.supported];
  const tablet = { ...phone, id: "mock_tab", name: "Galaxy Tab S9", type: "tablet", battery: { charge: 91, charging: true }, signal: null };
  tablet.supported = phone.supported.filter((p) => !["sms", "telephony", "connectivity_report"].includes(p));
  tablet.loaded = [...tablet.supported];

  let devices = [phone];
  let running = true;
  let conversations = Object.values(history).map((list) => list[list.length - 1]);
  let people = PEOPLE;
  let notifications = [
    { id: "9", app: "Signal", title: "Elene Beridze", text: "Did you see the schedule for next week? I moved Tuesday.", ticker: "", icon: appIcon("#3a76f0", "S"), dismissable: true, replyId: "r9", silent: false, conversation: true },
    { id: "8", app: "Signal", title: "Family", text: "Dato: bringing khachapuri", ticker: "", icon: appIcon("#3a76f0", "S"), dismissable: true, replyId: "r8", silent: false, conversation: true },
    { id: "7", app: "Calendar", title: "Dentist", text: "Tomorrow, 10:30 – 11:00", ticker: "", icon: appIcon("#1a9b6c", "31"), dismissable: true, replyId: "", silent: false, conversation: false },
    { id: "6", app: "Mail", title: "Giorgi Lomidze", text: "Invoice 0412 — attached is the invoice for October. Let me know if anything is unclear.", ticker: "", icon: appIcon("#d8523e", "M"), dismissable: true, replyId: "", silent: false, conversation: false },
    { id: "3", app: "Weather", title: "18° · Partly cloudy", text: "Tbilisi", ticker: "", icon: appIcon("#5a94c8", "W"), dismissable: false, replyId: "", silent: true, conversation: false },
    { id: "2", app: "System", title: "Charging slowly", text: "Use the charger that came with the phone.", ticker: "", icon: "", dismissable: false, replyId: "", silent: true, conversation: false }
  ];
  let media = { player: "Spotify", players: ["Spotify", "YouTube"], title: "Suliko", artist: "Trio Mandili", album: "With Love", playing: true, canSeek: true, length: 201000, position: 64000, volume: 56, art: cover };
  let storage = { mounted: false, sshfs: !params.has("nosshfs") };

  // the phone's apps (C1 → C2) and the debugging link
  const APPS = [
    ["com.whatsapp", "WhatsApp", "#25d366", "W"], ["org.telegram.messenger", "Telegram", "#2aabee", "T"],
    ["com.instagram.android", "Instagram", "#e1306c", "I"], ["com.google.android.youtube", "YouTube", "#ff0000", "▶"],
    ["com.spotify.music", "Spotify", "#1db954", "S"], ["com.google.android.apps.maps", "Maps", "#34a853", "M"],
    ["com.google.android.gm", "Gmail", "#ea4335", "G"], ["com.google.android.calendar", "Calendar", "#1a73e8", "31"],
    ["com.alderbank.mobile", "Alder Bank", "#ff5b00", "A"], ["com.hillbank.mobile", "Hill Bank", "#00a3e0", "H"],
    ["com.google.android.apps.photos", "Photos", "#fbbc04", "P"], ["com.android.chrome", "Chrome", "#4285f4", "C"],
    ["org.mozilla.firefox", "Firefox", "#ff7139", "F"], ["com.google.android.keep", "Keep", "#f9ab00", "K"],
    ["com.google.android.calculator", "Calculator", "#5f6368", "="], ["com.android.camera2", "Camera", "#202124", "◉"],
    ["com.google.android.apps.messaging", "Messages", "#1a73e8", "M"], ["com.google.android.dialer", "Phone", "#1e8e3e", "☏"],
    ["com.google.android.deskclock", "Clock", "#4285f4", "◴"], ["com.android.vending", "Play Store", "#01875f", "▷"],
    ["org.signal", "Signal", "#3a76f0", "S"], ["com.netflix.mediaclient", "Netflix", "#e50914", "N"],
    ["com.duolingo", "Duolingo", "#58cc02", "D"], ["com.google.android.apps.docs", "Drive", "#fbbc04", "△"]
  ].map(([pkg, label, color, letter]) => ({ package: pkg, name: label, sys: false, icon: appIcon(color, letter) }));
  for (const [pkg, label] of [["com.android.settings", "Settings"], ["com.android.documentsui", "Files"], ["com.google.android.apps.wellbeing", "Digital Wellbeing"]]) {
    APPS.push({ package: pkg, name: label, sys: true, icon: appIcon("#5f6368", label[0]) });
  }
  APPS.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  // the notifications name their packages (0|<package>|…), as the phone does
  const PKG_OF = { Signal: "org.signal", Calendar: "com.google.android.calendar", Mail: "com.google.android.gm", Weather: "", System: "" };
  for (const n of notifications) n.package = PKG_OF[n.app] ?? "";
  const stock = params.has("stock");
  const adbMode = params.get("adb") || "connected";
  const windows = params.has("mirror") ? [{ class: "ewe-mobile-screen", title: phone.name, package: null }] : [];
  let paired = ["paired", "connected"].includes(adbMode);
  let connected = adbMode === "connected";
  let launcherApps = true;
  const adbStatus = () => {
    const tools = adbMode === "none" ? { adb: false, scrcpy: false } : { adb: true, scrcpy: true };
    const services = [];
    if (["seen", "pairing", "paired", "connected"].includes(adbMode)) services.push({ name: "adb-0A1B2C3D4E5F6G-AbCdEf", kind: "connect", host: "192.168.1.5", port: 39855 });
    if (adbMode === "pairing" && !paired) services.push({ name: "adb-0A1B2C3D4E5F6G-AbCdEf", kind: "pairing", host: "192.168.1.5", port: 41021 });
    return { ok: true, tools, paired, connected, serial: connected ? "adb-0A1B2C3D4E5F6G-AbCdEf._adb-tls-connect._tcp" : "", model: connected ? "Pixel 9" : "", services, devices: [] };
  };
  let selfName = "ewe-laptop";
  const enabled = new Map();

  if (scene === "welcome" || scene === "pairing" || scene === "asks") {
    Object.assign(phone, { paired: false, pairState: scene === "pairing" ? 1 : scene === "asks" ? 2 : 0, loaded: [], battery: null, signal: null });
    phone.pairRequested = scene === "pairing";
    phone.pairRequestedByPeer = scene === "asks";
  } else if (scene === "away") {
    Object.assign(phone, { reachable: false, loaded: [], battery: null, signal: null });
  } else if (scene === "empty") {
    conversations = [];
    notifications = [];
    people = [];
    media = { player: "", players: [], title: "", artist: "", album: "", playing: false, canSeek: false, length: 0, position: 0, volume: 40, art: "" };
  } else if (scene === "nodaemon") {
    running = false;
  } else if (scene === "two") {
    devices = [phone, tablet];
  }

  const listeners = new Map();
  const emit = (payload) => (listeners.get("kdc") || []).forEach((cb) => cb({ payload }));
  const later = (ms, fn) => setTimeout(fn, params.has("still") ? 0 : ms);
  const slow = params.get("slow");
  const trace = params.has("trace");

  async function scheme() {
    const name = params.get("scheme");
    if (!name) throw new Error("no scheme asked for");
    const r = await fetch(`/.dev-mock/theme-${name}.json`);
    if (!r.ok) throw new Error("no such scheme sample");
    return r.json();
  }

  const handlers = {
    de_prefs: () => ({ scheme: params.get("scheme") || "" }),
    theme_tokens: scheme,
    take_page: () => null,
    status: () => ({ installed: true, running, name: selfName, id: "mock_self" }),
    daemon_start: () => {
      later(600, () => {
        running = true;
        emit({ kind: "daemon", running: true });
      });
      return true;
    },
    devices: () => JSON.parse(JSON.stringify(devices)),
    rescan: () => null,
    set_name: ({ name }) => {
      selfName = name;
      return null;
    },
    pair: ({ device, action }) => {
      const d = devices.find((x) => x.id === device);
      if (!d) throw new Error("That device is gone.");
      if (action === "request") {
        Object.assign(d, { pairState: 1, pairRequested: true });
        later(1800, () => {
          Object.assign(d, { paired: true, pairState: 3, pairRequested: false, loaded: [...d.supported], battery: { charge: 64, charging: false }, signal: { type: "LTE", strength: 3 } });
          emit({ kind: "devices" });
        });
      } else if (action === "accept") {
        Object.assign(d, { paired: true, pairState: 3, pairRequestedByPeer: false, loaded: [...d.supported], battery: { charge: 64, charging: false }, signal: { type: "LTE", strength: 3 } });
      } else if (action === "cancel") {
        Object.assign(d, { pairState: 0, pairRequested: false, pairRequestedByPeer: false });
      } else if (action === "unpair") {
        Object.assign(d, { paired: false, pairState: 0, loaded: [], battery: null, signal: null });
      }
      emit({ kind: "devices" });
      return null;
    },
    plugins: ({ device }) => {
      const d = devices.find((x) => x.id === device);
      return (d?.supported || []).map((id) => ({ id, enabled: enabled.get(`${device}/${id}`) ?? true }));
    },
    set_plugin: ({ device, plugin, enabled: on }) => {
      enabled.set(`${device}/${plugin}`, on);
      const d = devices.find((x) => x.id === device);
      if (d) d.loaded = d.supported.filter((id) => enabled.get(`${device}/${id}`) ?? true);
      emit({ kind: "devices" });
      return null;
    },
    notifications: () => notifications,
    notification_dismiss: ({ id }) => {
      notifications = notifications.filter((n) => n.id !== id);
      later(200, () => emit({ kind: "notifications", device: phone.id }));
      return null;
    },
    notification_reply: () => null,
    conversations: () => conversations,
    conversation_load: ({ thread, start, end }) => {
      const list = [...(history[thread] || [])].reverse().slice(start, end);
      later(350, () => {
        for (const m of list) emit({ kind: "message", device: phone.id, msg: m });
        emit({ kind: "threadLoaded", device: phone.id, thread, count: list.length });
      });
      return null;
    },
    send_message: ({ thread, addresses, text }) => {
      if (/fail/i.test(text)) throw new Error("The phone didn’t answer.");
      let t = thread;
      if (t == null) {
        t = Math.max(0, ...Object.keys(history).map(Number)) + 1;
        history[t] = [];
      }
      const m = msg(t, addresses.length ? addresses : history[t][0]?.addresses || [], 2, 0, text);
      history[t].push(m);
      later(900, () => emit({ kind: "message", device: phone.id, msg: m }));
      if (t === 1)
        later(3500, () => {
          const r = msg(1, [A], 1, 0, "Got it. See you soon", { read: false });
          history[1].push(r);
          emit({ kind: "message", device: phone.id, msg: r });
        });
      return null;
    },
    attachment_fetch: () => null,
    contacts: () => people.map(({ photo, ...c }) => c),
    contact_photo: ({ uid: id }) => people.find((c) => c.uid === id)?.photo || "",
    contacts_sync: () => {
      later(600, () => emit({ kind: "contacts", device: phone.id }));
      return null;
    },
    media: () => ({ ...media }),
    media_action: ({ action }) => {
      if (action === "PlayPause") media.playing = !media.playing;
      if (action === "Next") Object.assign(media, { title: "Tsintskaro", artist: "Hamlet Gonashvili", album: "Georgian Voices", position: 0, length: 187000 });
      if (action === "Previous") media.position = 0;
      later(150, () => emit({ kind: "media", device: phone.id }));
      return null;
    },
    media_set: ({ what, value }) => {
      media[what] = value;
      if (what === "player") Object.assign(media, value === "YouTube" ? { title: "How sheep are sheared", artist: "Wool Works", album: "", art: "" } : { title: "Suliko", artist: "Trio Mandili", album: "With Love", art: cover });
      later(150, () => emit({ kind: "media", device: phone.id }));
      return null;
    },
    share_files: ({ paths }) => paths.length,
    share_pick: () => 2,
    share_text: ({ text }) => (/^https?:\/\/\S+$/.test(text.trim()) ? "link" : "text"),
    dial: () => null,
    act: () => null,
    storage_state: () => ({ ...storage }),
    storage_open: () =>
      new Promise((resolve) =>
        later(1200, () => {
          storage.mounted = true;
          resolve("/run/user/1000/mock_pixel/storage/emulated/0");
        })
      ),
    storage_close: () => {
      storage.mounted = false;
      return null;
    },
    // a phone that types what it is sent says so; `deaf=1` is one with no
    // text field open — it takes the key and answers nothing
    key_send: () => {
      if (!params.has("deaf")) later(60, () => emit({ kind: "keyEcho", device: phone.id }));
      return null;
    },
    text_send: () => {
      if (!params.has("deaf")) later(60, () => emit({ kind: "keyEcho", device: phone.id }));
      return null;
    },
    keyboard_state: () => params.has("keyboard"),
    pointer_move: () => null,
    pointer_do: () => null,
    // the phone's apps and the debugging link
    apps: () => {
      if (stock) return { source: "none", updated: 0, apps: [] };
      if (!phone.reachable) return { source: "cache", updated: now - 3 * HOUR, apps: APPS };
      return { source: "phone", updated: now, apps: APPS };
    },
    apps_refresh: () => {
      later(700, () => emit({ kind: "apps", device: phone.id }));
      return null;
    },
    apps_scrcpy: () =>
      new Promise((resolve) =>
        later(1500, () => resolve({ updated: now, apps: APPS.map(({ icon, ...a }) => ({ ...a, icon: "" })) }))
      ),
    app_open: ({ package: pkg, name, where }) => {
      if (!connected && where === "here") throw new Error("The phone isn’t connected for debugging.");
      if (where === "phone" || (where === "auto" && !connected)) {
        if (stock) throw new Error(`${phone.name} runs the stock KDE Connect app, which can’t open apps. Install ewe Mobile on it.`);
        return "phone";
      }
      later(800, () => windows.push({ class: `ewe-mobile-app-${pkg}`, title: name, package: pkg }));
      return "here";
    },
    launcher_setting: () => launcherApps,
    set_launcher_setting: ({ on }) => {
      launcherApps = on;
      return null;
    },
    adb_status: adbStatus,
    adb_services: () => adbStatus().services,
    adb_pair: ({ code }) => {
      if (code !== "123456") throw new Error("The phone refused the code. Read the six digits again — each attempt gets a new code.");
      paired = true;
      later(1500, () => (connected = true));
      return "adb-0A1B2C3D4E5F6G-AbCdEf";
    },
    adb_connect: () => {
      connected = true;
      return null;
    },
    tools_offer: () => true,
    screen_start: () => {
      later(500, () => windows.push({ class: "ewe-mobile-screen", title: phone.name, package: null }));
      return null;
    },
    window_stop: ({ class: cls }) => {
      const i = windows.findIndex((w) => w.class === cls);
      if (i >= 0) windows.splice(i, 1);
      return i >= 0;
    },
    windows_state: () => ({ windows: [...windows], died: [] }),
    open_file: () => null,
    open_link: ({ url }) => {
      console.info("open", url);
      return null;
    }
  };

  window.__EWE_MOBILE_MOCK__ = {
    invoke: async (cmd, args = {}) => {
      if (trace) console.info("mock", cmd, JSON.stringify(args));
      if (slow && slow === cmd) return new Promise(() => {});
      const h = handlers[cmd];
      if (!h) throw new Error(`mock: no such command ${cmd}`);
      return h(args);
    },
    listen: async (event, cb) => {
      const list = listeners.get(event) || [];
      list.push(cb);
      listeners.set(event, list);
      return () => listeners.set(event, (listeners.get(event) || []).filter((x) => x !== cb));
    },
    emit
  };

  if (params.has("still")) {
    const s = document.createElement("style");
    s.textContent = "*,*::before,*::after{animation:none!important;transition:none!important}";
    document.head.appendChild(s);
  }
  // the page's own state, for a script driving a screenshot (a module
  // imported from outside the page's graph would be a second copy of it)
  const state = import("./state.svelte.js").then((st) => (window.__EWE_MOBILE_MOCK__.state = st));

  // scripted moments, once the page has read the phone
  const thread = Number(params.get("thread") || 0);
  if (thread) {
    later(400, async () => {
      const st = await state;
      st.app.route = "messages";
      st.openThread(thread);
    });
  }
  if (params.has("call")) {
    later(600, () => emit({ kind: "call", device: phone.id, event: "ringing", number: "+1 202 555 0101", name: "" }));
  }
  if (!["welcome", "pairing", "asks", "nodaemon", "empty"].includes(scene)) {
    later(500, () =>
      emit({ kind: "received", device: phone.id, url: "file:///home/you/Downloads/IMG_2041.jpg", path: "/home/you/Downloads/IMG_2041.jpg" })
    );
  }
}
