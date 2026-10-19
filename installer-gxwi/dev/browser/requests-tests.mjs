import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const module = new vm.SourceTextModule(
    readFileSync(new URL("../../page/requests.js", import.meta.url), "utf8"),
    { context: vm.createContext({ TextEncoder }) },
);
await module.link(() => {
    throw new Error("Unexpected request helper import");
});
await module.evaluate();
const { MAX_REQUEST_BYTES, canAsk, encodeRequest, refusalMessage } = module.namespace;

test("request size admission counts UTF-8 bytes including JSON overhead", () => {
    const overhead = encodeRequest({ values: { text: "" } }, 1).length;
    const exact = encodeRequest({ values: { text: "x".repeat(MAX_REQUEST_BYTES - overhead) } }, 1);
    assert.equal(new TextEncoder().encode(exact).length, MAX_REQUEST_BYTES);
    assert.equal(
        encodeRequest({ values: { text: "x".repeat(MAX_REQUEST_BYTES - overhead + 1) } }, 1),
        null,
    );
    assert.equal(encodeRequest({ values: { text: "é".repeat(MAX_REQUEST_BYTES / 2) } }, 1), null);
});

test("retained old view cannot authorize input before a fresh socket snapshot", () => {
    const retained = { seq: 1 };
    assert.equal(canAsk({ press: "next" }, retained, false), false);
    assert.equal(canAsk({ again: true }, retained, false), false);
    assert.equal(canAsk({ press: "next" }, retained, true), true);
    assert.equal(canAsk({}, { seq: 0 }, true), false);
    assert.equal(canAsk({ again: true }, { seq: 0 }, true), true);
    assert.equal(canAsk({}, null, true), false);
});

test("refusals explain oversized and interrupted input without replay", () => {
    assert.match(refusalMessage(1009), /Shorten the input/);
    assert.match(refusalMessage(1012), /not replayed/);
    assert.equal(refusalMessage(1000), "");
    assert.equal(JSON.parse(encodeRequest({ seq: 99, press: "next" }, 1)).seq, 1);
});
