// The confirmation, on the host.
//
//     node dev/browser/confirm.mjs
//
// It starts its own installerd (--dry-run) and its own installer-gxwi, so it
// needs no VM and nothing else running. installerd is told to be the machine
// dev/desktop.json describes, and then one whose disk carries markup where
// its names should be.
//
// It goes to the confirmation the way a person does, and looks at what the
// page says of the disk, that the button is held and not pressed, that
// letting go undoes it, that Back returns to the disks with the disk still
// chosen, and that something which cannot hold can press twice, which begins
// the installation (a pretended one). What a hold that completes leads to is
// progress.mjs's to look at.
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
const address = "127.0.0.1:7793";
const site = `http://${address}/`;
// installer-gxwi links libpeios, which is found beside the sibling checkout.
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const startInstallerd = (machine) => spawn(join(installer, "installerd"),
    ["--dry-run", "--inventory", machine, "--socket", socket], { stdio: "ignore" });

// A disk made by someone who hoped its names would be taken for markup.
const hostile = join(run, "hostile.json");
const desktop = JSON.parse(readFileSync(join(root, "dev/desktop.json"), "utf8"));
writeFileSync(hostile, JSON.stringify({
    disks: [{
        ...desktop.disks[2],
        model: '<img src=x onerror="document.title=1">',
        partitions: [{ ...desktop.disks[2].partitions[0], label: "<b id=planted>Media</b>", holds: "<script>document.title=2</script>" }],
    }],
}));

let installerd = startInstallerd(join(root, "dev/desktop.json"));
await sleep(400);
const installerGxwi = spawn(join(root, "target/debug/installer-gxwi"), ["--socket", socket, "--listen", address], { stdio: "ignore", env });
const chrome = await browser(9374);
const { send, js, picture, key, click } = chrome;

