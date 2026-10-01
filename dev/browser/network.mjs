// First-boot setup's network page, in headless Chromium.
//
//     node dev/browser/network.mjs
//
// It starts its own oobed (--dry-run) and its own oobe-gxwi, so it needs no
// VM and nothing else running. oobed is told the machine's network is what a
// file here says `net status` prints, starting from dev/net-status.txt with
// one interface more: the one this page is served on, 127.0.0.2, as though
// the person were reaching the machine by that address.
//
// It goes from the welcome to the network page and looks at what the page
// says of each interface, that the one it is reached through is marked, and
// that what cannot be done yet says why. Then the cable is pulled, by changing the file, and Check again
// shows it for everyone looking; then netd is taken away altogether, and the
// page says so in oobed's words. An interface whose names are markup is shown
// as written. Last, a narrow screen, and Back to the welcome.
//
// oobed comes from ../installer (cargo +1.98.1 build -p oobed) and oobe-gxwi
// from this checkout (cargo +1.98.1 build).
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "oobe-gxwi-"));
const socket = join(run, "oobed.sock");
const status = join(run, "net-status.txt");
const address = "127.0.0.2:7795";
const site = `http://${address}/`;
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const sample = readFileSync(join(root, "dev/net-status.txt"), "utf8");
// The way in: an interface with the address this page is served on.
const wayIn = `
enp4s0  [5a5a5a5a-0000-5000-8000-000000000004]
  verdict    JOIN(default) by wired
  state      up, carrier
  readiness  addressed
  hardware   52:54:00:00:00:04 pci-0000:04:00.0 virtio_net
  address    127.0.0.2/8
`;
writeFileSync(status, sample + wayIn);

const oobed = spawn(join(installer, "oobed"), ["--dry-run", "--socket", socket, "--net-status", status], { stdio: "ignore" });
await sleep(400);
const oobeGxwi = spawn(join(root, "target/debug/oobe-gxwi"), ["--socket", socket, "--listen", address], { stdio: "ignore", env });
const chrome = await browser(9379);
const { send, js, picture, key, click } = chrome;

