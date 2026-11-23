// The Toast (design/system/components/Toast): one at a time. A new one
// replaces the one showing — the last thing that happened is the one to read.

export const toasts = $state({ current: null });

let seq = 0;

/**
 * tone: info | success | warning | danger. `action` = { label, run }.
 * Errors stay longer: there is something to read and maybe to do.
 */
export function toast(message, tone = "info", action = null) {
  const t = ["success", "warning", "danger"].includes(tone) ? tone : "info";
  const timeout = t === "danger" || t === "warning" || action ? 8000 : 4000;
  toasts.current = { id: ++seq, message: String(message), tone: t, action, timeout };
}

export function dismissToast(id = null) {
  if (toasts.current && (id == null || toasts.current.id === id)) toasts.current = null;
}

/** an error from the backend, as a sentence */
export function fail(e, fallback = "That didn’t work. Try again.") {
  const s = String(e?.message ?? e ?? "").trim();
  toast(s || fallback, "danger");
}
