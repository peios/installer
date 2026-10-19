// Load the real harness against fake Chromium and DevTools: no subprocess or network.
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import test from "node:test";
import vm from "node:vm";

import * as cdp from "./cdp.mjs";

async function fixture({ target = true, commandError = false } = {}) {
    const child = new EventEmitter();
    child.pid = 42;
    let kills = 0;
    let profile;
    child.kill = () => {
        kills++;
        if (kills === 1) queueMicrotask(() => child.emit("exit", 0));
        return true;
    };
    class Socket {
        readyState = 1;
        send(text) {
            const request = JSON.parse(text);
            queueMicrotask(() =>
                this.onmessage({
                    data: JSON.stringify(
                        commandError
                            ? { id: request.id, error: { message: "mock command failed" } }
                            : { id: request.id, result: {} },
                    ),
                }),
            );
        }
        close() {
            this.onclose?.();
        }
    }
    const context = vm.createContext({
        setTimeout,
        clearTimeout,
        URL,
        Buffer,
        Date,
        AbortSignal,
        WebSocket: Socket,
        fetch: async () => ({
            json: async () => (target ? [{ type: "page", webSocketDebuggerUrl: "ws://mock" }] : []),
        }),
    });
    const imports = {
        "node:child_process": {
            spawn(_name, args) {
                profile = args.find((arg) => arg.startsWith("--user-data-dir=")).split("=")[1];
                return child;
            },
        },
        "node:fs": fs,
        "node:os": os,
        "node:path": path,
        "./cdp.mjs": cdp,
    };
    const module = new vm.SourceTextModule(
        fs.readFileSync(new URL("./chrome.mjs", import.meta.url), "utf8"),
        { context },
    );
    await module.link((name) => {
        const values = imports[name];
        return new vm.SyntheticModule(
            Object.keys(values),
            function () {
                for (const [key, value] of Object.entries(values)) this.setExport(key, value);
            },
            { context },
        );
    });
    await module.evaluate();
    return { browser: module.namespace.browser, child, kills: () => kills, profile: () => profile };
}

test("startup deadline terminates the child and removes its private profile", async () => {
    const f = await fixture({ target: false });
    await assert.rejects(f.browser(1, { startupTimeoutMs: 10 }), /did not become ready/);
    assert.equal(f.kills(), 1);
    assert.equal(fs.existsSync(f.profile()), false);
});

test("initialization command failure terminates Chromium and removes its profile", async () => {
    const f = await fixture({ commandError: true });
    await assert.rejects(f.browser(1), /mock command failed/);
    assert.equal(f.kills(), 1);
    assert.equal(fs.existsSync(f.profile()), false);
});

test("normal close is idempotent and waits for child exit to remove the profile", async () => {
    const f = await fixture();
    const browser = await f.browser(1);
    assert.equal(fs.existsSync(f.profile()), true);
    browser.close();
    browser.close();
    await Promise.resolve();
    assert.equal(f.kills(), 1);
    assert.equal(fs.existsSync(f.profile()), false);
});
