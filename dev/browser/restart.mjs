// Restarting the machine once an installation has finished, on the host.
//
//     node dev/browser/restart.mjs
//     LOST=1 node dev/browser/restart.mjs    # and the machine that never comes back: 3 minutes more
//
// It starts its own installerd (--dry-run, as the machine dev/desktop.json
// describes) and its own installer-gxwi, sees an installation through, and
// presses Reboot now. A dry run restarts nothing, so the machine's going is
// played here: the installer and installerd are stopped, and then something
// else answers at the address, as the machine that comes back would:
//
//   - Peios' own sign-in, which answers every address with a 401;
//   - the installer again, on another boot, which is the machine starting
//     from the medium;
//   - with LOST=1, nothing at all for as long as the page waits, and then
//     Peios after all.
//
// installerd and msip-drive come from ../installer (cargo +1.98.1 build -p
// installerd -p msip-drive) and installer-gxwi from this checkout (cargo
// +1.98.1 build).
import { execFileSync, spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "installer-gxwi-"));
const socket = join(run, "installerd.sock");
const [host, port] = ["127.0.0.1", 7795];
const site = `http://${host}:${port}/`;
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

let installerd = null, installerGxwi = null, standIn = null;
const startMachine = async () => {
    installerd = spawn(join(installer, "installerd"),
        ["--dry-run", "--inventory", join(root, "dev/desktop.json"), "--socket", socket], { stdio: "ignore" });
    await sleep(400);
    installerGxwi = spawn(join(root, "target/debug/installer-gxwi"), ["--socket", socket, "--listen", `${host}:${port}`], { stdio: "ignore", env });
    await sleep(600);
};
/** The machine goes down: nothing answers at the address. */
const down = async () => {
    installerGxwi?.kill();
    installerd?.kill();
    await new Promise((r) => (standIn ? standIn.close(r) : r()));
    standIn = null;
    await sleep(300);
};
/** Something answers at the address, as `answer` does. */
const comesBack = (answer) => new Promise((r) => {
    standIn = createServer(answer);
    standIn.listen(port, host, r);
});
// Peios' own GXWI on the installed system: everything wants a sign-in.
const peios = (req, res) => { res.writeHead(401, { "content-type": "text/html" }); res.end("<!doctype html><title>Sign in</title>"); };
// The installer, on the next boot of the machine: it started from the medium.
const installerAgain = (req, res) => {
    if (req.url === "/hello") {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify({ installer: "installer-gxwi/0.0.0", boot: "another-boot" }));
    }
    res.writeHead(200, { "content-type": "text/html" });
    res.end("<!doctype html><title>Peios Setup</title>");
};

/** Another front end to the same conversation, pressing one action and leaving. */
const press = (action, ...more) => {
    try {
        execFileSync(join(installer, "msip-drive"), ["--socket", socket, ...more, "--press", action], { stdio: "ignore", timeout: 5000 });
    } catch {
        // It has nothing to press on the page it arrives at and says so, which is how it leaves.
    }
};

const chrome = await browser(9377);
const { send, js, picture, click } = chrome;

