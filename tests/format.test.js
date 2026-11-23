// node --test tests/   (npm test)
import test from "node:test";
import assert from "node:assert/strict";
import {
  numberKey, sameNumber, isNumber, initials, when, dayLabel, duration, smsParts, linkify, snippet, batteryIcon,
  signalIcon, baseName
} from "../src/lib/format.js";

test("two spellings of one number match", () => {
  assert.ok(sameNumber("+1 202 555 0101", "(202) 555-0101"));
  assert.ok(sameNumber("+12025550101", "202.555.0101"));
  assert.ok(sameNumber("+1 555 010 0100", "(555) 010-0100"));
  assert.ok(!sameNumber("+1 202 555 0101", "+1 202 555 0102"));
});

test("short codes and named senders are themselves", () => {
  assert.equal(numberKey("2020"), "2020");
  assert.ok(!sameNumber("2020", "92020"));
  assert.equal(numberKey("MyBank"), "mybank");
  assert.ok(sameNumber("MYBANK", "MyBank"));
  assert.ok(!sameNumber("", ""));
  assert.ok(isNumber("+1 202 555 0101"));
  assert.ok(isNumber("*100#"));
  assert.ok(!isNumber("MyBank"));
  assert.ok(!isNumber(""));
});

test("initials never uppercase Georgian", () => {
  assert.equal(initials("nino abashidze"), "NA");
  assert.equal(initials("Giorgi"), "G");
  assert.equal(initials("მარიამ კაპანაძე"), "მკ");
  assert.equal(initials("Dr. Ana-Maria Tsereteli Jr"), "DA");
  assert.equal(initials("+1 202 555 0101"), "");
  assert.equal(initials("2020"), "");
  assert.equal(initials(""), "");
});

test("when: relative, then the day, then the date", () => {
  const now = new Date(2026, 10, 23, 15, 0, 0).getTime(); // Mon 23 Nov 2026, 15:00
  const min = 60000;
  assert.equal(when(now - 20000, now, "en-GB"), "now");
  assert.equal(when(now - 5 * min, now, "en-GB"), "5 min");
  assert.equal(when(now - 3 * 60 * min, now, "en-GB"), "12:00");
  assert.equal(when(now - 20 * 60 * min, now, "en-GB"), "Yesterday");
  assert.equal(when(now - 3 * 24 * 60 * min, now, "en-GB"), "Fri");
  assert.equal(when(new Date(2026, 8, 17, 9).getTime(), now, "en-GB"), "17 Sept");
  assert.equal(when(new Date(2025, 11, 31, 9).getTime(), now, "en-GB"), "31 Dec 2025");
  assert.equal(when(0, now), "");
});

test("dayLabel names the day", () => {
  const now = new Date(2026, 10, 23, 15).getTime();
  assert.equal(dayLabel(now - 3600000, now, "en-GB"), "Today");
  assert.equal(dayLabel(new Date(2026, 10, 22, 23, 59).getTime(), now, "en-GB"), "Yesterday");
  assert.equal(dayLabel(new Date(2026, 10, 17, 9).getTime(), now, "en-GB"), "Tuesday 17 November");
  assert.match(dayLabel(new Date(2025, 4, 1, 9).getTime(), now, "en-GB"), /2025/);
});

test("duration", () => {
  assert.equal(duration(201000), "3:21");
  assert.equal(duration(4413541), "1:13:33");
  assert.equal(duration(0), "0:00");
  assert.equal(duration(-5), "0:00");
});

test("how many SMS a text becomes", () => {
  assert.deepEqual(smsParts(""), { parts: 0, length: 0, unicode: false });
  assert.equal(smsParts("a".repeat(160)).parts, 1);
  assert.equal(smsParts("a".repeat(161)).parts, 2);
  assert.equal(smsParts("a".repeat(306)).parts, 2);
  assert.equal(smsParts("a".repeat(307)).parts, 3);
  // the euro sign costs two in the 7-bit alphabet
  assert.equal(smsParts("€".repeat(80)).parts, 1);
  assert.equal(smsParts("€".repeat(81)).parts, 2);
  // Georgian makes the whole message 16-bit: 70, then 67 a part
  assert.equal(smsParts("ა".repeat(70)).parts, 1);
  assert.equal(smsParts("ა".repeat(71)).parts, 2);
  assert.ok(smsParts("გამარჯობა").unicode);
});

test("links are found, and only web links", () => {
  assert.deepEqual(linkify("see https://example.org/a?b=1."), [
    { text: "see " },
    { text: "https://example.org/a?b=1", url: "https://example.org/a?b=1" },
    { text: "." }
  ]);
  assert.deepEqual(linkify("no links here"), [{ text: "no links here" }]);
  assert.deepEqual(linkify("javascript:alert(1) file:///etc/passwd"), [{ text: "javascript:alert(1) file:///etc/passwd" }]);
  const two = linkify("(http://a.example) and https://b.example/x)");
  assert.deepEqual(two.filter((p) => p.url).map((p) => p.url), ["http://a.example", "https://b.example/x"]);
  assert.equal(two.map((p) => p.text).join(""), "(http://a.example) and https://b.example/x)");
});

test("a list row's one line", () => {
  assert.equal(snippet({ body: " two\nlines  here " }), "two lines here");
  assert.equal(snippet({ body: "", attachments: [{ mime: "image/jpeg" }] }), "Photo");
  assert.equal(snippet({ body: "", attachments: [{ mime: "video/mp4" }] }), "Video");
  assert.equal(snippet({ body: "", attachments: [{ mime: "text/x-vcard" }] }), "Attachment");
  assert.equal(snippet({ body: "", attachments: [] }), "");
});

test("glyphs for the phone's state", () => {
  assert.equal(batteryIcon(80, true), "batteryCharging");
  assert.equal(batteryIcon(5, false), "batteryWarning");
  assert.equal(batteryIcon(30, false), "batteryLow");
  assert.equal(batteryIcon(60, false), "batteryMedium");
  assert.equal(batteryIcon(100, false), "batteryFull");
  assert.equal(signalIcon(0), "signalZero");
  assert.equal(signalIcon(4), "signal");
  assert.equal(baseName("/home/you/Downloads/a b.jpg"), "a b.jpg");
});
