import { test } from "node:test";
import assert from "node:assert/strict";
import { filterApps, letterOf, packageOf, readWhen } from "../src/lib/apps.js";

const LIST = [
  { package: "com.whatsapp", name: "WhatsApp", sys: false },
  { package: "com.android.settings", name: "Settings", sys: true },
  { package: "com.alderbank.mobile", name: "Alder Bank", sys: false },
  { package: "org.mozilla.firefox", name: "Firefox", sys: false },
  { package: "com.example.accent", name: "Élève", sys: false }
];

test("an empty query is the whole list", () => {
  assert.equal(filterApps(LIST, ""), LIST);
  assert.equal(filterApps(LIST, "   "), LIST);
});

test("search by name: every word, any case, accents folded", () => {
  assert.deepEqual(filterApps(LIST, "whats").map((a) => a.package), ["com.whatsapp"]);
  assert.deepEqual(filterApps(LIST, "bank alder").map((a) => a.package), ["com.alderbank.mobile"]);
  assert.deepEqual(filterApps(LIST, "alder bank").map((a) => a.package), ["com.alderbank.mobile"]);
  assert.deepEqual(filterApps(LIST, "ele").map((a) => a.package), ["com.example.accent"]);
  assert.deepEqual(filterApps(LIST, "nothing here"), []);
});

test("search by package when the query has a dot, and by a package part", () => {
  assert.deepEqual(filterApps(LIST, "org.mozilla").map((a) => a.package), ["org.mozilla.firefox"]);
  assert.deepEqual(filterApps(LIST, "mozilla").map((a) => a.package), ["org.mozilla.firefox"]);
  assert.deepEqual(filterApps(LIST, "alderbank").map((a) => a.package), ["com.alderbank.mobile"]);
});

test("the letter of an app without an icon", () => {
  assert.equal(letterOf("WhatsApp"), "W");
  assert.equal(letterOf("  firefox"), "F");
  assert.equal(letterOf("მარიამ"), "მ");
  assert.equal(letterOf("1Password"), "1");
  assert.equal(letterOf("…"), "#");
  assert.equal(letterOf(""), "?");
  assert.equal(letterOf(null), "?");
});

test("the package behind a notification", () => {
  assert.equal(packageOf("0|com.instagram.android|64278|direct|1523925756;thread_id:3"), "com.instagram.android");
  assert.equal(packageOf("0|com.whatsapp|1"), "com.whatsapp");
  assert.equal(packageOf("0|../etc|1"), "");
  assert.equal(packageOf("0|has space|1"), "");
  assert.equal(packageOf("plain"), "");
  assert.equal(packageOf(""), "");
  assert.equal(packageOf(undefined), "");
});

test("when the list was read, in words", () => {
  const now = 1_000_000_000_000;
  assert.equal(readWhen(0), "");
  assert.equal(readWhen(now - 10_000, now), "just now");
  assert.equal(readWhen(now - 5 * 60_000, now), "5 min ago");
  assert.equal(readWhen(now - 3 * 3_600_000, now), "3 h ago");
  assert.equal(readWhen(now - 2 * 86_400_000, now), "2 days ago");
});
