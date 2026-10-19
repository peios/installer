// A headless Chromium to look at the installer with, driven over its
// debugging port. What each check in this directory shares.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { REQUEST_TIMEOUT_MS, connect, elsewhere } from "./cdp.mjs";

const STARTUP_TIMEOUT_MS = 20_000;
const POLL_INTERVAL_MS = 500;

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Starts a browser whose debugging port is `port`, which each check keeps to
 * itself so that two can run at once. Everything the page logs, throws or
 * fetches is kept, in `problems` and `requests`.
 */
export async function browser(port, { startupTimeoutMs = STARTUP_TIMEOUT_MS } = {}) {
    const profile = mkdtempSync(join(tmpdir(), "installer-chromium-"));
    const chrome = spawn(
        "chromium",
        [
            "--headless",
            `--user-data-dir=${profile}`,
            "--disable-gpu",
            "--hide-scrollbars",
            "--blink-settings=preferredColorScheme=0",
            "--window-size=1280,800",
            `--remote-debugging-port=${port}`,
            "about:blank",
        ],
        { stdio: "ignore" },
    );
    let client;
    let socket;
    let failure;
    let closed = false;
    const close = () => {
        if (closed) return;
        closed = true;
        client?.close();
        if (!client) socket?.close();
        if (!chrome.kill()) rmSync(profile, { recursive: true, force: true });
    };
    chrome.on("error", (error) => {
        failure = error;
        close();
        if (!chrome.pid) rmSync(profile, { recursive: true, force: true });
    });
    chrome.on("exit", () => {
        failure ??= new Error("Chromium exited");
        close();
        rmSync(profile, { recursive: true, force: true });
    });
    const problems = [],
        requests = [];
    try {
        const target = await findTarget(port, startupTimeoutMs, () => failure);
        socket = new WebSocket(target.webSocketDebuggerUrl);
        client = await connect(socket, record, REQUEST_TIMEOUT_MS, close);
        if (closed) {
            client.close();
            throw failure ?? new Error("Chromium closed during startup");
        }
        return await session(client, close);
    } catch (error) {
        close();
        throw error;
    }

    function record(msg) {
        if (msg.method === "Log.entryAdded") {
            problems.push(`${msg.params.entry.level}: ${msg.params.entry.text}`);
        }
        if (msg.method === "Runtime.exceptionThrown") {
            problems.push(
                msg.params.exceptionDetails.exception?.description ??
                    msg.params.exceptionDetails.text,
            );
        }
        if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
            problems.push(msg.params.args.map((a) => a.value).join(" "));
        }
        if (msg.method === "Network.requestWillBeSent") requests.push(msg.params.request.url);
        if (msg.method === "Network.webSocketCreated") requests.push(msg.params.url);
    }
    async function session({ send }, close) {
        const js = async (expression) => {
            const { result } = await send("Runtime.evaluate", {
                expression,
                returnByValue: true,
                awaitPromise: true,
            });
            if (result.exceptionDetails) {
                throw new Error(
                    result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
                );
            }
            return result.result?.value;
        };
        for (const domain of ["Log", "Runtime", "Network", "Page"]) await send(`${domain}.enable`);
        return {
            send,
            js,
            problems,
            requests,
            /** Saves what the browser shows beside these scripts, as `name`. */
            picture: async (name) =>
                writeFileSync(
                    new URL(name, import.meta.url),
                    Buffer.from((await send("Page.captureScreenshot")).result.data, "base64"),
                ),
            /** Presses a key and lets it go. */
            key: async (k, code, vk) => {
                await send("Input.dispatchKeyEvent", {
                    type: "keyDown",
                    key: k,
                    code,
                    windowsVirtualKeyCode: vk,
                });
                await send("Input.dispatchKeyEvent", {
                    type: "keyUp",
                    key: k,
                    code,
                    windowsVirtualKeyCode: vk,
                });
            },
            /** Clicks the first element `selector` finds. */
            click: (selector) => js(`document.querySelector(${JSON.stringify(selector)}).click()`),
            /** The page's buttons a pointer cannot reach: scrolled to, each one's
            middle is under something else. click() presses a button
            whatever covers it, so a page can pass every check and still
            not be pressable; this is what a person's pointer finds. */
            unreachable: () =>
                js(`[...document.querySelectorAll("#turn button:not([hidden])")].filter((b) => b.getClientRects().length).map((b) => {
            b.scrollIntoView({ block: "nearest" });
            const r = b.getBoundingClientRect();
            const at = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
            return at === b || b.contains(at) ? null : b.textContent.trim() + " under " + (at?.closest("[class]")?.className ?? at?.tagName);
        }).filter(Boolean)`),
            /** Which requests went anywhere but `site`. */
            elsewhere: (site) => elsewhere(requests, site),
            close,
        };
    }
}

/** Looks with `look` until `until` holds of what it sees, or `seconds` pass.
    What was last seen comes back, with how long it took, or null if never. */
export async function eventually(look, until, seconds = 10) {
    const started = Date.now();
    let last;
    while (Date.now() - started < seconds * 1000) {
        last = await look();
        if (last && until(last)) {
            return { ...last, after: Math.round((Date.now() - started) / 100) / 10 };
        }
        await sleep(200);
    }
    return { ...last, after: null };
}

async function findTarget(port, timeoutMs, failure) {
    const deadlineMs = Date.now() + timeoutMs;
    while (Date.now() < deadlineMs) {
        if (failure()) throw failure();
        try {
            const response = await fetch(`http://127.0.0.1:${port}/json`, {
                signal: AbortSignal.timeout(
                    Math.max(1, Math.min(POLL_INTERVAL_MS, deadlineMs - Date.now())),
                ),
            });
            const target = (await response.json()).find((target) => target.type === "page");
            if (target) return target;
        } catch {
            // Chromium's debugging listener may not be ready yet; retry within the deadline.
        }
        await sleep(Math.min(POLL_INTERVAL_MS, Math.max(0, deadlineMs - Date.now())));
    }
    throw failure() ?? new Error("Chromium debugging endpoint did not become ready");
}