const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.getAttribute("aria-label") ?? turn.querySelector("h1")?.textContent ?? null,
        lede: turn.querySelector(".lede")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        nets: [...turn.querySelectorAll(".net")].map((n) => ({
            name: n.querySelector(".model").textContent,
            where: n.querySelector(".where").textContent,
            facts: n.querySelector(".net-facts")?.textContent ?? null,
            warning: n.querySelector(".net-warn")?.textContent ?? null,
            state: n.querySelector(".net-state").textContent,
            how: n.className,
            here: !!n.querySelector(".chip.here"),
            label: n.getAttribute("aria-label"),
            // The name and what it is, in a column of their own.
            column: Math.round(n.querySelector(".who").getBoundingClientRect().width),
        })),
        list: !turn.querySelector(".nets")?.hidden,
        words: turn.querySelector(".net-words:not([hidden])")?.textContent ?? null,
        note: turn.querySelector("p.note:not([hidden])")?.textContent ?? null,
        planned: turn.querySelector(".planned:not([hidden]) .planned-words")?.textContent ?? null,
        plannedActs: [...turn.querySelectorAll(".planned:not([hidden]) .planned-acts .btn:not([hidden])")].map((b) => b.textContent),
        links: [...turn.querySelectorAll(".aux .link")].filter((l) => !l.hidden).map((l) => [l.textContent, l.getAttribute("aria-disabled") === "true"]),
        help: turn.querySelector(".help.on")?.textContent ?? null,
        scanning: turn.querySelector(".nets")?.classList.contains("scanning") ?? false,
        focused: document.activeElement?.textContent?.trim() || document.activeElement?.tagName,
        toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
        title: document.title,
        wide: document.documentElement.scrollWidth > innerWidth,
        markup: turn.querySelectorAll("img, script, #planted").length,
    };
})()`);
const onNetwork = (s) => s.kind === "network" && s.showing;
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
    out.welcome = await eventually(seen, (s) => s.kind === "welcome" && s.showing, 15);
    expect("the welcome is showing", out.welcome.after !== null);
    await click("#turn .btn.go-on");
    out.network = await eventually(seen, onNetwork, 5);
    await sleep(900);
    out.network = { ...await seen(), after: out.network.after };
    await picture("network-1.png");
    expect("Next on the welcome goes on to the network page", out.network.after !== null && out.network.heading === "Network"
        && out.network.title === "Network · Peios Setup");
    expect("it says what oobed says of the machine", out.network.lede === "This machine is connected to a network.");
    expect("and each interface, loopback left out", JSON.stringify(out.network.nets.map((n) => [n.name, n.state])) === JSON.stringify([
        ["enp1s0 on Office", "Connected"], ["enp2s0", "Not connected"], ["wlp3s0", "Not used"], ["enp4s0", "This network only"]]));
    const [office, , wireless, wayInRow] = out.network.nets;
    expect("with what it has: addresses, the way out, the name servers",
        office.facts === "192.168.1.20/24 · fd00::5054:ff:fe12:3456/64via 192.168.1.1DNS 192.168.1.1" && office.how === "net ok rise");
    expect("and what it is", office.where === "52:54:00:12:34:56 · igc" && wireless.where === "8c:b8:7e:01:02:03 · iwlwifi");
    expect("the interface this page reaches the machine through is marked, and only that one",
        wayInRow.here && wayInRow.label === "enp4s0: This network only, the way this page reaches the machine" && out.network.nets.filter((n) => n.here).length === 1);
    expect("oobed's note is kept", out.network.note === "Setup does not need a network, and nothing here has to be answered.");
    expect("Check again, addressing by hand, and what cannot be done yet, greyed", JSON.stringify(out.network.links) === JSON.stringify([
        ["Check again", false], ["Configure manually…", false], ["Connect to Wi-Fi…", true]]));
    expect("the keyboard starts on Next", out.network.focused === "Next");
    expect("each interface is laid out as a row, not squeezed into a column", out.network.nets.every((n) => n.column > 200));
    out.unreachable = await chrome.unreachable();
    expect("a pointer reaches every button", out.unreachable.length === 0);
    expect("the words are not said twice", out.network.words === null);

    await js(`document.querySelectorAll("#turn .aux .link")[2].focus()`);
    out.why = await eventually(seen, (s) => s.help !== null, 3);
    expect("what cannot be done says why", out.why.help === "No wireless stack is packaged yet.");

    // The cable pulled: the file changes, and checking again shows it.
    writeFileSync(status, sample.replace("state      up, carrier\n  readiness  routed", "state      up, no-carrier\n  readiness  absent")
        .replace("readiness  routed\n\nlo", "readiness  absent\n\nlo"));
    await click("#turn .aux .link");
    out.checking = await eventually(seen, (s) => s.scanning, 3);
    out.checked = await eventually(seen, (s) => !s.scanning && s.lede === "This machine is not connected to a network.", 5);
    await picture("network-2-unplugged.png");
    expect("checking again is shown while it is under way, and ends", out.checking.after !== null && out.checked.after !== null);
    expect("and the page is the same page, changed", out.checked.kind === "network" && out.checked.nets[0].state === "Not connected"
        && !out.checked.nets[0].how.includes("ok"));
    const shared = (await elsewhere()).page;
    expect("for everyone looking", shared.kind === "network" && shared.status.startsWith("This machine is not connected"));

    // No netd at all: oobed's words, and no list.
    unlinkSync(status);
    await click("#turn .aux .link");
    out.gone = await eventually(seen, (s) => s.lede === "Could not ask netd about the network.", 5);
    await picture("network-3-no-netd.png");
    expect("with netd gone the page says so in oobed's words, and lists nothing", out.gone.after !== null && !out.gone.list && out.gone.nets.length === 0);

    // An interface whose names are markup, as far as a name can be: Linux
    // takes no space or slash in one, and the operator can write anything
    // in what they call a network.
    writeFileSync(status, `readiness  routed

<img>eth9<img>  [x]
  verdict    JOIN(p) by r
  state      up, carrier
  readiness  routed
  network    <b id=planted>Lab</b> [y]
  address    10.0.0.2/24
  warning    <script>document.title=2</script>
