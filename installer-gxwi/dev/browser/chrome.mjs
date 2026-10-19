// A headless Chromium to look at the installer with, driven over its
// debugging port. What each check in this directory shares.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Starts a browser whose debugging port is `port`, which each check keeps to
 * itself so that two can run at once. Everything the page logs, throws or
 * fetches is kept, in `problems` and `requests`.
 */
export async function browser(port) {
    const chrome = spawn("chromium", ["--headless", "--disable-gpu", "--hide-scrollbars",
        "--blink-settings=preferredColorScheme=0", "--window-size=1280,800",
        `--remote-debugging-port=${port}`, "about:blank"], { stdio: "ignore" });
    let target;
    for (let i = 0; i < 40 && !target; i++) {
        await sleep(500);
        try {
            target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
        } catch {}
    }
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((r) => (ws.onopen = r));
    let next = 1;
    const waiting = new Map();
    const problems = [], requests = [];
    ws.onmessage = (m) => {
        const msg = JSON.parse(m.data);
        if (msg.id) return waiting.get(msg.id)?.(msg);
        if (msg.method === "Log.entryAdded") problems.push(`${msg.params.entry.level}: ${msg.params.entry.text}`);
        if (msg.method === "Runtime.exceptionThrown") problems.push(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
        if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") problems.push(msg.params.args.map((a) => a.value).join(" "));
        if (msg.method === "Network.requestWillBeSent") requests.push(msg.params.request.url);
        if (msg.method === "Network.webSocketCreated") requests.push(msg.params.url);
    };
    const send = (method, params = {}) => new Promise((r) => { const id = next++; waiting.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
    const js = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result.result?.value;
    for (const domain of ["Log", "Runtime", "Network", "Page"]) await send(`${domain}.enable`);
    return {
        send, js, problems, requests,
        /** Saves what the browser shows beside these scripts, as `name`. */
        picture: async (name) => writeFileSync(new URL(name, import.meta.url), Buffer.from((await send("Page.captureScreenshot")).result.data, "base64")),
        /** Presses a key and lets it go. */
        key: async (k, code, vk) => {
            await send("Input.dispatchKeyEvent", { type: "keyDown", key: k, code, windowsVirtualKeyCode: vk });
            await send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk });
        },
        /** Clicks the first element `selector` finds. */
        click: (selector) => js(`document.querySelector(${JSON.stringify(selector)}).click()`),
        /** The page's buttons a pointer cannot reach: scrolled to, each one's
            middle is under something else. click() presses a button
            whatever covers it, so a page can pass every check and still
            not be pressable; this is what a person's pointer finds. */
        unreachable: () => js(`[...document.querySelectorAll("#turn button:not([hidden])")].filter((b) => b.getClientRects().length).map((b) => {
            b.scrollIntoView({ block: "nearest" });
            const r = b.getBoundingClientRect();
            const at = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
            return at === b || b.contains(at) ? null : b.textContent.trim() + " under " + (at?.closest("[class]")?.className ?? at?.tagName);
        }).filter(Boolean)`),
        /** Which requests went anywhere but `site`. */
        elsewhere: (site) => {
            const origin = new URL(site).origin;
            return requests.filter((url) => !url.startsWith(origin) && !url.startsWith(origin.replace(/^http/, "ws")) && !url.startsWith("data:"));
        },
        close: () => { ws.close(); chrome.kill(); },
    };
}

/** Looks with `look` until `until` holds of what it sees, or `seconds` pass.
    What was last seen comes back, with how long it took, or null if never. */
export async function eventually(look, until, seconds = 10) {
    const started = Date.now();
    let last;
    while (Date.now() - started < seconds * 1000) {
        last = await look();
        if (last && until(last)) return { ...last, after: Math.round((Date.now() - started) / 100) / 10 };
        await sleep(200);
    }
    return { ...last, after: null };
}
