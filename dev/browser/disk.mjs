// The disk page, on the host.
//
//     node dev/browser/disk.mjs
//
// It starts its own installerd (--dry-run) and its own installer-gxwi, so it
// needs no VM and nothing else running. installerd is told to be the machine
// dev/desktop.json describes -- Windows on one disk, Peios on another -- and
// then two others made here: one with no disk to choose, and one whose disk
// carries markup where its names should be.
//
// It goes to the disk page each of the three ways there is, and looks at what
// the page says of a disk, that a disk chosen here is chosen for everyone
// looking, that the keyboard works, and that a narrow screen holds it.
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
const address = "127.0.0.1:7792";
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
const chrome = await browser(9372);
const { send, js, picture, key, click } = chrome;

/** What the page is showing. */
const seen = () => js(`({
    heading: document.querySelector("#turn h1")?.textContent ?? null,
    lede: document.querySelector("#turn .lede")?.textContent ?? null,
    showing: document.getElementById("turn").classList.contains("in") && document.getElementById("page").classList.contains("live"),
    actions: document.querySelectorAll("#turn .act").length,
    disks: [...document.querySelectorAll("#turn .disk")].map((d) => ({
        value: d.dataset.value, model: d.querySelector(".model").textContent, where: d.querySelector(".where").textContent,
        size: d.querySelector(".size").textContent, chip: d.querySelector(".chip")?.textContent ?? null,
        checked: d.getAttribute("aria-checked") === "true", disabled: d.getAttribute("aria-disabled") === "true",
        stop: d.tabIndex === 0, shown: getComputedStyle(d.closest(".rise")).opacity === "1",
    })),
    scanning: document.querySelector("#turn .disks")?.classList.contains("scanning") ?? false,
    empty: document.querySelector("#turn .no-disks")?.textContent ?? null,
    trouble: document.querySelector("#turn .trouble:not([hidden])")?.textContent ?? null,
    help: document.querySelector("#turn .help.on")?.textContent ?? null,
    custom: (() => { const c = document.querySelectorAll("#turn .aux .link")[1]; return !c || c.hidden ? null : { name: c.textContent, disabled: c.getAttribute("aria-disabled") === "true" }; })(),
    next: document.querySelector("#turn .btn.go-on")?.getAttribute("aria-disabled") === "true" ? "off" : "on",
    label: document.querySelector("#turn .plan-label")?.textContent ?? null,
    plan: {
        title: document.querySelector("#turn .plan-title b")?.textContent ?? null,
        under: document.querySelector("#turn .plan-title span")?.textContent ?? null,
        empty: document.querySelector("#turn .plan-empty")?.textContent ?? null,
        was: document.querySelectorAll("#turn .layout .old .ls").length,
        doomed: document.querySelectorAll("#turn .layout .old .ls.doomed").length,
        becomes: [...document.querySelectorAll("#turn .layout .new .ls")].map((s) => Math.round(s.getBoundingClientRect().width)),
        wiped: document.querySelector("#turn .layout")?.classList.contains("wiped") ?? false,
        drawn: document.querySelector("#turn .layout")?.getAttribute("aria-label") ?? null,
        legend: [...document.querySelectorAll("#turn .legend li")].map((li) => li.querySelector("b").textContent + ": " + li.querySelector("span").textContent),
        erase: document.querySelector("#turn .erase")?.textContent ?? null,
        notes: [...document.querySelectorAll("#turn .plan .note")].map((n) => n.textContent),
        versions: [...document.querySelectorAll("#turn .rel-v")].map((v) => v.textContent),
    },
    toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
    focused: document.activeElement?.closest?.(".disk")?.dataset.value ?? document.activeElement?.closest?.(".act")?.querySelector(".name").textContent ?? document.activeElement?.tagName,
    title: document.title,
    said: document.getElementById("say").textContent,
    wide: document.documentElement.scrollWidth > innerWidth,
    planBeforeNav: (() => { const p = document.querySelector("#turn .plan"), n = document.querySelector("#turn .nav"); return p && n ? p.getBoundingClientRect().top < n.getBoundingClientRect().top : null; })(),
    markup: document.querySelectorAll("#turn img, #turn script, #planted").length,
})`);
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing;
const onDisks = (heading) => (s) => s.heading === heading && s.showing && s.disks.every((d) => d.shown);
const chosen = (value) => (s) => s.disks.some((d) => d.value === value && d.checked) && s.plan.title !== null;
const disk = (value) => `#turn .disk[data-value="${value}"]`;
/** The state as any other browser is sent it. */
const elsewhere = () => new Promise((resolve, reject) => {
    const other = new WebSocket(`ws://${address}/live`);
    other.onmessage = (m) => { resolve(JSON.parse(m.data)); other.close(); };
    other.onerror = reject;
});

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    out.first = await eventually(seen, onFirstPage);
    expect("the first page is showing", out.first.after !== null);

    // Install: the disks, and nothing chosen.
    await key("1", "Digit1", 49);
    out.install = await eventually(seen, onDisks("Choose a disk"));
    await picture("disk-1-install.png");
    expect("choosing Install goes on to the disks", out.install.after !== null && out.install.title === "Choose a disk · Peios Setup" && out.install.said === "Choose a disk");
    expect("every disk installerd lists is there, as it words them",
        JSON.stringify(out.install.disks.map((d) => [d.value, d.model, d.size])) === JSON.stringify([
            ["/dev/mmcblk0", "SD64G", "59.5 GiB"], ["/dev/nvme0n1", "Samsung SSD 980 PRO 1TB", "931.5 GiB"],
            ["/dev/sda", "WDC WD20EZAZ-00GGJB0", "1.8 TiB"], ["/dev/sdb", "SanDisk Ultra Fit", "28.6 GiB"],
            ["/dev/sdc", "CT500MX500SSD1", "465.8 GiB"]]));
    expect("a disk says where it is and what is on it",
        out.install.disks[1].where === "/dev/nvme0n1 · NVMe · Windows" && out.install.disks[0].where === "/dev/mmcblk0 · SD/MMC · 1 partitionremovable"
        && out.install.disks[4].where.startsWith("/dev/sdc · SATA · Peios 2026.8-7"));
    expect("the boot medium is listed, marked, and cannot be chosen",
        out.install.disks[3].disabled && out.install.disks[3].chip === "boot medium" && out.install.disks[3].where === "/dev/sdb · USBboot medium");
    expect("nothing is chosen, so there is nothing to go on with", out.install.disks.every((d) => !d.checked) && out.install.next === "off");
    expect("the keyboard starts on the first disk that can be chosen", out.install.focused === "/dev/mmcblk0" && out.install.disks[0].stop);
    expect("the panel waits for a disk", out.install.label === "What the disk becomes" && out.install.plan.empty === "Choose a disk to see how it will be laid out.");
    expect("laying it out by hand is offered greyed", out.install.custom?.disabled === true);

    // Going on with nothing chosen says why not.
    await click("#turn .btn.go-on");
    out.nothing = await eventually(seen, (s) => s.help !== null, 3);
    expect("Next with no disk chosen says to choose one", out.nothing.help === "Choose a disk first.");

    // Choosing a disk.
    await click(disk("/dev/nvme0n1"));
    out.windows = await eventually(seen, chosen("/dev/nvme0n1"));
    expect("a disk clicked is chosen", out.windows.after !== null && out.windows.disks.filter((d) => d.checked).length === 1 && out.windows.next === "on");
    expect("the panel names it", out.windows.plan.title === "Samsung SSD 980 PRO 1TB" && out.windows.plan.under === "/dev/nvme0n1 · 931.5 GiB");
    expect("and says what an install makes of it",
        JSON.stringify(out.windows.plan.legend) === JSON.stringify(["EFI system partition: 512 MiB · FAT32", "Peios: 931.0 GiB · ext4"]));
    expect("and what goes to make room",
        out.windows.plan.erase === "Everything on /dev/nvme0n1 will be erased, including Windows." && out.windows.plan.doomed === 4);
    await sleep(1800);
    out.wiped = await seen();
    await picture("disk-2-chosen.png");
    expect("what it becomes is wiped in over what it was, the small partition still to be seen",
        out.wiped.plan.wiped && out.wiped.plan.becomes.length === 2 && out.wiped.plan.becomes[0] >= 10 && out.wiped.plan.becomes[1] > out.wiped.plan.becomes[0]);
    expect("and the picture is put into words too", out.wiped.plan.drawn?.startsWith("/dev/nvme0n1 as it will be: EFI system partition, 512 MiB and Peios, 931.0 GiB"));
    out.shared = (await elsewhere()).page;
    expect("the disk chosen here is chosen for everyone looking", out.shared.kind === "disk" && out.shared.chosen === "/dev/nvme0n1");

    // The keyboard: an arrow moves and chooses, as in any group of choices.
    await js(`document.querySelector(${JSON.stringify(disk("/dev/nvme0n1"))}).focus()`);
    await key("ArrowDown", "ArrowDown", 40);
    out.arrowed = await eventually(seen, chosen("/dev/sda"));
    expect("an arrow moves to the next disk and chooses it", out.arrowed.after !== null && out.arrowed.focused === "/dev/sda");
    expect("what is not a system is named by its label and what is in it",
        out.arrowed.plan.erase === "Everything on /dev/sda will be erased, including “Media” (640.0 GiB in use).");
    await key("ArrowDown", "ArrowDown", 40);
    out.skipped = await eventually(seen, chosen("/dev/sdc"));
    expect("and passes over the boot medium", out.skipped.after !== null && out.skipped.focused === "/dev/sdc");

    // Next leads to a page that is not drawn yet, so it says so and stays.
    await key("Enter", "Enter", 13);
    out.held = await eventually(seen, (s) => s.toast !== null, 3);
    expect("Enter on the chosen disk goes on, and the step not drawn yet is said to be", out.held.toast === "The step after this one is not drawn yet." && out.held.heading === "Choose a disk");
    expect("and nobody else is moved on", (await elsewhere()).page.kind === "disk");

    // Rescan: installerd looks again, and the page shows it looking.
    await click("#turn .aux .link");
    out.scanning = await eventually(seen, (s) => s.scanning, 3);
    out.scanned = await eventually(seen, (s) => !s.scanning, 5);
    expect("a rescan is shown while it is under way, and ends", out.scanning.after !== null && out.scanned.after !== null);
    expect("and leaves the disk that is still there chosen", out.scanned.disks.length === 5 && out.scanned.disks[4].checked && out.scanned.focused !== "BODY");

    // A narrow screen.
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(400);
    out.phone = await seen();
    await picture("disk-3-phone.png");
    expect("a phone holds the page without scrolling sideways, the panel before Back and Next", out.phone.wide === false && out.phone.planBeforeNav === true);
    await send("Emulation.clearDeviceMetricsOverride");

    // Back, and the way it came.
    await click("#turn .btn.quiet");
    out.back = await eventually(seen, onFirstPage);
    expect("Back returns to the first page, the keyboard on what is most likely wanted", out.back.after !== null && out.back.focused === "Install Peios");

    // Upgrade: the same disks, and what is on the chosen one.
    await key("2", "Digit2", 50);
    out.upgrade = await eventually(seen, onDisks("Upgrade: choose a disk"));
    expect("choosing Upgrade asks for the disk to upgrade, with nothing chosen", out.upgrade.after !== null && out.upgrade.disks.every((d) => !d.checked));
    expect("the panel is about what is there, and laying a disk out by hand is not offered",
        out.upgrade.label === "What's on the disk" && out.upgrade.custom === null && out.upgrade.plan.empty === "Choose a disk to see what is on it.");
    await click(disk("/dev/sdc"));
    out.upgradable = await eventually(seen, chosen("/dev/sdc"));
    await picture("disk-4-upgrade.png");
    expect("a disk holding Peios says which release, and what this medium would move it to",
        JSON.stringify(out.upgradable.plan.versions) === JSON.stringify(["On the disk2026.8-7", "This medium2026.9-1"])
        && out.upgradable.plan.notes[0] === "Peios 2026.8-7 (experimental), on sdc2.");
    expect("and its partitions, with what is in use",
        JSON.stringify(out.upgradable.plan.legend) === JSON.stringify(["EFI system partition: 71 MiB of 512 MiB · FAT32", "Peios root: 18.4 GiB of 465.3 GiB · ext4"])
        && out.upgradable.plan.erase === null && out.upgradable.plan.becomes.length === 0);
    await click(disk("/dev/nvme0n1"));
    out.notPeios = await eventually(seen, chosen("/dev/nvme0n1"));
    expect("a disk with no Peios on it says so",
        out.notPeios.plan.notes[0] === "No Peios system here: no partition on it holds a Peios system. Nothing to upgrade." && out.notPeios.plan.was === 4);
    await click("#turn .btn.quiet");
    await eventually(seen, onFirstPage);

    // Repair.
    await key("3", "Digit3", 51);
    out.repair = await eventually(seen, onDisks("Repair: choose a disk"));
    await click(disk("/dev/sdc"));
    out.repairable = await eventually(seen, chosen("/dev/sdc"));
    expect("choosing Repair asks for the disk to repair, and says what system is on it",
        out.repair.after !== null && out.repairable.plan.notes[0] === "Peios 2026.8-7 (experimental), on sdc2, with 18.4 GiB in use." && out.repairable.plan.versions.length === 0);
    await click("#turn .btn.quiet");
    await eventually(seen, onFirstPage);

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...chrome.problems];
    expect("the page reports no errors", out.problems.length === 0);

    // A machine with no disk to choose, and a controller nothing drives.
    installerd.kill();
    installerd = startInstallerd(join(root, "dev/no-disks.json"));
    out.restarted = await eventually(seen, onFirstPage, 15);
    await key("1", "Digit1", 49);
    out.none = await eventually(seen, onDisks("Choose a disk"));
    await picture("disk-5-none.png");
    expect("with no disk to choose the page says so in installerd's words",
        out.none.after !== null && out.none.disks.length === 1 && out.none.disks[0].disabled && out.none.empty === "No disks found. Attach one and rescan.");
    expect("and says what installerd found instead",
        out.none.trouble?.startsWith("A storage controller that nothing on this system is driving: a RAID controller (8086:9a0b) at 0000:00:0e.0."));
    expect("with nothing to show beside it, and the keyboard somewhere useful",
        out.none.plan.empty === "Nothing to show until a disk turns up." && out.none.focused !== "BODY" && out.none.next === "off");

    // A disk whose names are markup: they are names, and shown as written.
    installerd.kill();
    installerd = startInstallerd(hostile);
    await eventually(seen, onFirstPage, 15);
    await key("1", "Digit1", 49);
    await eventually(seen, onDisks("Choose a disk"));
    await click(disk("/dev/sda"));
    out.hostile = await eventually(seen, chosen("/dev/sda"));
    expect("what a disk calls itself is shown as text, never taken as markup",
        out.hostile.after !== null && out.hostile.markup === 0 && out.hostile.title === "Choose a disk · Peios Setup"
        && out.hostile.plan.title === '<img src=x onerror="document.title=1">' && out.hostile.plan.erase.includes("<script>document.title=2</script>"));
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    installerGxwi.kill();
    installerd.kill();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
