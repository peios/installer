// First-boot setup's account page, in headless Chromium.
//
//     node dev/browser/account.mjs
//
// It starts its own oobed (--dry-run) and its own oobe-gxwi, so it needs no
// VM and nothing else running. It goes from the welcome through the network
// page to the account, and looks at what is asked, who the account will be as
// the name is typed, whether the passwords match as the second is typed, the
// way to see them, Caps Lock, and that nothing typed reaches anyone else
// looking. Then a narrow screen, and Back, which leaves nothing typed
// behind. Opened by an address that is not the machine's own, the page says
// the password crosses the network unencrypted; by the loopback, it does
// not. Last, what oobed turns down, and Next on to the naming page.
//
// oobed comes from ../installer (cargo +1.98.1 build -p oobed) and oobe-gxwi
// from this checkout (cargo +1.98.1 build).
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { networkInterfaces, tmpdir } from "node:os";
import { join } from "node:path";
import { browser, eventually, sleep } from "./chrome.mjs";

const root = new URL("../../", import.meta.url).pathname;
const installer = join(root, "../installer/target/x86_64-unknown-linux-musl/debug");
const run = mkdtempSync(join(tmpdir(), "oobe-gxwi-"));
const socket = join(run, "oobed.sock");
const port = 7796;
const site = `http://127.0.0.2:${port}/`;
// An address of this machine's that is not the loopback, as a browser
// elsewhere would reach it, if it has one.
const lan = Object.values(networkInterfaces()).flat().find((a) => a.family === "IPv4" && !a.internal)?.address;
const env = { ...process.env, LD_LIBRARY_PATH: join(root, "../libpeios/target/debug") };

const oobed = spawn(join(installer, "oobed"), ["--dry-run", "--socket", socket, "--net-status", join(root, "dev/net-status.txt")], { stdio: "ignore" });
await sleep(400);
const oobeGxwi = spawn(join(root, "target/debug/oobe-gxwi"), ["--socket", socket, "--listen", `0.0.0.0:${port}`], { stdio: "ignore", env });
const chrome = await browser(9381);
const { send, js, picture, key, click } = chrome;

