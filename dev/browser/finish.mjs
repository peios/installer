// The end of first-boot setup, in headless Chromium: Finish, setup applying
// what it was told, and the page going on to the sign-in page.
//
//     node dev/browser/finish.mjs
//
// It starts its own oobed (--dry-run) and oobe-gxwi, so it needs no VM and
// nothing else running, and plays GXWI's part at the end itself: when setup
// is done, oobed takes the overlay away and GXWI serves its sign-in page in
// its place, which here is oobe-gxwi stopped and a stand-in sign-in page
// started.
//
// Twice. First as setup mostly is: Finish, the applying page, its phases
// done, and once oobe-gxwi has gone the page goes to the sign-in page at the
// same address. Then with an address given by hand to the interface the page
// came in through (127.0.0.2, as dev/net-status.txt and the line added here
// say): the sign-in page comes back only at the new address, as the machine
// would, and the page follows it there. oobed gives no interface a loopback
// address, so the new one is this machine's own on its network, which the
// stand-in sign-in page answers on; without one, that half is not tried.
//
// oobed comes from ../installer (cargo +1.98.1 build -p oobed) and oobe-gxwi
// from this checkout (cargo +1.98.1 build).
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { networkInterfaces, tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "oobe-gxwi-"));
const socket = join(run, "oobed.sock");
const status = join(run, "net-status.txt");
const port = 7798;
const site = `http://127.0.0.2:${port}/`;
// This machine's address on its network, and the length of that network.
const lan = Object.values(networkInterfaces()).flat().find((a) => a.family === "IPv4" && !a.internal);
const moved = lan && `http://${lan.address}:${port}/`;
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

writeFileSync(status, readFileSync(join(root, "dev/net-status.txt"), "utf8") + `
enp4s0  [5a5a5a5a-0000-5000-8000-000000000004]
  verdict    JOIN(default) by wired
  state      up, carrier
  readiness  addressed
  hardware   52:54:00:00:00:04 pci-0000:04:00.0 virtio_net
  address    127.0.0.2/8
`);

let oobed = null, oobeGxwi = null, signIn = null;
async function setupUp() {
    rmSync(socket, { force: true });
    oobed = spawn(join(installer, "oobed"), ["--dry-run", "--socket", socket, "--net-status", status], { stdio: "ignore" });
    await sleep(400);
    oobeGxwi = spawn(join(root, "target/debug/oobe-gxwi"), ["--socket", socket, "--listen", `127.0.0.2:${port}`], { stdio: "ignore", env });
    await sleep(400);
}
/** What GXWI does when oobed takes the overlay away: setup goes, and the
    sign-in page is served, at `host`. */
async function setupDone(host) {
    oobeGxwi.kill();
    oobed.kill();
    await sleep(300);
    signIn = createServer((request, response) => {
        // GXWI's sign-in page answers anything it does not know with itself.
        response.writeHead(request.url === "/" ? 200 : 401, { "content-type": "text/html" });
        response.end("<!doctype html><title>Sign in</title><h1>Sign in</h1>");
    }).listen(port, host);
}
function signInDown() {
    signIn?.close();
    signIn = null;
}

const chrome = await browser(9384);
const { send, js, picture, key, click } = chrome;
const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    return {
        url: location.href, title: document.title,
        kind: turn?.dataset.kind ?? null,
        showing: !!turn && turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        heading: turn?.querySelector("h1")?.textContent ?? null,
        phases: [...(turn?.querySelectorAll(".phase") ?? [])].map((p) => [p.querySelector(".pname").textContent, p.className]),
        signIn: !!turn?.querySelector('[data-way="sign-in"]:not([hidden])'),
        again: !!turn?.querySelector('[data-way="again"]:not([hidden])'),
        going: turn?.querySelector(".going:not([hidden])")?.textContent ?? null,
        status: document.getElementById("status-text")?.textContent ?? null,
    };
})()`);
const type = (id, text) => js(`(() => { const i = document.getElementById(${JSON.stringify(id)}); i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input")); })()`);
const at = (kind) => eventually(seen, (s) => s.kind === kind && s.showing, 10);

/** From the welcome to Finish, with an address given by hand if `address`. */
async function throughSetup(address) {
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    await eventually(seen, (s) => s.kind === "welcome" && s.showing, 15);
    await click("#turn .btn.go-on");
    await at("network");
    if (address) {
        await click("#turn .aux .link:nth-of-type(2)");
        await at("manual");
        await click('#turn .iface[data-value="enp4s0"]');
        await type("field-manual-address", address);
        await click("#turn .nav .btn.go-on");
        await eventually(seen, (s) => s.kind === "network" && s.showing && s.heading === "Network", 10);
        await sleep(500);
    }
    await click("#turn .nav .btn.go-on");
    await at("account");
    await type("field-account-name", "jack");
    await type("field-account-password", "correct horse");
    await type("field-account-confirm", "correct horse");
    await click("#turn .nav .btn.go-on");
    await at("naming");
    await type("field-hostname", "workshop");
    await click("#turn .nav .btn.go-on");
}

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    // As setup mostly is.
    await setupUp();
    await throughSetup(null);
    out.applying = await eventually(seen, (s) => s.kind === "progress" && s.showing, 10);
    out.done = await eventually(seen, (s) => s.signIn, 15);
    await sleep(1200);
    out.done = await seen();
    await picture("finish-1-done.png");
    expect("Finish goes on to setup applying what it was told", out.applying.after !== null);
    expect("its phases, all done", JSON.stringify(out.done.phases.map(([name]) => name)) === JSON.stringify(["Creating your account", "Naming the machine"])
        && out.done.phases.every(([, how]) => how.includes("done")));
    expect("finished, it says so in oobed's words and offers the sign-in page, not starting again",
        out.done.heading === "Setup is complete" && out.done.signIn && !out.done.again);
    out.unreachable = await chrome.unreachable();
    expect("a pointer reaches every button", out.unreachable.length === 0);
    await setupDone("127.0.0.2");
    out.signedIn = await eventually(seen, (s) => s.title === "Sign in", 15);
    expect("setup gone, the page goes to the sign-in page where it was", out.signedIn.after !== null && out.signedIn.url === site);
    signInDown();

    // With the address the page came in by given another by hand.
    if (!lan) throw new Error("no address but the loopback to move to");
    await setupUp();
    await throughSetup(lan.cidr);
    out.movedDone = await eventually(seen, (s) => s.signIn, 15);
    await sleep(1200);
    out.movedDone = await seen();
    expect("the address given by hand is a phase of its own, last", out.movedDone.phases.at(-1)?.[0] === "Addressing enp4s0");
    await setupDone(lan.address);
    out.following = await eventually(seen, (s) => s.going !== null, 10);
    await picture("finish-2-following.png");
    expect("the page says it is following the machine", out.following.going === `Following this machine to ${lan.address}.`
        && out.following.status === "Setup has finished");
    out.movedIn = await eventually(seen, (s) => s.title === "Sign in", 15);
    expect("and goes to the sign-in page at the new address", out.movedIn.after !== null && out.movedIn.url === moved);
    signInDown();

    // Asking for setup where it has gone is answered by the sign-in page, as
    // GXWI's is, and the browser says so.
    out.problems = chrome.problems.filter((p) => !/ERR_CONNECTION_REFUSED|Failed to load resource|WebSocket connection to .* failed/.test(p));
    expect("the page reports no errors", out.problems.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    oobeGxwi?.kill();
    oobed?.kill();
    signInDown();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
