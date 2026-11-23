import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

// Outside Tauri (a plain browser with `?mock=1`) src/lib/devmock.js answers.
const mock = () => window.__EWE_MOBILE_MOCK__;
const invoke = (cmd, args) => (mock() ? mock().invoke(cmd, args) : tauriInvoke(cmd, args));

/** subscribe to a backend event; resolves to the unsubscribe function */
export const listen = (event, cb) => (mock() ? mock().listen(event, cb) : tauriListen(event, cb));
export const inTauri = () => !!window.__TAURI_INTERNALS__;

// the look
export const dePrefs = () => invoke("de_prefs");
export const themeTokens = () => invoke("theme_tokens");

// the app
export const takePage = () => invoke("take_page");

// the daemon and its devices
export const status = () => invoke("status");
export const daemonStart = () => invoke("daemon_start");
export const devices = () => invoke("devices");
export const rescan = () => invoke("rescan");
export const setName = (name) => invoke("set_name", { name });
/** action: request | accept | cancel | unpair */
export const pair = (device, action) => invoke("pair", { device, action });
export const plugins = (device) => invoke("plugins", { device });
export const setPlugin = (device, plugin, enabled) => invoke("set_plugin", { device, plugin, enabled });

// notifications
export const notifications = (device) => invoke("notifications", { device });
export const notificationDismiss = (device, id) => invoke("notification_dismiss", { device, id });
export const notificationReply = (device, replyId, text) => invoke("notification_reply", { device, replyId, text });

// messages
export const conversations = (device) => invoke("conversations", { device });
export const conversationLoad = (device, thread, start, end) =>
  invoke("conversation_load", { device, thread, start, end });
export const sendMessage = (device, thread, addresses, text) =>
  invoke("send_message", { device, thread, addresses, text });
export const attachmentFetch = (device, part, uid) => invoke("attachment_fetch", { device, part, uid });

// contacts
export const contacts = (device) => invoke("contacts", { device });
export const contactPhoto = (device, uid) => invoke("contact_photo", { device, uid });
export const contactsSync = (device) => invoke("contacts_sync", { device });

// what is playing on the phone
export const media = (device) => invoke("media", { device });
/** action: Play | Pause | PlayPause | Next | Previous | Stop */
export const mediaAction = (device, action) => invoke("media_action", { device, action });
/** what: volume | position | player */
export const mediaSet = (device, what, value) => invoke("media_set", { device, what, value });

// sharing
/** send files that were dropped on the window (the backend checks they were) */
export const shareFiles = (device, paths) => invoke("share_files", { device, paths });
/** ewe's file picker, then send; resolves to how many went (0 = cancelled) */
export const sharePick = (device) => invoke("share_pick", { device });
export const shareText = (device, text) => invoke("share_text", { device, text });
export const dial = (device, number) => invoke("dial", { device, number });
/** action: ring | ping | clipboard */
export const act = (device, action) => invoke("act", { device, action });

// the phone's storage
export const storageState = (device) => invoke("storage_state", { device });
export const storageOpen = (device) => invoke("storage_open", { device });
export const storageClose = (device) => invoke("storage_close", { device });

// typing and pointing on the phone
export const keySend = (device, key, mods = {}) =>
  invoke("key_send", { device, key, shift: !!mods.shift, ctrl: !!mods.ctrl, alt: !!mods.alt });
export const keyboardState = (device) => invoke("keyboard_state", { device });
export const pointerMove = (device, dx, dy) => invoke("pointer_move", { device, dx, dy });
export const pointerDo = (device, action, dx = null, dy = null) => invoke("pointer_do", { device, action, dx, dy });

// this computer
export const openFile = (path, reveal = false) => invoke("open_file", { path, reveal });
export const openLink = (url) => invoke("open_link", { url });

/** files dropped on the window: cb({ type: "enter" | "leave" | "drop", paths }) */
export async function onFileDrop(cb) {
  if (mock() || !inTauri()) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((e) => cb({ type: e.payload.type, paths: e.payload.paths || [] }));
}