const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const stage = document.getElementById("stage");
    const lockup = document.getElementById("intro-lockup");
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        ledes: [...turn.querySelectorAll(".lede")].map((p) => p.textContent),
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        reboot: turn.querySelector('[data-way="reboot"]:not([hidden])')?.textContent ?? null,
        buttons: [...turn.querySelectorAll(".btn")].map((b) => b.textContent),
        cases: [...turn.querySelectorAll(".cases b")].map((b) => b.textContent),
        rebooting: stage.classList.contains("rebooting"),
        stalled: stage.classList.contains("stalled"),
        mark: ["formed", "drawn", "sleeping", "popped", "word-in", "gone"].filter((c) => lockup.classList.contains(c)),
        ticker: [...document.querySelectorAll("#ticker li")].map((li) => li.textContent + (li.classList.contains("waiting") ? "…" : "")),
        tickerShown: getComputedStyle(document.getElementById("ticker")).opacity === "1",
        headShown: document.getElementById("head-lockup").classList.contains("shown"),
        status: document.getElementById("status-text").textContent,
        focused: document.activeElement?.textContent?.trim() || document.activeElement?.tagName,
        title: document.title,
        said: document.getElementById("say").textContent,
        wide: document.documentElement.scrollWidth > innerWidth,
    };
})()`);

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

/** An installation seen through, from a fresh page, to Reboot now pressed. */
async function finishedAndRebooted(label) {
    await startMachine();
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await chrome.key("x", "KeyX", 88);
    await eventually(seen, (s) => s.heading === "Peios Setup" && s.showing, 15);
    press("act.install");
    await eventually(seen, (s) => s.heading === "Choose a disk" && s.showing);
    press("nav.next", "--set", "disk.target=/dev/nvme0n1");
    await eventually(seen, (s) => s.heading === "Ready to install" && s.showing);
    const driving = spawn(join(installer, "msip-drive"), ["--socket", socket, "--press", "act.begin"], { stdio: "ignore" });
    out[`${label}Finished`] = await eventually(seen, (s) => s.heading === "Installation complete" && s.reboot === "Reboot now" && s.showing, 60);
    driving.kill();
    await sleep(1600);
    await click('#turn [data-way="reboot"]');
}

try {
    // ---- Peios comes back ----
    await finishedAndRebooted("peios");
    expect("an installation finishes with the restart offered", out.peiosFinished.after !== null);
    out.going = await eventually(seen, (s) => s.rebooting && s.ticker[0] === "reboot asked of installerd" && s.tickerShown && !s.showing, 5);
    expect("pressed, the page goes, and the corner says the restart was asked of installerd",
        out.going.after !== null && !out.going.showing && out.going.tickerShown && out.going.title === "Restarting · Peios Setup" && out.going.said === "Restarting the machine.");
    out.gathered = await eventually(seen, (s) => s.mark.includes("sleeping"), 8);
    await picture("restart-1-away.png");
    expect("the stars make the mark again in the middle, and it sleeps while the machine is away",
        out.gathered.after !== null && out.gathered.mark.includes("formed") && out.gathered.mark.includes("drawn") && !out.gathered.headShown
        && out.gathered.ticker.at(-1) === "waiting for the machine to come back…");
    // The same installer on the same boot is the machine that has not gone.
    await sleep(3000);
    out.notYet = await seen();
    expect("while the installer that was asked still answers, the page waits", out.notYet.kind === "restart" && out.notYet.rebooting);

    await down();
    out.closed = await eventually(seen, (s) => s.ticker.some((l) => l.startsWith("closed")), 5);
    expect("the connection closing is noted, as it goes", out.closed.after !== null);
    await sleep(3000);
    await comesBack(peios);
    out.running = await eventually(seen, (s) => s.kind === "running" && s.showing, 20);
    await sleep(1200);
    out.running = { ...await seen(), after: out.running.after };
    await picture("restart-2-running.png");
    expect("Peios answering, the page lands on it, the lockup back in the corner",
        out.running.after !== null && out.running.heading === "Peios is running" && out.running.headShown && !out.running.rebooting
        && out.running.ticker.includes(`answered ${host}:${port}`));
    expect("it says what the machine started from, and that first-boot setup is on its own screen",
        out.running.ledes[0] === "The machine restarted from Samsung SSD 980 PRO 1TB (/dev/nvme0n1)." && out.running.ledes[1]?.startsWith("First-boot setup is waiting on the machine's own screen"));
    expect("the way on is signing in, and has the keyboard", JSON.stringify(out.running.buttons) === JSON.stringify(["Sign in"]) && out.running.focused === "Sign in");
    expect("the status and the tab say where it is", out.running.status === `Peios is running at ${host}` && out.running.title === "Peios is running · Peios Setup");

    // ---- the installer again ----
    await down();
    await finishedAndRebooted("again");
    await eventually(seen, (s) => s.mark.includes("sleeping"), 8);
    await down();
    await sleep(2000);
    await comesBack(installerAgain);
    out.again = await eventually(seen, (s) => s.kind === "again" && s.showing, 20);
    await sleep(1200);
    out.again = { ...await seen(), after: out.again.after };
    await picture("restart-3-again.png");
    expect("the installer answering on another boot is the machine starting from the medium, and the page says so",
        out.again.after !== null && out.again.heading === "It started the installer again"
        && out.again.ledes[0]?.startsWith("The machine started from the install medium instead of Samsung SSD 980 PRO 1TB (/dev/nvme0n1).")
        && JSON.stringify(out.again.buttons) === JSON.stringify(["Back to the installer"]));

    // ---- nothing comes back ----
    if (process.env.LOST) {
        await down();
        await finishedAndRebooted("lost");
        await eventually(seen, (s) => s.mark.includes("sleeping"), 8);
        await down();
        out.lost = await eventually(seen, (s) => s.kind === "lost" && s.showing, 200);
        await sleep(1200);
        out.lost = { ...await seen(), after: out.lost.after };
        await picture("restart-4-lost.png");
        expect("nothing answering for three minutes, the page says the machine has not come back, and what its screen might show",
            out.lost.after !== null && out.lost.heading === "The machine hasn't come back" && out.lost.stalled && out.lost.cases.length === 3
            && out.lost.buttons[0] === "Keep waiting" && out.lost.focused === "Keep waiting");
        await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
        await sleep(400);
        out.lostPhone = await seen();
        await picture("restart-5-phone.png");
        expect("a phone holds it without scrolling sideways", out.lostPhone.wide === false);
        await send("Emulation.clearDeviceMetricsOverride");
        await comesBack(peios);
        out.lateComer = await eventually(seen, (s) => s.kind === "running" && s.showing, 30);
        expect("and when Peios does answer after all, the page goes to it", out.lateComer.after !== null && !out.lateComer.stalled);
    }

    out.problems = chrome.problems.filter((p) => !/WebSocket|ERR_CONNECTION_REFUSED|ERR_EMPTY_RESPONSE|401|Failed to load resource/.test(p));
    expect("the page reports no errors but the connections it lost", out.problems.length === 0);
    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    await down();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
