// The pages the installer shows when things are not as usual, on the host.
//
//     node dev/browser/states.mjs
//
// It starts its own installerd (--dry-run) and its own installer-gxwi, so it
// needs no VM and nothing else running, and then takes each away and puts it
// back while a browser watches:
//
//   - another front end moves the conversation on to a page that is not
//     drawn here yet, and back;
//   - installerd goes, and comes back;
//   - the installer itself goes, and comes back.
//
// installerd and msip-drive come from ../installer (cargo +1.98.1 build -p
// installerd -p msip-drive) and installer-gxwi from this checkout (cargo
// +1.98.1 build).
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "installer-gxwi-"));
const socket = join(run, "installerd.sock");
const address = "127.0.0.1:7791";
const site = `http://${address}/`;
const port = 9371;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// installer-gxwi links libpeios, which is found beside the sibling checkout.
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const startInstallerd = () => spawn(join(installer, "installerd"), ["--dry-run", "--socket", socket], { stdio: "ignore" });
const startInstaller = () => spawn(join(root, "target/debug/installer-gxwi"), ["--socket", socket, "--listen", address], { stdio: "ignore", env });
/** Another front end to the same conversation, pressing one action and leaving. */
const press = (action) => {
    try {
        execFileSync(join(installer, "msip-drive"), ["--socket", socket, "--press", action], { stdio: "ignore", timeout: 5000 });
    } catch {
        // It has nothing to press on the page it arrives at and says so, which is how it leaves.
    }
};

let installerd = startInstallerd();
await sleep(400);
let installerGxwi = startInstaller();
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
ws.onmessage = (m) => { const msg = JSON.parse(m.data); if (msg.id) waiting.get(msg.id)?.(msg); };
const send = (method, params = {}) => new Promise((r) => { const id = next++; waiting.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
const js = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true })).result.result?.value;
const picture = async (name) => writeFileSync(new URL(name, import.meta.url), Buffer.from((await send("Page.captureScreenshot")).result.data, "base64"));
const seen = () => js(`({
    heading: document.querySelector("#turn h1")?.textContent ?? null,
    lede: document.querySelector("#turn .lede")?.textContent ?? null,
    why: document.querySelector("#turn .why")?.textContent ?? null,
    actions: document.querySelectorAll("#turn .act").length,
    showing: document.getElementById("turn").classList.contains("in") && document.getElementById("page").classList.contains("live"),
    status: document.getElementById("status-text").textContent,
    statusIs: document.getElementById("status").className.replace("status", "").trim(),
    title: document.title,
    said: document.getElementById("say").textContent,
})`);
/** Looks until `until` holds of what is seen, or `seconds` pass. */
async function eventually(until, seconds = 10) {
    const started = Date.now();
    let last;
    while (Date.now() - started < seconds * 1000) {
        last = await seen();
        if (last && until(last)) return { ...last, after: Math.round((Date.now() - started) / 100) / 10 };
        await sleep(200);
    }
    return { ...last, after: null };
}
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing && s.status.startsWith("Connected to installerd");

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.enable");
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: "x", code: "KeyX", windowsVirtualKeyCode: 88 });
    out.first = await eventually(onFirstPage);
    expect("the first page is showing", out.first.after !== null);

    // Another front end answers the first page. The conversation is
    // installerd's and shared, so this browser is taken along.
    press("act.install");
    out.unbuilt = await eventually((s) => s.heading === "Choose a disk" && s.showing);
    await picture("states-1-unbuilt.png");
    expect("a page not drawn here yet is named and said to be one",
        out.unbuilt.after !== null && out.unbuilt.lede?.includes("not drawn") && out.unbuilt.why === "disk.choose" && out.unbuilt.actions === 0);
    expect("and names the tab", out.unbuilt.title === "Choose a disk · Peios Setup");
    press("nav.back");
    out.back = await eventually(onFirstPage);
    expect("going back elsewhere brings the first page back here", out.back.after !== null && out.back.title === "Peios Setup");

    // installerd goes.
    installerd.kill();
    out.lost = await eventually((s) => s.heading === "The installer is not answering" && s.showing);
    await picture("states-2-lost.png");
    expect("with installerd gone the page says so, and why", out.lost.after !== null && out.lost.statusIs === "bad" && /installerd closed the connection|connect /.test(out.lost.why ?? ""));
    expect("and says it to a screen reader, and in the tab", out.lost.said === "The installer is not answering" && out.lost.title.startsWith("The installer is not answering"));
    installerd = startInstallerd();
    out.found = await eventually(onFirstPage);
    expect("and carries on when it is back", out.found.after !== null && out.found.statusIs === "");

    // The installer itself goes: the page has nobody to hear from.
    installerGxwi.kill();
    out.away = await eventually((s) => s.heading === "Lost touch with this machine" && s.showing);
    await picture("states-3-away.png");
    expect("with the installer gone the page says it has lost touch", out.away.after !== null && out.away.statusIs === "bad" && out.away.said === "Lost touch with this machine");
    installerGxwi = startInstaller();
    out.home = await eventually(onFirstPage, 15);
    expect("and carries on when that is back", out.home.after !== null);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    ws.close();
    chrome.kill();
    installerGxwi.kill();
    installerd.kill();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
