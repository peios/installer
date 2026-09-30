// The intro and the first page, in headless Chromium.
//
//     node dev/browser/intro.mjs [URL]
//
// URL is where the installer is: http://127.0.0.1:7780/ by default, which is
// the dev VM with the installer set as GXWI's overlay (dev/overlay.sh on), or
// wherever dev/host.sh is serving.
//
// It watches the intro through without touching it, then looks at the page it
// lands on: that it says what installerd sent, that nothing was fetched from
// anywhere else, that the keyboard works, and that a narrow screen holds it.
// A second load presses a key early, which is how the intro is skipped.
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const site = process.argv[2] ?? "http://127.0.0.1:7780/";
const port = 9370;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
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
const picture = async (name) => writeFileSync(new URL(name, import.meta.url), Buffer.from((await send("Page.captureScreenshot")).result.data, "base64"));
const key = async (k, code, vk) => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: k, code, windowsVirtualKeyCode: vk });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk });
};
/** What the page is showing. */
const seen = () => js(`({
    settled: document.getElementById("stage").classList.contains("settled"),
    live: document.getElementById("page").classList.contains("live"),
    title: document.title,
    heading: document.querySelector("#turn h1")?.textContent ?? null,
    lede: document.querySelector("#turn .lede")?.textContent ?? null,
    actions: [...document.querySelectorAll("#turn .act")].map((a) => ({
        name: a.querySelector(".name").textContent, key: a.querySelector(".key").textContent,
        primary: a.classList.contains("primary"), highlighted: a.classList.contains("hl"),
        shown: getComputedStyle(a.closest(".rise")).opacity === "1",
    })),
    status: document.getElementById("status-text").textContent,
    statusShown: getComputedStyle(document.getElementById("status")).opacity,
    edition: document.getElementById("edition").textContent,
    release: document.getElementById("release").textContent,
    ticker: [...document.querySelectorAll("#ticker li")].map((li) => li.textContent),
    toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
    focused: document.activeElement?.closest?.(".act")?.querySelector(".name").textContent ?? document.activeElement?.tagName,
    header: document.getElementById("head-lockup").classList.contains("shown"),
    wide: document.documentElement.scrollWidth > innerWidth,
})`);

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Log.enable");
    await send("Runtime.enable");
    await send("Network.enable");
    await send("Page.enable");

    // The intro, left alone.
    await send("Page.navigate", { url: site });
    await sleep(1900);
    await picture("intro-1-gathering.png");
    out.gathering = await seen();
    expect("the intro is playing, and the page is not in the way of it", out.gathering.settled === false && out.gathering.live === false);
    expect("what the installer did is said as the intro plays", out.gathering.ticker.length >= 2 && out.gathering.ticker[1].startsWith("connect"));
    await sleep(1300);
    await picture("intro-2-formed.png");
    await sleep(1900);
    await picture("intro-3-named.png");
    out.named = await seen();
    expect("the release is named under the mark", out.named.release.length > 0 || out.named.edition.length === 0);
    await sleep(3400);
    await picture("intro-4-landed.png");
    out.landed = await seen();
    expect("the intro lands on a page that can be used", out.landed.settled && out.landed.live && out.landed.header);
    expect("the page is installerd's first", out.landed.heading === "Peios Setup" && out.landed.lede?.startsWith("Set up Peios on this machine"));
    expect("with its three actions, numbered and all showing",
        JSON.stringify(out.landed.actions.map((a) => [a.key, a.name, a.shown])) === JSON.stringify([
            ["1", "Install Peios", true], ["2", "Upgrade an installation", true], ["3", "Repair an existing system", true]]));
    expect("the one most likely wanted is marked, highlighted and has the keyboard",
        out.landed.actions[0]?.primary && out.landed.actions[0]?.highlighted && out.landed.focused === "Install Peios");
    expect("the status line says which installer answered", /^Connected to installerd\/\d/.test(out.landed.status) && out.landed.statusShown === "1");

    // The keyboard: arrows walk, a number chooses, and choosing does nothing yet but say so.
    await key("ArrowDown", "ArrowDown", 40);
    await sleep(200);
    out.walked = (await seen()).focused;
    expect("an arrow moves to the next action", out.walked === "Upgrade an installation");
    await key("3", "Digit3", 51);
    await sleep(400);
    out.chosen = await seen();
    await picture("intro-5-chosen.png");
    expect("a number chooses, and the page says the step is not built", out.chosen.toast === "Repair an existing system is not built yet." && out.chosen.heading === "Peios Setup");

    // Nothing came from anywhere but the installer, and nothing went wrong.
    const origin = new URL(site).origin;
    out.elsewhere = requests.filter((url) => !url.startsWith(origin) && !url.startsWith(origin.replace(/^http/, "ws")) && !url.startsWith("data:"));
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...problems];
    expect("the page reports no errors", out.problems.length === 0);

    // Skipped: a key pressed early goes straight to the page.
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    // The lockup glides for a second, and the page rises behind it.
    await sleep(2600);
    out.skipped = await seen();
    expect("a key skips the intro", out.skipped.settled && out.skipped.live && out.skipped.actions.length === 3 && out.skipped.actions.every((a) => a.shown));

    // Replayed from the button, and left to finish.
    await js(`document.getElementById("replay").click()`);
    await sleep(800);
    out.replaying = (await seen()).settled;
    await sleep(6600);
    out.replayed = await seen();
    expect("the intro replays and lands again", out.replaying === false && out.replayed.settled && out.replayed.live && out.replayed.focused === "Install Peios");

    // A phone.
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    await sleep(2600);
    out.phone = await seen();
    await picture("intro-6-phone.png");
    expect("a phone holds the page without scrolling sideways", out.phone.wide === false && out.phone.actions.length === 3);

    // Any other address comes here.
    await send("Emulation.clearDeviceMetricsOverride");
    await send("Page.navigate", { url: new URL("/desktop/files?open=1", site).href });
    await sleep(1500);
    out.elsewhereLands = await js(`location.pathname`);
    expect("an address left over from a desktop comes to the installer", out.elsewhereLands === "/");
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    ws.close();
    chrome.kill();
    process.exit(failed.length ? 1 : 0);
}
