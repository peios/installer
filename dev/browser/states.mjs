// The pages the installer shows when things are not as usual, on the host.
//
//     node dev/browser/states.mjs
//
// It starts its own installerd (--dry-run, as the machine dev/desktop.json
// describes) and its own installer-gxwi, so it needs no VM and nothing else
// running, and then takes each away and puts it back while a browser watches:
//
//   - another front end moves the conversation on, to a page drawn here and
//     then to one that is not yet, and back;
//   - installerd goes, and comes back;
//   - the installer itself goes, and comes back.
//
// installerd and msip-drive come from ../installer (cargo +1.98.1 build -p
// installerd -p msip-drive) and installer-gxwi from this checkout (cargo
// +1.98.1 build).
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "installer-gxwi-"));
const socket = join(run, "installerd.sock");
const address = "127.0.0.1:7791";
const site = `http://${address}/`;
// installer-gxwi links libpeios, which is found beside the sibling checkout.
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const startInstallerd = () => spawn(join(installer, "installerd"),
    ["--dry-run", "--inventory", join(root, "dev/desktop.json"), "--socket", socket], { stdio: "ignore" });
const startInstaller = () => spawn(join(root, "target/debug/installer-gxwi"), ["--socket", socket, "--listen", address], { stdio: "ignore", env });
/** Another front end to the same conversation, pressing one action and leaving. */
const press = (action, ...more) => {
    try {
        execFileSync(join(installer, "msip-drive"), ["--socket", socket, ...more, "--press", action], { stdio: "ignore", timeout: 5000 });
    } catch {
        // It has nothing to press on the page it arrives at and says so, which is how it leaves.
    }
};

let installerd = startInstallerd();
await sleep(400);
let installerGxwi = startInstaller();
const chrome = await browser(9371);
const { send, js, picture } = chrome;

const seen = () => js(`({
    heading: document.querySelector("#turn h1")?.textContent ?? null,
    lede: document.querySelector("#turn .lede")?.textContent ?? null,
    why: document.querySelector("#turn .why")?.textContent ?? null,
    actions: document.querySelectorAll("#turn .act").length,
    disks: document.querySelectorAll("#turn .disk").length,
    showing: document.getElementById("turn").classList.contains("in") && document.getElementById("page").classList.contains("live"),
    status: document.getElementById("status-text").textContent,
    statusIs: document.getElementById("status").className.replace("status", "").trim(),
    title: document.title,
    said: document.getElementById("say").textContent,
})`);
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing && s.status.startsWith("Connected to installerd");

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await chrome.key("x", "KeyX", 88);
    out.first = await eventually(seen, onFirstPage);
    expect("the first page is showing", out.first.after !== null);

    // Another front end answers the first page. The conversation is
    // installerd's and shared, so this browser is taken along.
    press("act.install");
    out.taken = await eventually(seen, (s) => s.heading === "Choose a disk" && s.showing);
    expect("answered elsewhere, the next page arrives here too", out.taken.after !== null && out.taken.disks === 5);
    expect("and names the tab", out.taken.title === "Choose a disk · Peios Setup");

    // And then to a page that is not drawn here yet.
    press("nav.next", "--set", "disk.target=/dev/sdc");
    out.unbuilt = await eventually(seen, (s) => s.heading === "Ready to install" && s.showing);
    await picture("states-1-unbuilt.png");
    expect("a page not drawn here yet is named and said to be one",
        out.unbuilt.after !== null && out.unbuilt.lede?.includes("not drawn") && out.unbuilt.why === "confirm" && out.unbuilt.actions === 0);
    press("nav.back");
    out.back = await eventually(seen, (s) => s.heading === "Choose a disk" && s.showing && s.disks === 5);
    expect("going back elsewhere brings the disks back here", out.back.after !== null);
    press("nav.back");
    out.home = await eventually(seen, onFirstPage);
    expect("and then the first page", out.home.after !== null && out.home.title === "Peios Setup");

    // installerd goes.
    installerd.kill();
    out.lost = await eventually(seen, (s) => s.heading === "The installer is not answering" && s.showing);
    await picture("states-2-lost.png");
    expect("with installerd gone the page says so, and why", out.lost.after !== null && out.lost.statusIs === "bad" && /installerd closed the connection|connect /.test(out.lost.why ?? ""));
    expect("and says it to a screen reader, and in the tab", out.lost.said === "The installer is not answering" && out.lost.title.startsWith("The installer is not answering"));
    installerd = startInstallerd();
    out.found = await eventually(seen, onFirstPage);
    expect("and carries on when it is back", out.found.after !== null && out.found.statusIs === "");

    // The installer itself goes: the page has nobody to hear from.
    installerGxwi.kill();
    out.away = await eventually(seen, (s) => s.heading === "Lost touch with this machine" && s.showing);
    await picture("states-3-away.png");
    expect("with the installer gone the page says it has lost touch", out.away.after !== null && out.away.statusIs === "bad" && out.away.said === "Lost touch with this machine");
    installerGxwi = startInstaller();
    out.again = await eventually(seen, onFirstPage, 15);
    expect("and carries on when that is back", out.again.after !== null);

    out.problems = chrome.problems.filter((p) => !/WebSocket|ERR_CONNECTION_REFUSED/.test(p));
    expect("the page reports no errors but the connections it lost", out.problems.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    installerGxwi.kill();
    installerd.kill();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
