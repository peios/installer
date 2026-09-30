// An installation, really, on a real machine: the dev VM, onto the stick
// dev/boot.sh gives it for the purpose.
//
//     node dev/browser/install.mjs [URL] [--reboot]
//
// URL is where the installer is: http://127.0.0.1:7780/ by default, which is
// the dev VM with the installer set as GXWI's overlay (dev/overlay.sh on).
//
// With --reboot, an installation that finishes is restarted into: Reboot
// now is pressed, and the page is followed down and back until it says what
// answered, which on the dev VM should be the Peios just installed. The VM is
// then running that, and needs dev/boot.sh again to be an installer.
//
// THIS ERASES A DISK. It chooses the removable disk on the USB bus, which in
// the dev VM is target/disks/blank.img and holds nothing anyone wants, and
// stops without pressing anything if there is not exactly one such disk.
//
// progress.mjs checks the page against an installerd that pretends. This
// checks what pretending cannot: that holding the button really begins the
// job, that the page follows a job that takes as long as it takes and says
// what real tools say, and that it is left on an ending that says how the
// job ended. Whether the job finishes is the image's business and not this
// page's, so either ending is accepted here, and which it was is printed,
// with the last of what the job said.
//
// It expects to find the conversation on its first page, and leaves it
// there, having asked to start again.
import { browser, eventually, sleep } from "./chrome.mjs";

const site = process.argv.slice(2).find((a) => !a.startsWith("--")) ?? "http://127.0.0.1:7780/";
const reboot = process.argv.includes("--reboot");
const chrome = await browser(9376);
const { send, js, picture, key, click } = chrome;

