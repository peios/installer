// First-boot setup's welcome, in headless Chromium.
//
//     node dev/browser/welcome.mjs [URL]
//
// URL is where setup is: http://127.0.0.1:7791/ by default, which is
// dev/host-oobe.sh. It lets the intro play, then looks at the page it lands
// on: that it says what oobed sent, that the language is shown and not a
// choice, that the keyboard is left to the browser, that the greeting goes
// round, that Next goes on to the network page and Back comes back, and that
// a narrow screen holds it. Then it arrives as a page
// following a restart would, and sees the mark woken rather than made.
//
// The conversation is oobed's and outlives any one browser, so this expects
// to find it on its first page, and leaves it there.
import { browser, eventually, sleep } from "./chrome.mjs";

const site = process.argv[2] ?? "http://127.0.0.1:7791/";
const chrome = await browser(9378);
const { send, js, picture, click } = chrome;

const seen = () => js(`(() => {
    const turn = document.getElementById("turn");
    const lockup = document.getElementById("intro-lockup");
    const box = turn.querySelector(".select");
    return {
        conversation: document.documentElement.dataset.conversation ?? null,
        kind: turn.dataset.kind,
        heading: turn.querySelector("h1")?.getAttribute("aria-label") ?? null,
        greeting: turn.querySelector(".greet")?.textContent ?? null,
        greetingLang: turn.querySelector(".greet")?.lang || null,
        lede: turn.querySelector(".lede")?.textContent ?? null,
        label: turn.querySelector(".field label")?.textContent ?? null,
        language: box?.textContent ?? null,
        languageOff: box?.getAttribute("aria-disabled") === "true",
        notes: [...turn.querySelectorAll(".note")].map((p) => p.textContent),
        next: turn.querySelector(".btn.go-on")?.textContent ?? null,
        showing: turn.classList.contains("in") && document.getElementById("page").classList.contains("live"),
        settled: document.getElementById("stage").classList.contains("settled"),
        mark: ["formed", "drawn", "sleeping", "popped", "word-in", "gone"].filter((c) => lockup.classList.contains(c)),
        ticker: [...document.querySelectorAll("#ticker li")].map((li) => li.textContent),
        status: document.getElementById("status-text").textContent,
        focused: document.activeElement?.textContent?.trim() || document.activeElement?.tagName,
        title: document.title,
        toast: document.getElementById("toast").classList.contains("on") ? document.getElementById("toast").textContent : null,
        wide: document.documentElement.scrollWidth > innerWidth,
    };
})()`);

const out = {};
const failed = [];
const expect = (what, holds) => { if (!holds) failed.push(what); };

try {
    await send("Page.navigate", { url: site });
    await sleep(1900);
    out.playing = await seen();
    expect("the page knows it is setup's before anything is sent", out.playing.conversation === "oobe");
    expect("the intro plays, from the stars", !out.playing.settled && !out.playing.mark.includes("sleeping"));
    expect("and says what setup did to get here", out.playing.ticker.some((line) => line.startsWith("connect") && line.endsWith("oobed.sock")));
    out.landed = await eventually(seen, (s) => s.kind === "welcome" && s.showing, 15);
    await sleep(900);
    out.landed = { ...await seen(), after: out.landed.after };
    await picture("welcome-1.png");
    expect("the intro lands on oobed's first page", out.landed.after !== null && out.landed.heading === "Welcome to Peios"
        && out.landed.greeting === "Welcome to Peios" && out.landed.lede === "A few questions and this machine is ready to use.");
    expect("the language is shown, and why it is not a choice",
        out.landed.label === "Language" && out.landed.language === "English" && out.landed.languageOff
        && out.landed.notes[0]?.startsWith("Peios ships in English only"));
    expect("the keyboard is left to the browser, and the page says so", out.landed.notes[1] === "Your keyboard is this device's own, so there is no layout to choose here.");
    expect("Next has the keyboard", out.landed.next === "Next" && out.landed.focused === "Next");
    expect("the status line says setup answered", /^Connected to oobed\/\d/.test(out.landed.status));
    expect("the tab is named for the page", out.landed.title === "Welcome to Peios · Peios Setup");

    out.greeted = await eventually(seen, (s) => s.greeting !== "Welcome to Peios" && s.greetingLang, 8);
    await picture("welcome-2-greeting.png");
    expect("the greeting goes round the languages, each marked as its own", out.greeted.after !== null);
    expect("while the heading is still what oobed called it", out.greeted.heading === "Welcome to Peios");

    await click("#turn .btn.go-on");
    out.next = await eventually(seen, (s) => s.kind === "network" && s.showing, 3);
    expect("Next goes on to the network page", out.next.after !== null && out.next.toast === null);
    await click("#turn .btn.quiet");
    out.back = await eventually(seen, (s) => s.kind === "welcome" && s.showing, 3);
    expect("and Back comes back to the welcome", out.back.after !== null && out.back.heading === "Welcome to Peios");

    await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(500);
    out.phone = await seen();
    await picture("welcome-3-phone.png");
    expect("a phone holds it without scrolling sideways", out.phone.wide === false);
    await send("Emulation.clearDeviceMetricsOverride");

    // Arriving from a restart another page followed down.
    await js(`sessionStorage.setItem("peios.woke", "1")`);
    await send("Page.reload");
    out.waking = await eventually(seen, (s) => s.mark.includes("formed") || s.settled, 5);
    await picture("welcome-4-waking.png");
    expect("arriving from a restart, the mark is already made, asleep or waking, in the middle",
        out.waking.after !== null && out.waking.mark.includes("drawn") && !out.waking.settled);
    out.woken = await eventually(seen, (s) => s.kind === "welcome" && s.showing, 8);
    expect("and it is on the page sooner than the intro would have been", out.woken.after !== null && out.woken.after < 5);
    expect("once", (await js(`sessionStorage.getItem("peios.woke")`)) === null);

    const hello = await (await fetch(`${site}hello`)).json();
    out.hello = hello;
    expect("asked who is here, it says setup, and which boot", /^oobe-gxwi\//.test(hello.setup) && typeof hello.boot === "string" && !("installer" in hello));

    out.elsewhere = chrome.elsewhere(site);
    expect("nothing is fetched from anywhere else", out.elsewhere.length === 0);
    out.problems = chrome.problems;
    expect("the page reports no errors", out.problems.length === 0);
} finally {
    out.failed = failed;
    console.log(JSON.stringify(out, null, 1));
    chrome.close();
    process.exit(failed.length ? 1 : 0);
}
