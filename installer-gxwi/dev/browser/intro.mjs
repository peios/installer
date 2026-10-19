// The intro and the first page, in headless Chromium.
//
//     node dev/browser/intro.mjs [URL]
//
// URL is where the installer is: http://127.0.0.1:7780/ by default, which is
// the dev VM with the installer set as GXWI's overlay (dev/overlay.sh on), or
// wherever dev/host.sh is serving.
//
// It watches the intro through without touching it, then looks at the page it
// lands on: that it says what installerd sent, that nothing was fetched from
// anywhere else, that the keyboard works, and that a narrow screen holds it.
// A second load presses a key early, which is how the intro is skipped.
//
// The conversation is installerd's and outlives any one browser, so this
// expects to find it on its first page, and leaves it there.
import { browser, eventually, sleep } from "./chrome.mjs";

const site = process.argv[2] ?? "http://127.0.0.1:7780/";
const chrome = await browser(9370);
const { send, js, picture, key, problems } = chrome;
/** What the page is showing. */
const seen = () => js(`({
    settled: document.getElementById("stage").classList.contains("settled"),
    live: document.getElementById("page").classList.contains("live"),
    title: document.title,
    heading: document.querySelector("#turn h1")?.textContent ?? null,
    lede: document.querySelector("#turn .lede")?.textContent ?? null,
    actions: [...document.querySelectorAll("#turn .act")].map((a) => ({
        name: a.querySelector(".name").textContent, key: a.querySelector(".key").textContent,
        primary: a.classList.contains("primary"), highlighted: a.classList.contains("hl"),
        shown: getComputedStyle(a.closest(".rise")).opacity === "1",
    })),
    status: document.getElementById("status-text").textContent,
    statusShown: getComputedStyle(document.getElementById("status")).opacity,
    edition: document.getElementById("edition").textContent,
    release: document.getElementById("release").textContent,
    ticker: [...document.querySelectorAll("#ticker li")].map((li) => li.textContent),
    toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
    focused: document.activeElement?.closest?.(".act")?.querySelector(".name").textContent ?? document.activeElement?.tagName,
    header: document.getElementById("head-lockup").classList.contains("shown"),
    wide: document.documentElement.scrollWidth > innerWidth,
    // A page has arrived, and is not on its way out or in.
    shown: document.getElementById("turn").classList.contains("in") && document.getElementById("page").classList.contains("live"),
})`);

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    // The intro, left alone.
    await send("Page.navigate", { url: site });
    await sleep(1900);
    await picture("intro-1-gathering.png");
    out.gathering = await seen();
    expect("the intro is playing, and the page is not in the way of it", out.gathering.settled === false && out.gathering.live === false);
    expect("what the installer did is said as the intro plays", out.gathering.ticker.length >= 2 && out.gathering.ticker[1].startsWith("connect"));
    await sleep(1300);
    await picture("intro-2-formed.png");
    await sleep(1900);
    await picture("intro-3-named.png");
    out.named = await seen();
    expect("the release is named under the mark", out.named.release.length > 0 || out.named.edition.length === 0);
    await sleep(3400);
    await picture("intro-4-landed.png");
    out.landed = await seen();
    expect("the intro lands on a page that can be used", out.landed.settled && out.landed.live && out.landed.header);
    expect("the page is installerd's first", out.landed.heading === "Peios Setup" && out.landed.lede?.startsWith("Set up Peios on this machine"));
    expect("with its three actions, numbered and all showing",
        JSON.stringify(out.landed.actions.map((a) => [a.key, a.name, a.shown])) === JSON.stringify([
            ["1", "Install Peios", true], ["2", "Upgrade an installation", true], ["3", "Repair an existing system", true]]));
    expect("the one most likely wanted is marked, highlighted and has the keyboard",
        out.landed.actions[0]?.primary && out.landed.actions[0]?.highlighted && out.landed.focused === "Install Peios");
    expect("the status line says which installer answered", /^Connected to installerd\/\d/.test(out.landed.status) && out.landed.statusShown === "1");

    // The keyboard: arrows walk, a number chooses, and choosing goes on to
    // the page installerd asks next, and back again.
    await key("ArrowDown", "ArrowDown", 40);
    await sleep(200);
    out.walked = (await seen()).focused;
    expect("an arrow moves to the next action", out.walked === "Upgrade an installation");
    await key("3", "Digit3", 51);
    out.chosen = await eventually(seen, (s) => s.heading === "Repair: choose a disk" && s.shown, 30);
    await picture("intro-5-chosen.png");
    expect("a number chooses, and installerd's next page arrives", out.chosen.after !== null && out.chosen.title === "Repair: choose a disk · Peios Setup");
    await chrome.click("#turn .btn.quiet");
    out.back = await eventually(seen, (s) => s.heading === "Peios Setup" && s.shown && s.actions.length === 3);
    expect("and Back returns to the first", out.back.after !== null && out.back.focused === "Install Peios");

    // Nothing came from anywhere but the installer, and nothing went wrong.
    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = [...problems];
    expect("the page reports no errors", out.problems.length === 0);

    // Skipped: a key pressed early goes straight to the page.
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    // The lockup glides for a second, and the page rises behind it.
    await sleep(2600);
    out.skipped = await seen();
    expect("a key skips the intro", out.skipped.settled && out.skipped.live && out.skipped.actions.length === 3 && out.skipped.actions.every((a) => a.shown));

    // Replayed from the button, and left to finish.
    await js(`document.getElementById("replay").click()`);
    await sleep(800);
    out.replaying = (await seen()).settled;
    await sleep(6600);
    out.replayed = await seen();
    expect("the intro replays and lands again", out.replaying === false && out.replayed.settled && out.replayed.live && out.replayed.focused === "Install Peios");

    // A phone.
    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await send("Page.navigate", { url: site });
    await sleep(1200);
    await key("x", "KeyX", 88);
    await sleep(2600);
    out.phone = await seen();
    await picture("intro-6-phone.png");
    expect("a phone holds the page without scrolling sideways", out.phone.wide === false && out.phone.actions.length === 3);

    // Any other address comes here.
    await send("Emulation.clearDeviceMetricsOverride");
    await send("Page.navigate", { url: new URL("/desktop/files?open=1", site).href });
    await sleep(1500);
    out.elsewhereLands = await js(`location.pathname`);
    expect("an address left over from a desktop comes to the installer", out.elsewhereLands === "/");
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    process.exit(failed.length ? 1 : 0);
}
