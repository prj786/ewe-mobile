// The Apps page's plain functions — searching the list, the letter of a tile
// that has no icon, the package behind a notification — kept free of Svelte
// so `npm test` can hold them.

/** lower-case, accents folded, so "whats" finds "WhatsApp" and "éle" finds "Élève" */
const fold = (s) =>
  String(s ?? "")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();

/**
 * The apps that match `query` — by name (every word must appear), or by
 * package when the query looks like one (has a dot). An empty query is the
 * whole list, as given.
 */
export function filterApps(list, query) {
  const q = fold(query).trim();
  if (!q) return list;
  const words = q.split(/\s+/).filter(Boolean);
  return list.filter((a) => {
    const name = fold(a.name);
    const pkg = fold(a.package);
    if (q.includes(".") && pkg.includes(q)) return true;
    return words.every((w) => name.includes(w) || pkg.split(".").some((part) => part.startsWith(w)));
  });
}

/** The one letter a tile shows while an app has no icon (the scrcpy list). */
export function letterOf(name) {
  const s = String(name ?? "").trim();
  if (!s) return "?";
  const first = [...s][0];
  if (!/\p{L}|\p{N}/u.test(first)) return "#";
  // Georgian has capitals (Mtavruli) nobody writes with: keep the letter
  return /\p{Script=Georgian}/u.test(first) ? first : first.toUpperCase();
}

/**
 * The package in a notification's `internalId` (`0|com.whatsapp|…`, the
 * second `|` field), or "". The backend sends it as `package` already; this
 * is the same rule for mock data and old backends.
 */
export function packageOf(internalId) {
  const p = String(internalId ?? "").split("|")[1] ?? "";
  return /^[A-Za-z0-9._]{1,255}$/.test(p) && !p.startsWith(".") && !p.includes("..") ? p : "";
}

/** "5 minutes ago"-style words for when the list was read */
export function readWhen(updated, now = Date.now()) {
  if (!updated) return "";
  const d = Math.max(0, now - updated);
  if (d < 60000) return "just now";
  if (d < 3600000) return `${Math.round(d / 60000)} min ago`;
  if (d < 86400000) return `${Math.round(d / 3600000)} h ago`;
  return `${Math.round(d / 86400000)} days ago`;
}
