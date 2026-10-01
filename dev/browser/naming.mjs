// First-boot setup's naming page, in headless Chromium.
//
//     node dev/browser/naming.mjs
//
// It starts its own oobed (--dry-run) and its own oobe-gxwi, so it needs no
// VM and nothing else running. It goes from the welcome through the network
// page and the account to the naming page, and looks at the name oobed
// offers, the machine shown as it is typed, what the page says of a name a
// network will not carry before oobed is asked, having the offered name back
// in one press, joining a domain, which says why it cannot, and what oobed
// turns down when Finish is pressed. Finishing is finish.mjs's. Last, a narrow
// screen, and Back to the account.
//
// oobed comes from ../installer (cargo +1.98.1 build -p oobed) and oobe-gxwi
// from this checkout (cargo +1.98.1 build).
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "oobe-gxwi-"));
const socket = join(run, "oobed.sock");
const address = "127.0.0.2:7797";
const site = `http://${address}/`;
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const oobed = spawn(join(installer, "oobed"), ["--dry-run", "--socket", socket, "--net-status", join(root, "dev/net-status.txt")], { stdio: "ignore" });
await sleep(400);
const oobeGxwi = spawn(join(root, "target/debug/oobe-gxwi"), ["--socket", socket, "--listen", address], { stdio: "ignore", env });
const chrome = await browser(9383);
const { send, js, picture, key, click } = chrome;

const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const input = document.getElementById("field-hostname");
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        label: turn.querySelector(".field label")?.textContent ?? null,
        value: input?.value ?? null,
        machine: turn.querySelector(".machine") && [turn.querySelector(".machine-name").textContent, turn.querySelector(".machine-what").textContent],
        off: turn.querySelector(".machine")?.classList.contains("off") ?? null,
        hint: turn.querySelector(".field-hint")?.textContent || null,
        help: turn.querySelector(".field .note:not([hidden])")?.textContent ?? null,
        count: turn.querySelector(".count")?.textContent ?? null,
        restore: [...turn.querySelectorAll(".aux .link")].filter((l) => !l.hidden).map((l) => [l.textContent, l.getAttribute("aria-disabled") === "true"]),
        why: turn.querySelector(".help.on")?.textContent ?? null,
        error: turn.querySelector(".field-error")?.textContent || null,
        focused: document.activeElement?.id || document.activeElement?.tagName,
        selected: document.activeElement?.selectionStart === 0 && document.activeElement?.selectionEnd === document.activeElement?.value?.length,
        toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
        title: document.title,
        wide: document.documentElement.scrollWidth > innerWidth,
    };
})()`);
const onNaming = (s) => s.kind === "naming" && s.showing;
const type = (ref, text) => js(`(() => { const i = document.getElementById("field-${ref.replace(/\W/g, "-")}"); i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input")); })()`);
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
    await eventually(seen, (s) => s.kind === "welcome" && s.showing, 15);
    await click("#turn .btn.go-on");
    await eventually(seen, (s) => s.kind === "network" && s.showing, 5);
    await click("#turn .nav .btn.go-on");
    await eventually(seen, (s) => s.kind === "account" && s.showing, 5);
    await type("account.name", "jack");
    await type("account.password", "correct horse");
    await type("account.confirm", "correct horse");
    await click("#turn .nav .btn.go-on");
    out.naming = await eventually(seen, onNaming, 5);
    await sleep(800);
    out.naming = await seen();
    await picture("naming-1.png");
    expect("the account answered, Next goes on to the naming page", out.naming.heading === "Name this machine"
        && out.naming.title === "Name this machine · Peios Setup");
    expect("with oobed's name offered, chosen to be typed over", out.naming.value === "peios-0000"
        && out.naming.focused === "field-hostname" && out.naming.selected);
    expect("and shown as the machine it will be", JSON.stringify(out.naming.machine) === JSON.stringify(["peios-0000", "How this machine is known on a network"])
        && out.naming.count === "10 / 63");
    out.unreachable = await chrome.unreachable();
    expect("a pointer reaches every button", out.unreachable.length === 0);
    expect("joining a domain is there, greyed", JSON.stringify(out.naming.restore) === JSON.stringify([["Join a domain instead…", true]]));

    await type("hostname", "my workshop");
    out.off = await seen();
    await picture("naming-2-off.png");
    expect("a name a network will not carry is said to be so as it is typed",
        out.off.off && out.off.hint === "Letters, digits and hyphens only, with no hyphen at either end." && out.off.machine[1] === "Not a name a network will carry yet");
    expect("in place of what a name may be, not beside it", out.off.help === null && out.naming.help?.startsWith("How this machine identifies itself"));
    expect("and the name offered can be had back", out.off.restore[0]?.[0] === "Use peios-0000");
    await click("#turn .aux .link");
    out.restored = await seen();
    expect("in one press", out.restored.value === "peios-0000" && !out.restored.off && out.restored.restore.length === 1);

    await js(`document.querySelector("#turn .aux .link[aria-disabled]").focus()`);
    out.why = await eventually(seen, (s) => s.why !== null, 3);
    expect("joining a domain says why it cannot", out.why.why?.startsWith("Domain membership needs a directory source"));

    // What oobed turns down: the hint is the page's, the word is oobed's.
    await type("hostname", "host.example.com");
    await click("#turn .nav .btn.go-on");
    out.wrong = await eventually(seen, (s) => s.error !== null, 5);
    await picture("naming-3-wrong.png");
    expect("what oobed turns down is said on the field, the page staying and the typing kept",
        out.wrong.kind === "naming" && out.wrong.error === "One name, without dots: the network the machine is on gives the rest."
        && out.wrong.value === "host.example.com" && out.wrong.focused === "field-hostname" && out.wrong.selected);
    expect("for everyone looking", (await elsewhere()).page.name.error === out.wrong.error);
    // Enter in the name is Finish too.
    await type("hostname", "-workshop");
    await key("Enter", "Enter", 13);
    out.enter = await eventually(seen, (s) => s.error === "A machine's name cannot begin or end with a hyphen.", 5);
    expect("as is Enter in the name", out.enter.after !== null);

    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(500);
    out.phone = await seen();
    await picture("naming-4-phone.png");
    expect("a phone holds it without scrolling sideways", out.phone.wide === false);
    await send("Emulation.clearDeviceMetricsOverride");

    await click("#turn .nav .btn.quiet");
    out.back = await eventually(seen, (s) => s.kind === "account" && s.showing, 5);
    expect("Back returns to the account, which asks for the password again",
        (await js(`[document.getElementById("field-account-name").value, document.getElementById("field-account-password").value]`)).join("|") === "jack|");

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
