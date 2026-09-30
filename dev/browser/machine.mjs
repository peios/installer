// The disk page on a real machine: the dev VM, with the disks dev/boot.sh
// gives it, and the installerd dev/push.sh put there.
//
//     node dev/browser/machine.mjs [URL]
//
// URL is where the installer is: http://127.0.0.1:7780/ by default, which is
// the dev VM with the installer set as GXWI's overlay (dev/overlay.sh on).
//
// disk.mjs checks the page against a machine that is described. This checks
// what describing cannot: that installerd really finds the disks, reads what
// is on them through the filesystems the kernel has, and says so; and it
// prints what each disk's panel says, for a person to read.
//
// The conversation is installerd's and outlives any one browser, so this
// expects to find it on its first page, and leaves it there.
import { browser, eventually, sleep } from "./chrome.mjs";

const site = process.argv[2] ?? "http://127.0.0.1:7780/";
const chrome = await browser(9373);
const { send, js, picture, key, click } = chrome;

const seen = () => js(`({
    heading: document.querySelector("#turn h1")?.textContent ?? null,
    showing: document.getElementById("turn").classList.contains("in") && document.getElementById("page").classList.contains("live"),
    actions: document.querySelectorAll("#turn .act").length,
    disks: [...document.querySelectorAll("#turn .disk")].map((d) => ({
        value: d.dataset.value, model: d.querySelector(".model").textContent, where: d.querySelector(".where").textContent,
        size: d.querySelector(".size").textContent, checked: d.getAttribute("aria-checked") === "true",
        disabled: d.getAttribute("aria-disabled") === "true",
    })),
    scanning: document.querySelector("#turn .disks")?.classList.contains("scanning") ?? false,
    trouble: document.querySelector("#turn .trouble:not([hidden])")?.textContent ?? null,
    status: document.getElementById("status-text").textContent,
    plan: {
        title: document.querySelector("#turn .plan-title b")?.textContent ?? null,
        drawn: document.querySelector("#turn .layout")?.getAttribute("aria-label") ?? null,
        legend: [...document.querySelectorAll("#turn .legend li")].map((li) => li.querySelector("b").textContent + ": " + li.querySelector("span").textContent),
        erase: document.querySelector("#turn .erase")?.textContent ?? null,
        notes: [...document.querySelectorAll("#turn .plan .note")].map((n) => n.textContent),
    },
})`);
const onFirstPage = (s) => s.heading === "Peios Setup" && s.actions === 3 && s.showing;
const chosen = (value) => (s) => s.disks.some((d) => d.value === value && d.checked) && s.plan.title !== null;
/** Chooses each disk that can be chosen in turn, and says what its panel says. */
async function panels(page) {
    const said = {};
    for (const disk of page.disks.filter((d) => !d.disabled)) {
        await click(`#turn .disk[data-value="${disk.value}"]`);
        said[disk.value] = (await eventually(seen, chosen(disk.value))).plan;
    }
    return said;
}

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    out.first = await eventually(seen, onFirstPage, 20);
    expect("the first page is showing", out.first.after !== null);

    // Install. installerd looks into every disk before it answers, which is
    // a mount for each filesystem.
    await key("1", "Digit1", 49);
    out.install = await eventually(seen, (s) => s.heading === "Choose a disk" && s.showing, 60);
    out.tookSeconds = out.install.after;
    expect("choosing Install goes on to the disks", out.install.after !== null);
    const by = (bus) => out.install.disks?.find((d) => d.where.includes(` · ${bus}`) && !d.disabled);
    const windows = by("NVMe"), data = by("SATA"), blank = by("USB"), peios = by("virtio");
    const medium = out.install.disks?.find((d) => d.disabled);
    expect("the medium it booted from is listed and cannot be chosen", medium?.where.includes("boot medium"));
    expect("the Windows disk is found, and said to hold Windows", windows?.where.endsWith(" · Windows"));
    expect("the data disk is found, with the other system on it named", data?.where.includes("Debian GNU/Linux 13 (trixie)"));
    expect("the stick is found, and marked removable", blank?.where.endsWith("removable"));
    // The stick is empty until something is installed onto it, which is
    // what it is there for: this holds either way, and says which it found.
    const empty = blank && !/ · .* · /.test(blank.where);
    out.stick = empty ? "empty" : "installed onto";
    out.becomes = await panels(out.install);
    await picture("machine-1-install.png");
    expect("the Windows disk says what would go: the system by name, not what it starts from or repairs itself with",
        out.becomes[windows?.value]?.erase === `Everything on ${windows?.value} will be erased, including Windows.`);
    expect("the data disk says what would go: the other system, read though its journal wanted replaying, and the files by their label",
        /including “MEDIA” \(\d.* in use\) and Debian GNU\/Linux 13 \(trixie\)\.$/.test(out.becomes[data?.value]?.erase ?? ""));
    if (empty) expect("the empty stick has nothing on it to lose", out.becomes[blank.value]?.erase === `Everything on ${blank.value} will be erased.`);
    expect("and each says what an install makes of it",
        [windows, data, blank].every((d) => out.becomes[d?.value]?.legend[0] === "EFI system partition: 512 MiB · FAT32" && out.becomes[d?.value]?.legend[1]?.startsWith("Peios: ")));
    // A disk Peios is on says so; with which release, where its package
    // database says. ../dist/release/disk.img may be too old to.
    if (peios) expect("the disk Peios is on says so", /^\/dev\/vd. · virtio · Peios( \d.*)?$/.test(peios.where));
    const released = out.install.disks.filter((d) => / · Peios \d/.test(d.where));
    out.released = released.map((d) => d.where);

    // Rescan: the same again, while the page shows it looking.
    await click("#turn .aux .link");
    out.scanning = await eventually(seen, (s) => s.scanning, 5);
    out.scanned = await eventually(seen, (s) => !s.scanning, 60);
    expect("a rescan is shown while it is under way, and finds the same disks",
        out.scanning.after !== null && out.scanned.after !== null && out.scanned.disks.length === out.install.disks.length);
    await click("#turn .btn.quiet");
    await eventually(seen, onFirstPage, 20);

    // Repair: what is on each.
    await key("3", "Digit3", 51);
    out.repair = await eventually(seen, (s) => s.heading === "Repair: choose a disk" && s.showing, 60);
    out.holds = await panels(out.repair);
    await picture("machine-2-repair.png");
    expect("a disk with no Peios on it says so, and what is on it instead",
        out.holds[windows?.value]?.notes[0] === "No Peios system here: no partition on it holds a Peios system."
        && out.holds[windows?.value]?.legend.length === 4 && /^Windows: \d.* of 3\.3 GiB · NTFS$/.test(out.holds[windows?.value]?.legend[2]));
    if (empty) expect("an empty disk says it has no partitions", out.holds[blank.value]?.notes[0] === "No Peios system here: it has no partitions.");
    for (const disk of released) {
        expect(`${disk.value}, which Peios is on, says which release, where, and how much is in use`,
            /^Peios \d.*, on \w+2, with \d.* in use\.$/.test(out.holds[disk.value]?.notes[0] ?? "")
            && out.holds[disk.value]?.legend[0]?.startsWith("EFI system partition: ") && out.holds[disk.value]?.legend[1]?.startsWith("Peios root: "));
    }
    if (!released.length) out.peios = "no disk here holds a Peios release to read: install onto the stick, and run this again";
    await click("#turn .btn.quiet");
    await eventually(seen, onFirstPage, 20);

    // Upgrade: what the medium would make of each system.
    if (released.length) {
        await key("2", "Digit2", 50);
        out.upgrade = await eventually(seen, (s) => s.heading === "Upgrade: choose a disk" && s.showing, 60);
        out.moves = await panels(out.upgrade);
        await picture("machine-3-upgrade.png");
        for (const disk of released) {
            // It is the medium's own release that was installed, so there
            // is nothing newer to move it to, and the page says that.
            expect(`${disk.value} says what this medium would move it to, or why it would not`,
                /^Peios \d.*, on \w+2\.( \S.*)?$/.test(out.moves[disk.value]?.notes[0] ?? ""));
        }
        await click("#turn .btn.quiet");
    }
    out.left = await eventually(seen, onFirstPage, 20);
    expect("and it is left on the first page", out.left.after !== null);

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...chrome.problems];
    expect("the page reports no errors", out.problems.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    process.exit(failed.length ? 1 : 0);
}