`);
    await click("#turn .aux .link");
    out.hostile = await eventually(seen, (s) => s.nets.length === 1, 5);
    expect("what an interface is called is shown as text, never taken as markup",
        out.hostile.markup === 0 && out.hostile.title === "Network · Peios Setup"
        && out.hostile.nets[0]?.name === "<img>eth9<img> on <b id=planted>Lab</b>"
        && out.hostile.nets[0]?.warning === "<script>document.title=2</script>");

    writeFileSync(status, sample + wayIn);
    await click("#turn .aux .link");
    await eventually(seen, (s) => s.nets.length === 4, 5);
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(500);
    out.phone = await seen();
    await picture("network-4-phone.png");
    expect("a phone holds it without scrolling sideways", out.phone.wide === false);
    await send("Emulation.clearDeviceMetricsOverride");

    // An address by hand: its own page, said to be applied at the end.
    await send("Emulation.clearDeviceMetricsOverride");
    await click("#turn .aux .link:nth-of-type(2)");
    const manual = () => js(`(() => {
        const turn = document.getElementById("turn");
        return {
            kind: turn.dataset.kind, showing: turn.classList.contains("in"),
            heading: turn.querySelector("h1")?.textContent, when: turn.querySelector(".when")?.textContent ?? null,
            ifaces: [...turn.querySelectorAll(".iface")].map((r) => [r.dataset.value, r.getAttribute("aria-checked"), r.getAttribute("aria-disabled")]),
            labels: [...turn.querySelectorAll(".field label")].map((l) => l.textContent),
            errors: [...turn.querySelectorAll(".field-error")].map((e) => e.textContent).filter(Boolean),
            // Clear of the bottom bar, which covers the last 84px or so.
            errorSeen: [...turn.querySelectorAll(".field-error")].filter((e) => e.textContent).every((e) => {
                const r = e.getBoundingClientRect();
                return r.top >= 0 && r.bottom <= innerHeight - 90;
            }),
            focused: document.activeElement?.id || document.activeElement?.dataset?.value || document.activeElement?.tagName,
            wide: document.documentElement.scrollWidth > innerWidth,
        };
    })()`);
    out.manual = await eventually(manual, (s) => s.kind === "manual" && s.showing, 5);
    await sleep(800);
    out.manual = await manual();
    await picture("network-5-manual.png");
    expect("Configure manually goes to a page of its own, which says it is applied later",
        out.manual.heading === "Configure manually" && out.manual.when === "Applied when setup finishes, not now.");
    expect("the wired interfaces can be chosen, the wireless one not, none yet chosen", JSON.stringify(out.manual.ifaces) === JSON.stringify([
        ["enp1s0", "false", null], ["enp2s0", "false", null], ["wlp3s0", "false", "true"], ["enp4s0", "false", null]]));
    out.manualUnreachable = await chrome.unreachable();
    expect("a pointer reaches every button on it", out.manualUnreachable.length === 0);
    expect("the fields say which are needed", JSON.stringify(out.manual.labels) === JSON.stringify(["Address", "Gateway (optional)", "Name servers (optional)"]));
    const type = async (ref, text) => js(`(() => { const i = document.getElementById("field-${ref.replace(/\W/g, "-")}"); i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input")); })()`);
    await click('#turn .iface[data-value="enp1s0"]');
    await type("manual.address", "192.168.1.30");
    await type("manual.gateway", "10.0.0.1");
    await click("#turn .btn.go-on");
    out.wrong = await eventually(manual, (s) => s.errors.length > 0, 5);
    await sleep(700);
    out.wrong = { ...await manual(), after: out.wrong.after };
    await picture("network-6-wrong.png");
    expect("and is scrolled to where it can be read", out.wrong.errorSeen);
    expect("what oobed turns down is said on its field, the page staying and the typing kept",
        out.wrong.kind === "manual" && out.wrong.errors[0] === "An address and the length of its network, as 192.168.1.20/24."
        && out.wrong.focused === "field-manual-address" && (await js(`document.getElementById("field-manual-address").value`)) === "192.168.1.30");
    await type("manual.address", "192.168.1.30/24");
    await type("manual.gateway", "192.168.1.1");
    await type("manual.dns", "1.1.1.1 9.9.9.9");
    await key("Enter", "Enter", 13);
    out.kept = await eventually(seen, (s) => onNetwork(s) && s.planned !== null, 5);
    await sleep(700);
    out.kept = await seen();
    await picture("network-7-kept.png");
    expect("saved, the network page says what happens at the end, and marks the interface",
        out.kept.planned === "At the end of setup, enp1s0 is given 192.168.1.30/24, through 192.168.1.1, asking 1.1.1.1 and 9.9.9.9 for names. Until then it keeps the address it has."
        && out.kept.nets[0].where.includes("192.168.1.30/24 at the end"));
    expect("and offers to change it or give it up instead of Configure manually",
        JSON.stringify(out.kept.plannedActs) === JSON.stringify(["Change", "Use the network's address instead"]) && !out.kept.links.some(([n]) => n.startsWith("Configure")));
    await click("#turn .planned-acts .btn:nth-child(2)");
    out.given = await eventually(seen, (s) => onNetwork(s) && s.planned === null && s.links.some(([n]) => n === "Configure manually…"), 5);
    expect("given up, it is gone", out.given.after !== null);

    await click("#turn .nav .btn.quiet");
    out.back = await eventually(seen, (s) => s.kind === "welcome" && s.showing, 5);
    expect("Back returns to the welcome", out.back.after !== null);

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...chrome.problems];
    expect("the page reports no errors", out.problems.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    oobeGxwi.kill();
    oobed.kill();
    rmSync(run, { recursive: true, force: true });
    process.exit(failed.length ? 1 : 0);
}
