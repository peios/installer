// The installation as it runs, and how it ends, on the host.
//
//     node dev/browser/progress.mjs
//
// It starts its own installerd (--dry-run) and its own installer-gxwi, so it
// needs no VM and nothing else running. installerd is told to be the machine
// dev/desktop.json describes, and only pretends to install: each phase counts
// up over a few seconds and nothing is written anywhere.
//
// It holds the confirmation's button the whole way, as a person does, and
// watches the job: the phases, the one figure, the disk being made, what the
// job says, and a browser that arrives part way. It sees it finish, saves
// what it said, and goes back to the start. Then it does it again with an
// installerd told to fail part way through the copy (--fail-at copy), and
// looks at what a job that stopped leaves on the page.
//
// installerd comes from ../installer (cargo +1.98.1 build -p installerd) and
// installer-gxwi from this checkout (cargo +1.98.1 build).
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "installer-gxwi-"));
const socket = join(run, "installerd.sock");
const address = "127.0.0.1:7794";
const site = `http://${address}/`;
// installer-gxwi links libpeios, which is found beside the sibling checkout.
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const startInstallerd = (machine, ...more) => spawn(join(installer, "installerd"),
    ["--dry-run", "--inventory", machine, "--socket", socket, ...more], { stdio: "ignore" });

// A disk made by someone who hoped its names would be taken for markup.
const hostile = join(run, "hostile.json");
const desktop = JSON.parse(readFileSync(join(root, "dev/desktop.json"), "utf8"));
writeFileSync(hostile, JSON.stringify({
    disks: [{ ...desktop.disks[2], model: '<img src=x onerror="document.title=1">', partitions: [] }],
}));

let installerd = startInstallerd(join(root, "dev/desktop.json"));
await sleep(400);
const installerGxwi = spawn(join(root, "target/debug/installer-gxwi"), ["--socket", socket, "--listen", address], { stdio: "ignore", env });
const chrome = await browser(9375);
const { send, js, picture, key, click } = chrome;

/** What the page is showing. */
const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const build = turn.querySelector(".build");
    const number = (of, name) => of ? Number(getComputedStyle(of).getPropertyValue(name) || 0) : null;
    const log = turn.querySelector(".log");
    const again = turn.querySelector(".nav.ends .btn");
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        shown: [...turn.querySelectorAll(".rise:not([hidden])")].every((r) => getComputedStyle(r).opacity === "1"),
        actions: turn.querySelectorAll(".act").length,
        lede: turn.querySelector(".lede:not([hidden])")?.textContent ?? null,
        installerdSaid: turn.querySelector(".said:not([hidden]) code")?.textContent ?? null,
        figure: turn.querySelector(".pct .num")?.textContent ?? null,
        overall: turn.querySelector(".pct")?.getAttribute("aria-valuenow") ?? null,
        overallShown: !!turn.querySelector(".overall") && getComputedStyle(turn.querySelector(".overall")).display !== "none",
        now: [...turn.querySelectorAll(".overall-now > *")].map((n) => n.textContent),
        phases: [...turn.querySelectorAll(".phase")].map((li) => ({
            name: li.querySelector(".pname").textContent,
            state: li.className.replace("phase", "").trim(),
            says: li.querySelector(".pval").textContent,
            now: Number(li.querySelector(".ptrack").getAttribute("aria-valuenow")),
        })),
        finished: turn.classList.contains("finished"),
        stopped: turn.classList.contains("stopped"),
        disk: {
            there: !!build && !build.hidden,
            model: build?.querySelector(".plan-title b")?.textContent ?? null,
            under: build?.querySelector(".plan-title span")?.textContent ?? null,
            drawn: build?.querySelector(".layout")?.getAttribute("aria-label") ?? null,
            parts: [...(build?.querySelectorAll(".part") ?? [])].map((li) => ({
                is: li.querySelector("b").textContent + ": " + li.querySelector("span").textContent, done: li.classList.contains("done"),
            })),
            made: number(build, "--made"), fmt: number(build, "--fmt"), copy: number(build, "--copy"), boot: number(build, "--boot"),
            doing: build?.className.replace(/build|rise/g, "").trim() ?? null,
        },
        log: {
            lines: log ? log.children.length : null,
            first: log?.firstElementChild?.textContent ?? null,
            last: log?.lastElementChild?.textContent ?? null,
            commands: log ? log.querySelectorAll(".cmd").length : null,
            scrolls: log ? getComputedStyle(log).overflowY === "auto" : null,
            atEnd: log ? log.scrollTop + log.clientHeight >= log.scrollHeight - 4 : null,
            reachable: log?.getAttribute("tabindex") ?? null,
        },
        save: turn.querySelector(".details a")?.getAttribute("href") ?? null,
        again: again && !turn.querySelector(".nav.ends").hidden ? {
            says: again.textContent, how: again.className, off: again.getAttribute("aria-disabled") === "true",
            // Whether it is clear of the bar along the bottom of the screen.
            clear: again.getBoundingClientRect().bottom <= document.getElementById("status").getBoundingClientRect().top,
        } : null,
        heat: Number(getComputedStyle(document.getElementById("stage")).getPropertyValue("--heat") || 0),
        status: document.getElementById("status-text").textContent,
        statusIs: document.getElementById("status").className.replace("status", "").trim(),
        focused: document.activeElement?.tagName === "H1" ? "the heading" : document.activeElement?.textContent?.trim() ?? document.activeElement?.tagName,
        title: document.title,
        said: document.getElementById("say").textContent,
        wide: document.documentElement.scrollWidth > innerWidth,
        diskBeforePhases: (() => { const p = turn.querySelector(".build"), n = turn.querySelector(".phases"); return p && n ? p.getBoundingClientRect().top < n.getBoundingClientRect().top : null; })(),
        markup: turn.querySelectorAll("img, script, #planted").length,
        committed: turn.classList.contains("committed"),
        flashed: document.getElementById("flash").classList.contains("go"),
        disks: [...turn.querySelectorAll(".disk")].map((d) => ({ value: d.dataset.value, checked: d.getAttribute("aria-checked") === "true" })),
        next: turn.querySelector(".btn.go-on")?.getAttribute("aria-disabled") === "true" ? "off" : "on",
    };
})()`);
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing;
const onDisks = (s) => s.heading === "Choose a disk" && s.showing && s.disks.length > 0;
const onConfirm = (s) => s.kind === "confirm" && s.showing && s.shown;
const onProgress = (s) => s.kind === "progress" && s.showing && s.shown;
const under = (s, name) => s.phases.find((p) => p.name === name);
const pointer = (type, at) => send("Input.dispatchMouseEvent", { type, x: at.x, y: at.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
/** The state as any other browser is sent it. */
const elsewhere = () => new Promise((resolve, reject) => {
    const other = new WebSocket(`ws://${address}/live`);
    other.onmessage = (m) => { resolve(JSON.parse(m.data)); other.close(); };
    other.onerror = reject;
});
/** From the first page, through the disks and the confirmation, to the job
    begun on `device`: the button that erases is held until it takes. */