const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const field = (ref) => document.getElementById("field-" + ref.replace(/\\W/g, "-"));
    const input = (ref) => { const i = field(ref); return i && { value: i.value, type: i.type, autocomplete: i.autocomplete }; };
    return {
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        labels: [...turn.querySelectorAll(".field label")].map((l) => l.textContent),
        name: input("account.name"), password: input("account.password"), confirm: input("account.confirm"),
        who: turn.querySelector(".acct") && [turn.querySelector(".acct-face").textContent, turn.querySelector(".acct-name").textContent],
        match: turn.querySelector(".field-hint:not(:empty)")?.textContent ?? null,
        caps: [...turn.querySelectorAll(".field-caps")].filter((c) => !c.hidden).length,
        errors: [...turn.querySelectorAll(".field-error")].map((e) => e.textContent).filter(Boolean),
        eye: turn.querySelector(".eye")?.getAttribute("aria-pressed") ?? null,
        clear: turn.querySelector(".clear")?.textContent ?? null,
        focused: document.activeElement?.id || document.activeElement?.tagName,
        selected: document.activeElement?.selectionStart === 0 && document.activeElement?.selectionEnd === document.activeElement?.value?.length,
        toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
        title: document.title,
        wide: document.documentElement.scrollWidth > innerWidth,
    };
})()`);
const onAccount = (s) => s.kind === "account" && s.showing;
const type = (ref, text) => js(`(() => { const i = document.getElementById("field-${ref.replace(/\W/g, "-")}"); i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input")); })()`);
/** The state as any other browser is sent it. */
const elsewhere = () => new Promise((resolve, reject) => {
    const other = new WebSocket(`ws://127.0.0.2:${port}/live`);
    other.onmessage = (m) => { resolve(m.data); other.close(); };
    other.onerror = reject;
});
/** Opens `url` and goes from the welcome to the account. */
async function toAccount(url) {
    await send("Page.navigate", { url });
    await sleep(1200);
    await key("x", "KeyX", 88);
    const at = await eventually(seen, (s) => (s.kind === "welcome" || s.kind === "network" || s.kind === "account") && s.showing, 15);
    if (at.kind === "welcome") {
        await click("#turn .btn.go-on");
        await eventually(seen, (s) => s.kind === "network" && s.showing, 5);
    }
    if (at.kind !== "account") await click("#turn .nav .btn.go-on");
    await eventually(seen, onAccount, 5);
    await sleep(800);
    return seen();
}

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    out.account = await toAccount(site);
    await picture("account-1.png");
    expect("Next on the network page goes on to the account", out.account.heading === "Create your account"
        && out.account.title === "Create your account · Peios Setup");
    expect("a name, a password, and the password again",
        JSON.stringify(out.account.labels) === JSON.stringify(["User name", "Password", "Confirm password"]));
    expect("the name offered is chosen, to be typed over", out.account.name.value === "peios"
        && out.account.focused === "field-account-name" && out.account.selected);
    expect("the account is shown as who it will be", JSON.stringify(out.account.who) === JSON.stringify(["P", "peios"]));
    expect("the passwords are hidden, and a password manager knows what they are",
        out.account.password.type === "password" && out.account.confirm.type === "password"
        && out.account.password.autocomplete === "new-password" && out.account.name.autocomplete === "username");
    expect("reached by the loopback, nothing crosses a network", out.account.clear === null);
    out.unreachable = await chrome.unreachable();
    expect("a pointer reaches every button", out.unreachable.length === 0);

    await type("account.name", "Jack Palfrey");
    await type("account.password", "correct horse");
    await type("account.confirm", "correct");
    out.typing = await seen();
    expect("who it will be follows the name", JSON.stringify(out.typing.who) === JSON.stringify(["J", "Jack Palfrey"]));
    expect("nothing is said of a password half typed again", out.typing.match === null);
    await type("account.confirm", "correct hors3");
    out.differ = await seen();
    expect("one typed differently is said to differ", out.differ.match === "Does not match yet");
    await type("account.confirm", "correct horse");
    out.match = await seen();
    await picture("account-2-typed.png");
    expect("and the same, to match", out.match.match === "Matches");

    await click("#turn .eye");
    out.shown = await seen();
    expect("the eye shows both as typed", out.shown.eye === "true" && out.shown.password.type === "text" && out.shown.confirm.type === "text");
    await click("#turn .eye");
    out.hidden = await seen();
    expect("and hides them again", out.hidden.eye === "false" && out.hidden.password.type === "password");

    await js(`document.getElementById("field-account-password").dispatchEvent(new KeyboardEvent("keydown", { key: "a", modifierCapsLock: true }))`);
    out.caps = await seen();
    await js(`document.getElementById("field-account-password").dispatchEvent(new KeyboardEvent("keyup", { key: "a", modifierCapsLock: false }))`);
    out.uncaps = await seen();
    expect("Caps Lock is said while it is on", out.caps.caps === 1 && out.uncaps.caps === 0);

    const shared = await elsewhere();
    expect("nothing typed reaches anyone else looking", !shared.includes("correct horse") && !shared.includes("Jack Palfrey")
        && JSON.parse(shared).page.kind === "account");


    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(500);
    out.phone = await seen();
    await picture("account-3-phone.png");
    expect("a phone holds it without scrolling sideways", out.phone.wide === false);
    await send("Emulation.clearDeviceMetricsOverride");

    await click("#turn .nav .btn.quiet");
    await eventually(seen, (s) => s.kind === "network" && s.showing, 5);
    await click("#turn .nav .btn.go-on");
    out.again = await eventually(seen, onAccount, 5);
    expect("Back leaves nothing typed behind", out.again.name.value === "peios" && out.again.password.value === "" && out.again.confirm.value === "");

    if (lan) {
        out.lan = await toAccount(`http://${lan}:${port}/`);
        await picture("account-4-network.png");
        expect("reached over a network, the page says the password crosses it unencrypted",
            out.lan.clear?.startsWith("This page reaches the machine unencrypted"));
    } else {
        out.lan = "no address but the loopback to try";
    }

    // What lpsd would refuse, oobed turns down on the page: all of it at
    // once, each on its field, the typing kept.
    await type("account.name", "jack@home");
    await type("account.password", "one");
    await type("account.confirm", "two");
    await click("#turn .nav .btn.go-on");
    out.wrong = await eventually(seen, (s) => s.errors.length === 2, 5);
    await sleep(600);
    out.wrong = await seen();
    await picture("account-5-wrong.png");
    expect("what oobed turns down is said on each field, the page staying",
        out.wrong.kind === "account" && out.wrong.errors[0] === "A name cannot contain “@”." && out.wrong.errors[1] === "The passwords do not match.");
    expect("the keyboard goes to the first, chosen to be typed over", out.wrong.focused === "field-account-name" && out.wrong.selected);
    // Put right, and turned down for something else: what was right now is
    // no longer said to be wrong.
    await type("account.name", "jack");
    await type("account.password", "");
    await type("account.confirm", "");
    await click("#turn .nav .btn.go-on");
    out.wrongAgain = await eventually(seen, (s) => s.errors.length === 1 && s.errors[0].startsWith("Choose a password"), 5);
    expect("an error put right is taken back", out.wrongAgain.after !== null);
    await type("account.password", "correct horse");
    await type("account.confirm", "correct horse");
    await js(`document.getElementById("field-account-confirm").focus()`);
    await key("Enter", "Enter", 13);
    out.named = await eventually(seen, (s) => s.kind === "naming" && s.showing, 5);
    expect("answered, Next goes on to the naming page", out.named.after !== null);
    expect("and nothing typed went to anyone else looking", !(await elsewhere()).includes("correct horse"));

    out.elsewhere = chrome.elsewhere(site).filter((url) => !lan || new URL(url).host !== `${lan}:${port}`);
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