/** What the page is showing. */
const seen = () => js(`(() => {
    const turn = document.getElementById("turn"), stage = document.getElementById("stage");
    const number = (of, name) => Number(getComputedStyle(of).getPropertyValue(name) || 0);
    const button = turn.querySelector(".destroy");
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        shown: [...turn.querySelectorAll(".rise")].every((r) => getComputedStyle(r).opacity === "1"),
        actions: turn.querySelectorAll(".act").length,
        summary: turn.querySelector("#confirm-summary")?.textContent ?? null,
        back: turn.querySelector(".btn.quiet")?.textContent ?? null,
        begin: button?.textContent ?? null,
        described: button?.getAttribute("aria-describedby") ?? null,
        hint: turn.querySelector(".hold-hint")?.textContent ?? null,
        k: number(turn, "--k"),
        heat: number(stage, "--heat"),
        holding: turn.classList.contains("holding"),
        committed: turn.classList.contains("committed"),
        fill: button ? Math.round(button.querySelector(".fill").getBoundingClientRect().width / button.getBoundingClientRect().width * 100) : null,
        disk: {
            there: !!turn.querySelector(".erased:not([hidden])"),
            model: turn.querySelector(".erased .plan-title b")?.textContent ?? null,
            under: turn.querySelector(".erased .plan-title span")?.textContent ?? null,
            drawn: turn.querySelector(".erased .layout")?.getAttribute("aria-label") ?? null,
            going: turn.querySelectorAll(".erased .layout .old .ls.doomed").length,
            becomes: turn.querySelectorAll(".erased .layout .new .ls").length,
            // How much of what it becomes can be seen, in percent of the bar.
            wiped: (() => { const n = turn.querySelector(".erased .layout .new"); if (!n) return null;
                const m = /inset\\(0(?:px)? ([\\d.]+)%/.exec(getComputedStyle(n).clipPath); return m ? Math.round(100 - Number(m[1])) : 100; })(),
            legend: [...turn.querySelectorAll(".erased .legend li")].map((li) => li.querySelector("b").textContent + ": " + li.querySelector("span").textContent),
        },
        disks: [...turn.querySelectorAll(".disk")].map((d) => ({ value: d.dataset.value, checked: d.getAttribute("aria-checked") === "true" })),
        next: turn.querySelector(".btn.go-on")?.getAttribute("aria-disabled") === "true" ? "off" : "on",
        toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
        flashed: document.getElementById("flash").classList.contains("go"),
        focused: document.activeElement?.closest?.(".disk")?.dataset.value ?? document.activeElement?.textContent?.trim() ?? document.activeElement?.tagName,
        title: document.title,
        said: document.getElementById("say").textContent,
        wide: document.documentElement.scrollWidth > innerWidth,
        diskBeforeNav: (() => { const p = turn.querySelector(".erased"), n = turn.querySelector(".nav"); return p && n ? p.getBoundingClientRect().top < n.getBoundingClientRect().top : null; })(),
        markup: turn.querySelectorAll("img, script, #planted").length,
    };
})()`);
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing;
const onDisks = (s) => s.heading === "Choose a disk" && s.showing && s.disks.length > 0;
const onConfirm = (s) => s.kind === "confirm" && s.showing && s.shown;
const disk = (value) => `#turn .disk[data-value="${value}"]`;
/** Where the button that erases is, for a pointer to go to. */
const button = () => js(`(() => { const r = document.querySelector("#turn .destroy").getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
const pointer = (type, at) => send("Input.dispatchMouseEvent", { type, x: at.x, y: at.y, button: "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 });
/** The state as any other browser is sent it. */
const elsewhere = () => new Promise((resolve, reject) => {
    const other = new WebSocket(`ws://${address}/live`);
    other.onmessage = (m) => { resolve(JSON.parse(m.data)); other.close(); };
    other.onerror = reject;
});
/** From the first page to the confirmation for `device`. */
async function toConfirm(device) {
    await key("1", "Digit1", 49);
    await eventually(seen, onDisks);
    await click(disk(device));
    await eventually(seen, (s) => s.disks.some((d) => d.value === device && d.checked) && s.next === "on");
    await click("#turn .btn.go-on");
    return eventually(seen, onConfirm);
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

    // Install, the Windows disk, Next.
    out.confirm = await toConfirm("/dev/nvme0n1");
    await picture("confirm-1-arrived.png");
    expect("Next on the disk chosen for an install goes on to the confirmation",
        out.confirm.after !== null && out.confirm.heading === "Ready to install" && out.confirm.title === "Ready to install · Peios Setup" && out.confirm.said === "Ready to install");
    expect("it says which disk, that all of it goes, and what would be missed, in installerd's words",
        out.confirm.summary === "Peios will be installed onto Samsung SSD 980 PRO 1TB, 931.5 GiB (/dev/nvme0n1). The whole disk will be erased: partitioned, formatted, and overwritten, including Windows. This cannot be undone.");
    expect("the disk is drawn beside it: what it is, what is on it going, and what it becomes",
        out.confirm.disk.there && out.confirm.disk.model === "Samsung SSD 980 PRO 1TB" && out.confirm.disk.under === "/dev/nvme0n1 · 931.5 GiB"
        && out.confirm.disk.going === 4 && out.confirm.disk.becomes === 2
        && JSON.stringify(out.confirm.disk.legend) === JSON.stringify(["EFI system partition: 512 MiB · FAT32", "Peios: 931.0 GiB · ext4"]));
    expect("and the picture is put into words too",
        out.confirm.disk.drawn?.startsWith("/dev/nvme0n1 as it is: EFI system partition, 260 MiB") && out.confirm.disk.drawn.includes("As it will be: EFI system partition, 512 MiB and Peios, 931.0 GiB."));
    expect("nothing of what it becomes is shown until the button is held", out.confirm.disk.wiped === 0 && out.confirm.k === 0 && out.confirm.fill === 0);
    expect("the button says what it does and how it is pressed, and is described by both",
        out.confirm.begin === "Erase disk and install" && out.confirm.hint === "Press and hold" && out.confirm.described === "hold-hint confirm-summary");
    expect("the keyboard starts on the way back, not on the button that erases", out.confirm.focused === "Back");
    expect("the page warms as it arrives", out.confirm.heat > 0.3 && out.confirm.heat < 0.4);
    out.shared = (await elsewhere()).page;
    expect("everyone looking is on the confirmation, with the same disk",
        out.shared.kind === "confirm" && out.shared.disk.device === "/dev/nvme0n1" && out.shared.begin.destructive === true);

    // Held part of the way, and let go.
    const at = await button();
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
    await pointer("mousePressed", at);
    await sleep(700);
    out.part = await seen();
    await picture("confirm-2-holding.png");
    expect("holding fills the button and wipes in what the disk becomes, as far as the hold has got",
        out.part.holding && out.part.k > 0.25 && out.part.k < 0.8 && out.part.fill > 20 && out.part.disk.wiped > 20 && out.part.disk.wiped < 85
        && out.part.hint === "Keep holding…" && out.part.heat > out.confirm.heat);
    await pointer("mouseReleased", at);
    out.let = await eventually(seen, (s) => s.k === 0 && !s.holding, 3);
    expect("letting go undoes it, and nothing was begun",
        out.let.after !== null && out.let.hint === "Press and hold" && out.let.disk.wiped === 0 && !out.let.committed && out.let.toast === null
        && (await elsewhere()).waiting === null);

    // A key held is a hold too, and a tap of it is not.
    await js(`document.querySelector("#turn .destroy").focus()`);
    await key(" ", "Space", 32);
    await sleep(200);
    out.tapped = await eventually(seen, (s) => s.k === 0, 3);
    expect("a tap of the space bar begins nothing, and is not taken for a press that cannot be held",
        out.tapped.after !== null && !out.tapped.committed && out.tapped.hint === "Press and hold");
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await sleep(600);
    out.keyed = await seen();
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    expect("Enter held down on the button holds it", out.keyed.holding && out.keyed.k > 0.2);
    out.unkeyed = await eventually(seen, (s) => s.k === 0, 3);
    expect("and let up before it is full, begins nothing", out.unkeyed.after !== null && !out.unkeyed.committed && (await elsewhere()).waiting === null);

    // What cannot hold presses twice: a screen reader's press is a click no
    // pointer and no key made. One press alone asks for another, and is
    // forgotten if none comes.
    await click("#turn .destroy");
    out.asked = await eventually(seen, (s) => s.hint === "Press again to confirm" && s.said !== "", 3);
    expect("a press that cannot be held is asked for again, out loud",
        out.asked.after !== null && out.asked.k === 0 && !out.asked.committed && out.asked.said === "Erase disk and install: press again to confirm. This cannot be undone.");
    out.lapsed = await eventually(seen, (s) => s.hint === "Press and hold", 8);
    expect("and one press alone comes to nothing", out.lapsed.after !== null && out.lapsed.after > 3 && !out.lapsed.committed && (await elsewhere()).page.kind === "confirm");

    // A narrow screen.
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(400);
    out.phone = await seen();
    await picture("confirm-3-phone.png");
    expect("a phone holds the page without scrolling sideways, the disk before Back and the button", out.phone.wide === false && out.phone.diskBeforeNav === true);
    await send("Emulation.clearDeviceMetricsOverride");

    // Back: the disks again, with the disk still chosen.
    await click("#turn .btn.quiet");
    out.back = await eventually(seen, (s) => onDisks(s) && s.disks.some((d) => d.checked));
    expect("Back returns to the disks with the disk still chosen, and the keyboard on it",
        out.back.after !== null && out.back.disks.find((d) => d.checked)?.value === "/dev/nvme0n1" && out.back.next === "on" && out.back.focused === "/dev/nvme0n1");
    expect("and the page cools as it goes", out.back.heat === 0);
    expect("for everyone looking", (await elsewhere()).page.chosen === "/dev/nvme0n1");

    // A disk with files that are nobody's system, and one with nothing on it
    // that a person would miss.
    await click(disk("/dev/sda"));
    await eventually(seen, (s) => s.disks.some((d) => d.value === "/dev/sda" && d.checked));
    await click("#turn .btn.go-on");
    out.media = await eventually(seen, onConfirm);
    expect("what is not a system is named by its label and what is in it",
        out.media.summary?.includes("overwritten, including “Media” (640.0 GiB in use). This cannot be undone.") && out.media.disk.going === 1);
    await click("#turn .btn.quiet");
    await eventually(seen, onDisks);
    await click("#turn .btn.quiet");
    await eventually(seen, onFirstPage);

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...chrome.problems];
    expect("the page reports no errors", out.problems.length === 0);

    // Pressed twice by something that cannot hold, which is taken as meant:
    // the installation (a pretended one) is begun, for everyone looking.
    await toConfirm("/dev/sda");
    await click("#turn .destroy");
    await eventually(seen, (s) => s.hint === "Press again to confirm", 3);
    await click("#turn .destroy");
    out.twice = await eventually(seen, (s) => s.committed || s.kind === "progress", 3);
    expect("a second press soon after the first is taken as meant, and begins it", out.twice.after !== null && (out.twice.kind === "progress" || out.twice.hint === "Starting…"));
    out.begun = await eventually(seen, (s) => s.kind === "progress" && s.showing, 10);
    expect("and the installation's page takes the confirmation's place, the page no longer warm",
        out.begun.after !== null && out.begun.heading === "Installing" && out.begun.heat === 0 && (await elsewhere()).page.kind === "progress");
    // It is seen through, and the conversation started again, for what
    // follows here.
    await eventually(seen, (s) => s.kind === "progress" && s.heading === "Installation complete", 60);
    await sleep(1200);
    await click("#turn .nav.ends .btn");
    await eventually(seen, onFirstPage, 15);

    // A disk whose names are markup: they are names, and shown as written.
    installerd.kill();
    installerd = startInstallerd(hostile);
    await eventually(seen, onFirstPage, 15);
    out.hostile = await toConfirm("/dev/sda");
    expect("what a disk calls itself is shown as text, never taken as markup",
        out.hostile.after !== null && out.hostile.markup === 0 && out.hostile.title === "Ready to install · Peios Setup"
        && out.hostile.disk.model === '<img src=x onerror="document.title=1">' && out.hostile.summary.includes("<script>document.title=2</script>"));
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    installerGxwi.kill();
    installerd.kill();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