async function begin(device) {
    await key("1", "Digit1", 49);
    await eventually(seen, onDisks);
    await click(`#turn .disk[data-value="${device}"]`);
    await eventually(seen, (s) => s.disks.some((d) => d.value === device && d.checked) && s.next === "on");
    await click("#turn .btn.go-on");
    await eventually(seen, onConfirm);
    const at = await js(`(() => { const r = document.querySelector("#turn .destroy").getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
    await pointer("mousePressed", at);
    const committed = await eventually(seen, (s) => s.committed || s.kind === "progress", 4);
    await pointer("mouseReleased", at);
    return committed;
}

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    out.first = await eventually(seen, onFirstPage);
    expect("the first page is showing", out.first.after !== null);

    // Install, the Windows disk, Next, and the button held all the way.
    out.begun = await begin("/dev/nvme0n1");
    expect("a hold that completes begins the installation, and is seen to", out.begun.after !== null && out.begun.after >= 1.2 && out.begun.committed && out.begun.flashed);
    out.arrived = await eventually(seen, onProgress);
    await picture("progress-1-arrived.png");
    expect("and the page the installation runs on takes the confirmation's place",
        out.arrived.after !== null && out.arrived.heading === "Installing" && out.arrived.title === "Installing · Peios Setup");
    expect("it says what is being installed onto and that the machine is to be left to it, in installerd's words",
        out.arrived.lede === "Installing Peios onto Samsung SSD 980 PRO 1TB, 931.5 GiB (/dev/nvme0n1). Leave the machine on and the install medium in place until this finishes.");
    expect("the phases are installerd's, in its order",
        JSON.stringify(out.arrived.phases.map((p) => p.name)) === JSON.stringify(["Partitioning", "Formatting", "Copying the system", "Setting up boot"]));
    expect("nothing on it can be pressed while the job runs, and the keyboard is on the heading", out.arrived.again === null && out.arrived.focused === "the heading");
    expect("the disk is drawn beside them as it is being made",
        out.arrived.disk.there && out.arrived.disk.model === "Samsung SSD 980 PRO 1TB" && out.arrived.disk.under === "/dev/nvme0n1 · 931.5 GiB"
        && out.arrived.disk.drawn === "/dev/nvme0n1 as it is being made: EFI system partition, 512 MiB and Peios, 931.0 GiB."
        && JSON.stringify(out.arrived.disk.parts.map((p) => p.is)) === JSON.stringify(["EFI system partition: 512 MiB · FAT32", "Peios: 931.0 GiB · ext4"]));
    expect("what the job has said of itself can be saved", out.arrived.save === "log.txt");
    out.shared = (await elsewhere()).page;
    expect("everyone looking is on it, with the same disk", out.shared.kind === "progress" && out.shared.job === "install" && out.shared.disk.device === "/dev/nvme0n1" && out.shared.ended === null);

    // The copy, part of the way.
    out.copying = await eventually(seen, (s) => under(s, "Copying the system")?.state === "active" && under(s, "Copying the system").now >= 20, 20);
    await picture("progress-2-copying.png");
    expect("the phases before the one under way are done, and those after are waiting",
        out.copying.after !== null && under(out.copying, "Partitioning").state === "done" && under(out.copying, "Formatting").state === "done"
        && under(out.copying, "Setting up boot").state === "");
    expect("which phase is under way is said, and how far along it is", out.copying.now[0] === "Copying the system" && out.copying.now[1] === "3 of 4" && /^\d+%$/.test(under(out.copying, "Copying the system").says));
    expect("one figure stands for the whole, and is not there yet", Number(out.copying.figure) > 12 && Number(out.copying.figure) < 99 && out.copying.overall === out.copying.figure);
    expect("the disk is laid out and formatted, and the root is filling",
        out.copying.disk.made === 1 && out.copying.disk.fmt === 1 && out.copying.disk.copy > .15 && out.copying.disk.copy < 1 && out.copying.disk.boot === 0
        && out.copying.disk.doing === "copying" && out.copying.disk.parts.every((p) => !p.done));
    expect("what the job says of itself is shown as it says it", out.copying.log.lines >= 5 && out.copying.log.first === "dry run: no bytes will be written to /dev/nvme0n1" && out.copying.log.scrolls === false);
    expect("the status says who is doing it", out.copying.status.startsWith("Connected to installerd"));

    // A browser that arrives part way is shown the job as it stands.
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    out.late = await eventually(seen, onProgress);
    expect("a browser that arrives part way is shown the job as it stands, with everything it has said",
        out.late.after !== null && out.late.heading === "Installing" && under(out.late, "Formatting").state === "done" && out.late.disk.there
        && out.late.log.first === "dry run: no bytes will be written to /dev/nvme0n1");

    // It finishes.
    out.done = await eventually(seen, (s) => s.finished && s.again !== null, 40);
    await sleep(1600);
    out.done = { ...await seen(), after: out.done.after };
    await picture("progress-3-finished.png");
    expect("finished, the page says so in installerd's words: its first sentence the heading, the rest beneath",
        out.done.after !== null && out.done.heading === "Installation complete" && out.done.lede === "Reboot to start Peios."
        && out.done.title === "Installation complete · Peios Setup" && out.done.said === "Installation complete. Reboot to start Peios.");
    expect("every phase is done, the figure has given way, and the disk is whole",
        out.done.phases.every((p) => p.state === "done") && out.done.overall === "100" && out.done.now[0] === "Complete" && out.done.now[1] === "4 of 4"
        && out.done.disk.copy === 1 && out.done.disk.boot === 1 && out.done.disk.parts.every((p) => p.done) && out.done.disk.doing === "");
    expect("the way on is offered quietly, has the keyboard, and is not under the bar along the bottom",
        out.done.again?.says === "Back to the start" && out.done.again.how === "btn quiet" && out.done.focused === "Back to the start" && out.done.again.clear);
    expect("what the job said can now be read through, by a keyboard too", out.done.log.scrolls === true && out.done.log.reachable === "0" && out.done.log.last === "Setting up boot: ok");
    expect("and the status no longer claims a conversation", out.done.status === "Finished" && out.done.statusIs === "" && out.done.heat === 0);
    out.ended = (await elsewhere());
    expect("for everyone looking", out.ended.page.kind === "progress" && out.ended.page.ended.outcome === "complete" && out.ended.seq === 0);

    // What it said, whole, as a file.
    const saved = await fetch(`${site}log.txt`);
    out.saved = { type: saved.headers.get("content-type"), as: saved.headers.get("content-disposition"), lines: (await saved.text()).split("\n") };
    expect("what the job said is served whole, as a file to keep",
        out.saved.type === "text/plain; charset=utf-8" && out.saved.as === 'attachment; filename="peios-setup.log"'
        && out.saved.lines[0] === "dry run: no bytes will be written to /dev/nvme0n1" && out.saved.lines.includes("Copying the system: ok") && out.saved.lines.at(-1) === "");

    // Back to the start: another conversation.
    await click("#turn .nav.ends .btn");
    out.again = await eventually(seen, onFirstPage);
    expect("asked to, the installer opens another conversation, at the first page", out.again.after !== null && out.again.status.startsWith("Connected to installerd") && out.again.heat === 0);
    expect("for everyone looking", (await elsewhere()).page.kind === "mode");
    expect("and keeps nothing of the last job", (await (await fetch(`${site}log.txt`)).text()) === "");

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...chrome.problems];
    expect("the page reports no errors", out.problems.length === 0);

    // An installation that goes wrong part way through the copy.
    installerd.kill();
    installerd = startInstallerd(join(root, "dev/desktop.json"), "--fail-at", "copy");
    await eventually(seen, (s) => onFirstPage(s) && s.status.startsWith("Connected to installerd"), 15);
    await begin("/dev/nvme0n1");
    out.stopped = await eventually(seen, (s) => s.stopped && s.again !== null, 40);
    await sleep(1600);
    out.stopped = { ...await seen(), after: out.stopped.after };
    await picture("progress-4-stopped.png");
    expect("a job that fails stays on its page, which says that it stopped and where",
        out.stopped.after !== null && out.stopped.heading === "Installation stopped" && out.stopped.lede === "It stopped while copying the system."
        && out.stopped.title === "Installation stopped · Peios Setup");
    expect("and what installerd said of it, as it said it",
        out.stopped.installerdSaid === "dry run: stopped part way through copying the system, as it was told to"
        && out.stopped.said === "Installation stopped. It stopped while copying the system. dry run: stopped part way through copying the system, as it was told to");
    expect("the phases say which were done, which it stopped in and how far in, and which never started",
        JSON.stringify(out.stopped.phases.map((p) => [p.state, p.says])) === JSON.stringify([["done", "100%"], ["done", "100%"], ["failed", "Stopped at 52%"], ["skipped", "Not started"]]));
    expect("one figure for the whole is not shown for a job that stopped", out.stopped.overallShown === false);
    expect("the disk is left as far as it got, and nothing on it is said to be finished",
        out.stopped.disk.there && out.stopped.disk.copy > .5 && out.stopped.disk.copy < .54 && out.stopped.disk.parts.every((p) => !p.done) && out.stopped.disk.doing === "");
    expect("what the job said is there to read and to save, the last of it in view",
        out.stopped.log.scrolls === true && out.stopped.log.atEnd && out.stopped.log.last === "  dry run: told to fail at phase.copy" && out.stopped.save === "log.txt");
    expect("starting again is offered plainly, has the keyboard, and is not under the bar along the bottom",
        out.stopped.again?.says === "Start again" && out.stopped.again.how === "btn go-on" && out.stopped.focused === "Start again" && out.stopped.again.clear);
    expect("the status says it stopped, and the page is warm", out.stopped.status === "Stopped" && out.stopped.statusIs === "bad" && out.stopped.heat === .3);

    // A narrow screen.
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(400);
    out.phone = await seen();
    await picture("progress-5-phone.png");
    expect("a phone holds the page without scrolling sideways, the disk before the phases", out.phone.wide === false && out.phone.diskBeforePhases === true);
    await send("Emulation.clearDeviceMetricsOverride");

    await click("#turn .nav.ends .btn");
    out.restarted = await eventually(seen, onFirstPage);
    expect("starting again goes back to the first page, and the page cools", out.restarted.after !== null && out.restarted.heat === 0 && out.restarted.statusIs === "");
    out.problemsAfter = chrome.problems.filter((p) => !/WebSocket|ERR_CONNECTION_REFUSED/.test(p));
    expect("the page reports no errors", out.problemsAfter.length === 0);

    // A disk whose name is markup: it is a name, and shown as written. And
    // a job that goes wrong at once, which is over by the time its page has
    // taken the confirmation's place: the page arrives saying so.
    installerd.kill();
    installerd = startInstallerd(hostile, "--fail-at", "partition");
    await eventually(seen, (s) => onFirstPage(s) && s.status.startsWith("Connected to installerd"), 15);
    await begin("/dev/sda");
    out.hostile = await eventually(seen, (s) => s.stopped && s.again !== null && s.showing && s.shown, 30);
    expect("what a disk calls itself is shown as text, never taken as markup",
        out.hostile.after !== null && out.hostile.markup === 0 && out.hostile.disk.model === '<img src=x onerror="document.title=1">');
    expect("a job that stopped before its page arrived is drawn as it stands, the last of what it said in view",
        out.hostile.title === "Installation stopped · Peios Setup" && out.hostile.lede === "It stopped while partitioning."
        && JSON.stringify(out.hostile.phases.map((p) => p.state)) === JSON.stringify(["failed", "skipped", "skipped", "skipped"])
        && out.hostile.log.atEnd && out.hostile.again.says === "Start again" && out.hostile.focused === "Start again");
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    installerGxwi.kill();
    installerd.kill();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
