// Pure helpers: phone numbers, times, message text. No DOM, no state —
// tests/format.test.js runs them in node.

export const digits = (s) => String(s ?? "").replace(/\D+/g, "");

/**
 * What two spellings of one number share: "+1 202 555 0101" and
 * "(202) 555-0101" are the same phone. The last 9 digits decide once there are
 * at least 7; a short code is all of its digits; a sender written in letters
 * ("MyBank") is itself.
 */
export function numberKey(address) {
  const raw = String(address ?? "").trim();
  if (/[^\d\s+().#*-]/.test(raw)) return raw.toLowerCase();
  const d = digits(raw);
  if (!d) return raw.toLowerCase();
  return d.length >= 7 ? d.slice(-9) : d;
}

export const sameNumber = (a, b) => {
  const ka = numberKey(a);
  return !!ka && ka === numberKey(b);
};

/** does this look like something that can be dialled? */
export const isNumber = (s) => /\d/.test(String(s ?? "")) && !/[^\d\s+().#*-]/.test(String(s ?? "").trim());

/**
 * The letters an avatar shows: the first letter of the first two words.
 * Never uppercased by force — Georgian has no capitals (uppercasing turns
 * Mkhedruli into Mtavruli). A number has no initials: the avatar shows a
 * person instead.
 */
export function initials(name) {
  const n = String(name ?? "").trim();
  if (!n || /^[+\d(]/.test(n)) return "";
  const words = n.split(/\s+/).filter((w) => /\p{L}/u.test(w));
  const first = (w) => {
    const ch = Array.from(w).find((c) => /\p{L}/u.test(c)) || "";
    return /[ა-ჿᲐ-Ჿ]/.test(ch) ? ch : ch.toLocaleUpperCase();
  };
  return words.slice(0, 2).map(first).join("");
}

const sameDay = (a, b) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
const daysApart = (a, b) =>
  Math.round(
    (new Date(b.getFullYear(), b.getMonth(), b.getDate()) - new Date(a.getFullYear(), a.getMonth(), a.getDate())) /
      86400000
  );

/** the time of day, by the person's locale (12 or 24 hours) */
export function clock(ts, locale = undefined) {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(new Date(ts));
}

/** a short "when" for a list row: now · 5 min · 14:20 · Yesterday · Mon · 17 Nov · 17 Nov 2025 */
export function when(ts, now = Date.now(), locale = undefined) {
  if (!ts) return "";
  const d = new Date(ts);
  const n = new Date(now);
  const mins = Math.floor((now - ts) / 60000);
  if (mins < 1 && mins > -5) return "now";
  if (mins >= 1 && mins < 60) return `${mins} min`;
  if (sameDay(d, n)) return clock(ts, locale);
  const days = daysApart(d, n);
  if (days === 1) return "Yesterday";
  if (days > 1 && days < 7) return new Intl.DateTimeFormat(locale, { weekday: "short" }).format(d);
  const opts = { day: "numeric", month: "short" };
  if (d.getFullYear() !== n.getFullYear()) opts.year = "numeric";
  return new Intl.DateTimeFormat(locale, opts).format(d);
}

/** the heading over a day of messages: Today · Yesterday · Monday, 17 November */
export function dayLabel(ts, now = Date.now(), locale = undefined) {
  const d = new Date(ts);
  const n = new Date(now);
  if (sameDay(d, n)) return "Today";
  if (daysApart(d, n) === 1) return "Yesterday";
  const opts = { weekday: "long", day: "numeric", month: "long" };
  if (d.getFullYear() !== n.getFullYear()) opts.year = "numeric";
  return new Intl.DateTimeFormat(locale, opts).format(d);
}

export const dayKey = (ts) => {
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
};

/** 4413541 ms → "1:13:33"; 201000 → "3:21" */
export function duration(ms) {
  const total = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (x) => String(x).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

// GSM 03.38: the letters one SMS carries at 7 bits each; these cost two
const GSM = "@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà";
const GSM_EXT = "^{}\\[~]|€";

/**
 * How many SMS a text becomes. Plain Latin fits 160 characters (153 per part
 * once it is split); anything else — Georgian, emoji — makes the whole text
 * 16-bit: 70, then 67 per part.
 */
export function smsParts(text) {
  const t = String(text ?? "");
  if (!t) return { parts: 0, length: 0, unicode: false };
  let septets = 0;
  let unicode = false;
  for (const ch of t) {
    if (GSM.includes(ch)) septets += 1;
    else if (GSM_EXT.includes(ch)) septets += 2;
    else {
      unicode = true;
      break;
    }
  }
  if (unicode) {
    const units = t.length; // UTF-16 code units, what UCS-2 counts
    return { parts: units <= 70 ? 1 : Math.ceil(units / 67), length: units, unicode: true };
  }
  return { parts: septets <= 160 ? 1 : Math.ceil(septets / 153), length: septets, unicode: false };
}

/** text → [{ text }, { text, url }]: web links become links, nothing else does */
export function linkify(text) {
  const out = [];
  const re = /https?:\/\/[^\s<>"']+/g;
  const s = String(text ?? "");
  let last = 0;
  let m;
  while ((m = re.exec(s))) {
    let url = m[0];
    // a sentence's full stop or closing bracket is not part of the link
    const trail = url.match(/[.,;:!?)\]}]+$/);
    if (trail) url = url.slice(0, -trail[0].length);
    if (!url) continue;
    if (m.index > last) out.push({ text: s.slice(last, m.index) });
    out.push({ text: url, url });
    last = m.index + url.length;
    re.lastIndex = last;
  }
  if (last < s.length) out.push({ text: s.slice(last) });
  return out;
}

/** one line of a message for a list row */
export function snippet(msg) {
  const body = String(msg?.body ?? "").replace(/\s+/g, " ").trim();
  if (body) return body;
  const a = msg?.attachments?.[0];
  if (!a) return "";
  if (a.mime?.startsWith("image/")) return "Photo";
  if (a.mime?.startsWith("video/")) return "Video";
  if (a.mime?.startsWith("audio/")) return "Voice message";
  return "Attachment";
}

/** "mobile" → "Mobile" for the label beside a number */
export const kindWord = (kind) => ({ mobile: "Mobile", home: "Home", work: "Work" })[kind] || "Phone";

/** which battery glyph says this level */
export function batteryIcon(charge, charging) {
  if (charging) return "batteryCharging";
  if (charge < 0) return "battery";
  if (charge <= 10) return "batteryWarning";
  if (charge <= 35) return "batteryLow";
  if (charge <= 75) return "batteryMedium";
  return "batteryFull";
}

/** which bars glyph says this signal (0–4 from the phone) */
export function signalIcon(strength) {
  if (strength <= 0) return "signalZero";
  if (strength === 1) return "signalLow";
  if (strength === 2) return "signalMedium";
  if (strength === 3) return "signalHigh";
  return "signal";
}

export const deviceIcon = (type) =>
  ({ phone: "phone", smartphone: "phone", tablet: "tablet", laptop: "laptop", desktop: "monitor", tv: "tv" })[type] ||
  "phone";

/** "/home/you/Downloads/a b.jpg" → "a b.jpg" */
export const baseName = (path) => String(path ?? "").split("/").filter(Boolean).pop() || "";