/** What the page shows once the machine is restarting. */
const restartSeen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const lockup = document.getElementById("intro-lockup");
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        ledes: [...turn.querySelectorAll(".lede")].map((p) => p.textContent),
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        rebooting: document.getElementById("stage").classList.contains("rebooting"),
        mark: ["formed", "drawn", "sleeping", "popped", "word-in", "gone"].filter((c) => lockup.classList.contains(c)),
        ticker: [...document.querySelectorAll("#ticker li")].map((li) => li.textContent),
        status: document.getElementById("status-text").textContent,
    };
})()`);

const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const build = turn.querySelector(".build");
    const log = turn.querySelector(".log");
    const again = turn.querySelector('.nav.ends [data-way="again"]');
    const reboot = turn.querySelector('.nav.ends [data-way="reboot"]:not([hidden])');
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        actions: turn.querySelectorAll(".act").length,
        disks: [...turn.querySelectorAll(".disk")].map((d) => ({
            value: d.dataset.value, where: d.querySelector(".where").textContent, checked: d.getAttribute("aria-checked") === "true",
            disabled: d.getAttribute("aria-disabled") === "true",
        })),
        next: turn.querySelector(".btn.go-on")?.getAttribute("aria-disabled") === "true" ? "off" : "on",
        summary: turn.querySelector("#confirm-summary")?.textContent ?? null,
        committed: turn.classList.contains("committed"),
        lede: turn.querySelector(".lede:not([hidden])")?.textContent ?? null,
        installerdSaid: turn.querySelector(".said:not([hidden]) code")?.textContent ?? null,
        figure: turn.querySelector(".pct .num")?.textContent ?? null,
        now: [...turn.querySelectorAll(".overall-now > *")].map((n) => n.textContent),
        phases: [...turn.querySelectorAll(".phase")].map((li) => ({
            name: li.querySelector(".pname").textContent,
            state: li.className.replace("phase", "").trim(),
            says: li.querySelector(".pval").textContent,
        })),
        finished: turn.classList.contains("finished"),
        stopped: turn.classList.contains("stopped"),
        disk: {
            there: !!build && !build.hidden,
            under: build?.querySelector(".plan-title span")?.textContent ?? null,
            parts: [...(build?.querySelectorAll(".part") ?? [])].map((li) => ({
                is: li.querySelector("b").textContent + ": " + li.querySelector("span").textContent, done: li.classList.contains("done"),
            })),
        },
        lines: log ? log.children.length : null,
        tail: log ? [...log.children].slice(-12).map((li) => li.textContent) : [],
        again: again && !turn.querySelector(".nav.ends").hidden ? again.textContent : null,
        reboot: reboot && !turn.querySelector(".nav.ends").hidden ? reboot.textContent : null,
        status: document.getElementById("status-text").textContent,
        title: document.title,
    };
})()`);
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing;

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    out.first = await eventually(seen, onFirstPage, 20);
    expect("the first page is showing", out.first.after !== null);

    await key("1", "Digit1", 49);
    out.disks = await eventually(seen, (s) => s.heading === "Choose a disk" && s.showing, 60);
    const sticks = (out.disks.disks ?? []).filter((d) => !d.disabled && d.where.includes(" · USB") && d.where.includes("removable"));
    out.onto = sticks.map((d) => d.value);
    expect("there is exactly one removable USB disk to install onto", sticks.length === 1);
    if (sticks.length !== 1) throw new Error("no one disk that is safe to erase: nothing was pressed");
    const stick = sticks[0].value;

    await click(`#turn .disk[data-value="${stick}"]`);
    await eventually(seen, (s) => s.disks.some((d) => d.value === stick && d.checked) && s.next === "on");
    await click("#turn .btn.go-on");
    out.confirm = await eventually(seen, (s) => s.heading === "Ready to install" && s.showing, 60);
    expect("the confirmation is about the stick", out.confirm.after !== null && out.confirm.summary?.includes(`(${stick}). The whole disk will be erased`));
    if (!out.confirm.summary?.includes(`(${stick})`)) throw new Error("the confirmation is not about the stick: nothing was held");

    // The button that erases, held until it takes.
    await sleep(900);
    const at = await js(`(() => { const r = document.querySelector("#turn .destroy").getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", buttons: 1, clickCount: 1 });
    out.held = await eventually(seen, (s) => s.committed || s.kind === "progress", 5);
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", buttons: 0, clickCount: 1 });
    const began = Date.now();
    out.running = await eventually(seen, (s) => s.kind === "progress" && s.showing, 20);
    await picture("install-1-running.png");
    // A job that goes wrong at once is over before its page has arrived, and
    // the page then arrives saying so.
    expect("holding the button begins the installation, and its page arrives",
        out.running.after !== null && out.running.disk.there && out.running.disk.under?.startsWith(`${stick} · `)
        && (out.running.stopped || out.running.finished || out.running.lede?.includes(`(${stick}).`)));
    expect("the phases are an installation's", JSON.stringify(out.running.phases.map((p) => p.name)) === JSON.stringify(["Partitioning", "Formatting", "Copying the system", "Setting up boot"]));

    // As long as it takes. What the page shows along the way is kept, once
    // for each phase it is seen under way in.
    out.along = [];
    out.end = await eventually(async () => {
        const s = await seen();
        if (s.now?.[0] && !out.along.some((a) => a.now === s.now[0])) out.along.push({ now: s.now[0], figure: s.figure, at: Math.round((Date.now() - began) / 1000) });
        return s;
    }, (s) => (s.finished || s.stopped) && s.again !== null, 1800);
    out.tookSeconds = Math.round((Date.now() - began) / 1000);
    // The bars glide to where the job left them, and a phase is not shown
    // done until its bar has got there: the page is read once it is still.
    await sleep(1600);
    out.end = { ...await seen(), after: out.end.after };
    await picture("install-2-ended.png");
    expect("the job comes to an end, and the page stays to say how", out.end.after !== null && out.end.kind === "progress");
    out.outcome = out.end.finished ? "complete" : out.end.stopped ? "failed" : "neither";
    if (out.end.finished) {
        expect("finished, every phase is done and both partitions are made", out.end.phases.every((p) => p.state === "done") && out.end.disk.parts.every((p) => p.done));
        expect("and it says so in installerd's words", out.end.heading === "Installation complete" && out.end.again === "Back to the start" && out.end.status === "Finished");
        expect("and offers to restart the machine", out.end.reboot === "Reboot now");
    } else if (out.end.stopped) {
        expect("stopped, it says what installerd said, and where it stopped",
            out.end.heading === "Installation stopped" && !!out.end.installerdSaid && out.end.phases.some((p) => p.state === "failed" || p.state === "skipped"));
        expect("and offers to start again", out.end.again === "Start again" && out.end.status === "Stopped");
    }
    const saved = await fetch(`${site}log.txt`);
    const lines = (await saved.text()).split("\n");
    out.saved = { status: saved.status, lines: lines.length - 1, first: lines[0] };
    expect("everything the job said can be saved", saved.status === 200 && lines.length > 5 && lines[0].startsWith("$ part create"));

    if (reboot && out.end.finished) {
        // Restarted into what was installed, which is Peios' own GXWI on the
        // stick, with first-boot setup on the machine's screen.
        expect("the restart is offered", out.end.reboot === "Reboot now");
        await click('#turn .nav.ends [data-way="reboot"]');
        out.going = await eventually(restartSeen, (r) => r.rebooting && r.ticker[0] === "reboot asked of installerd", 30);
        expect("pressed, installerd takes it and the page goes down with the machine", out.going.after !== null);
        out.away = await eventually(restartSeen, (r) => r.mark.includes("sleeping"), 20);
        out.back = await eventually(restartSeen, (r) => ["running", "again", "lost"].includes(r.kind) && r.showing, 300);
        await sleep(1600);
        out.back = { ...await restartSeen(), after: out.back.after };
        await picture("install-3-restarted.png");
        expect("the machine comes back as Peios, and the page says so",
            out.back.after !== null && out.back.kind === "running" && out.back.heading === "Peios is running"
            && out.back.ledes[0]?.startsWith("The machine restarted from ") && out.back.ledes[0].endsWith(`(${stick}).`));
        const answering = await fetch(site);
        out.answering = { status: answering.status, installer: (await fetch(`${site}hello`)).headers.get("content-type") };
        expect("and what answers at the address is not the installer", !out.answering.installer?.startsWith("application/json"));
    } else {
        await click('#turn .nav.ends [data-way="again"]');
        out.left = await eventually(seen, onFirstPage, 30);
        expect("asked to start again, it is back on the first page", out.left.after !== null);
    }

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = chrome.problems.filter((p) => !reboot || !/WebSocket|ERR_CONNECTION_REFUSED|ERR_EMPTY_RESPONSE|ERR_CONNECTION_RESET|401|Failed to load resource/.test(p));
    expect("the page reports no errors", out.problems.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    process.exit(failed.length ? 1 : 0);
}
