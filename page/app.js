// Peios Setup, in a browser.
//
// The installer holds the state: which page this is and what is on it. It
// sends the whole of it whenever it changes, and this draws it. How a page
// arrives and leaves, the intro included, is decided here and nowhere else.
//
// First-boot setup is drawn by the same page, from oobe-gxwi, which holds
// oobed's conversation as the installer holds installerd's. The page says
// whose it is before anything has been sent, and the few things said here of
// "the installer" are said of setup instead.
import { WOKE } from "./bits.js";
import { createAccountPage } from "./account.js";
import { createConfirmPage } from "./confirm.js";
import { createDiskPage } from "./disk.js";
import { createEnding } from "./ending.js";
import { createField } from "./field.js";
import { createIntro } from "./intro.js";
import { createManualPage } from "./manual.js";
import { createNamingPage } from "./naming.js";
import { createNetworkPage } from "./network.js";
import { createOutro } from "./outro.js";
import { createProgressPage } from "./progress.js";
import { createRestart } from "./restart.js";
import { createWelcomePage } from "./welcome.js";

const $ = (id) => document.getElementById(id);
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const firstBoot = document.documentElement.dataset.conversation === "oobe";
const WORDS = firstBoot ? {
    reaching: "Reaching setup", silent: "Setup is not answering", unbuilt: "This step is not drawn in the browser yet.",
} : {
    reaching: "Reaching the installer", silent: "The installer is not answering",
    unbuilt: "This step is not drawn by the graphical installer yet.",
};
const els = {
    stage: $("stage"),
    introStack: $("intro-stack"),
    introLockup: $("intro-lockup"),
    headLockup: $("head-lockup"),
    edition: $("edition"),
    release: $("release"),
    page: $("page"),
    turn: $("turn"),
    ticker: $("ticker"),
    status: $("status"),
    statusText: $("status-text"),
    toast: $("toast"),
    say: $("say"),
    flash: $("flash"),
};
const { turn } = els;

/** An element, with its classes, its text or children, and its attributes. */
function el(tag, className, content, attributes = {}) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (typeof content === "string") node.textContent = content;
    else if (content) node.append(...content);
    for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, value);
    return node;
}
/** The same, as a part of a page that rises into place, `i`th in order. */
function rise(i, tag, className, content, attributes) {
    const node = el(tag, `${className} rise`.trim(), content, attributes);
    node.style.setProperty("--i", i);
    return node;
}
function svg(viewBox, d, className) {
    const NS = "http://www.w3.org/2000/svg";
    const node = document.createElementNS(NS, "svg");
    node.setAttribute("class", className);
    node.setAttribute("viewBox", viewBox);
    node.setAttribute("fill", "none");
    node.setAttribute("stroke", "currentColor");
    node.setAttribute("stroke-width", "1.6");
    node.setAttribute("stroke-linecap", "round");
    node.setAttribute("stroke-linejoin", "round");
    node.setAttribute("aria-hidden", "true");
    const path = document.createElementNS(NS, "path");
    path.setAttribute("d", d);
    node.append(path);
    return node;
}

// ---- what the installer last said, and whether it can be heard ----
let view = null;
let heard = "opening"; // this browser's own line to the installer: opening, open or lost
let socket = null;

function listen() {
    socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/live`);
    socket.onopen = () => { heard = "open"; ending.stop(); draw(); };
    socket.onmessage = (message) => {
        // Once the machine is restarting, what was said is over.
        if (restart?.active) return;
        view = JSON.parse(message.data);
        draw();
    };
    // The installer has gone, or the machine has. The page stays as it is
    // and says so, and keeps asking: a restarted installer sends everything
    // again, so there is nothing here to put right. Unless the machine is
    // restarting: then the page has its own way of waiting, and whatever
    // answers here next is not this installer.
    socket.onclose = () => {
        heard = "lost";
        // Setup applying what it was told, or done: its last acts take this
        // page's ground away, and the page follows the machine to what
        // comes after it.
        if (firstBoot && view?.page?.kind === "progress" && view.page.job === "setup" && view.page.ended?.outcome !== "failed") {
            ending.begin(view.page);
        }
        draw();
        if (restart?.active) return restart.closed();
        setTimeout(listen, 1500);
    };
}

// Asks the installer for something: to press an action, to choose a disk, or,
// once a conversation has ended, to start another. It is told which page
// this was looking at, so that what is pressed on a page that has just gone
// is not taken for an answer to the next; an ending is no page of
// installerd's, and is asked from as none. Nothing comes back but the state,
// which says what came of it.
function ask(request) {
    if (socket?.readyState !== WebSocket.OPEN || !view) return;
    if (!view.seq && !request.again) return;
    socket.send(JSON.stringify({ ...request, seq: view.seq }));
}

// ---- said to a screen reader, politely ----
let sayTimer = 0;
function say(text) {
    els.say.textContent = "";
    clearTimeout(sayTimer);
    sayTimer = setTimeout(() => (els.say.textContent = text), 60);
}

let toastTimer = 0;
function toast(...content) {
    els.toast.replaceChildren(...content);
    els.toast.classList.add("on");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => els.toast.classList.remove("on"), 2800);
}

// ---- the release, the ticker, the status line ----
function drawRelease() {
    const release = view?.release;
    for (const where of [els.release, els.edition]) {
        if (!release || (!release.variant && !release.version)) { where.replaceChildren(); continue; }
        where.replaceChildren(release.variant, el("span", "", release.version));
    }
}

// What the installer did to get here, a line at a time as the intro allows.
let linesAllowed = 0, linesShown = [];
function drawTicker() {
    const lines = (view?.trail ?? []).slice(0, linesAllowed);
    const same = linesShown.every((line, i) => lines[i] && line === lines[i].join(" "));
    if (!same) { els.ticker.replaceChildren(); linesShown = []; }
    for (const [what, to] of lines.slice(linesShown.length)) {
        els.ticker.append(el("li", "", [el("span", "k", what), ` ${to}`]));
        linesShown.push(`${what} ${to}`);
    }
}

function drawStatus() {
    const link = view?.link;
    // A job's end is the conversation's: there is nothing to be connected to.
    const ended = view?.page?.kind === "progress" ? view.page.ended?.outcome : view?.page?.kind === "ended" ? view.page.outcome : null;
    const [text, how] =
        ending.active ? [ending.state.stage === "lost" || ended !== "complete" ? "Waiting for the machine" : "Setup has finished", ending.state.stage === "lost" ? "bad" : "wait"]
        : heard === "lost" ? ["Lost touch with this machine", "bad"]
        : !link ? ["Starting", "wait"]
        : view.waiting === "again" ? ["Starting again", "wait"]
        : ended === "complete" ? ["Finished", ""]
        : ended ? ["Stopped", "bad"]
        : link.state === "connected" ? [`Connected to ${link.daemon}`, ""]
        : link.state === "lost" ? [WORDS.silent, "bad"]
        : [WORDS.reaching, "wait"];
    els.statusText.textContent = text;
    els.status.classList.toggle("bad", how === "bad");
    els.status.classList.toggle("wait", how === "wait");
}

// ---- the page ----
// Which page is to be shown, from what the installer says and whether it can
// be heard at all. Every one has a kind, which is what a change of page is.
function pageWanted() {
    // Setup is ending: the page stays as it was while it finds where to go.
    if (ending.active && view?.page) return view.page;
    if (heard === "lost") {
        return {
            kind: "away", title: "Lost touch with this machine",
            lede: "Peios Setup is trying again. If the machine is restarting, this page carries on when it is back.",
        };
    }
    if (!view) return { kind: "starting", title: "Peios Setup", lede: "Starting." };
    if (view.page) return view.page;
    if (view.link.state === "lost") {
        return {
            kind: "lost", title: WORDS.silent,
            lede: "Peios Setup is trying again.", why: view.link.why,
        };
    }
    return { kind: "starting", title: "Peios Setup", lede: `${WORDS.reaching}.` };
}

const ENDED = { complete: "Done", cancelled: "Stopped", failed: "It did not finish" };

/** Draws `page` into the turn. The pages that answer change only what
    changed, so that the keyboard stays where it was; the rest are replaced. */
function fill(page) {
    const was = turn.dataset.kind;
    turn.dataset.kind = page.kind;
    if (was === "confirm" && page.kind !== "confirm") confirmPage.gone();
    if (was === "progress" && page.kind !== "progress") progressPage.gone();
    if (was === "welcome" && page.kind !== "welcome") welcomePage.gone();
    if (was === "manual" && page.kind !== "manual") manualPage.gone();
    if (was === "account" && page.kind !== "account") accountPage.gone();
    if (was === "naming" && page.kind !== "naming") namingPage.gone();
    if (page.kind === "mode") return fillMode(page, was === "mode");
    if (page.kind === "welcome") return welcomePage.draw(page, view?.waiting);
    if (page.kind === "network") return networkPage.draw(page, view?.waiting);
    if (page.kind === "manual") return manualPage.draw(page, view?.waiting ?? null);
    if (page.kind === "account") return accountPage.draw(page, view?.waiting ?? null);
    if (page.kind === "naming") return namingPage.draw(page, view?.waiting ?? null);
    if (page.kind === "disk") return diskPage.draw(page, view?.waiting);
    if (page.kind === "confirm") return confirmPage.draw(page, view?.waiting);
    if (page.kind === "progress") return progressPage.draw(page, view?.waiting);
    const parts =
        page.kind === "unbuilt" ? {
            title: page.title || "The next step",
            lede: WORDS.unbuilt,
            why: page.id,
        }
        : page.kind === "ended" ? { title: ENDED[page.outcome] ?? "Finished", lede: page.message }
        : page;
    turn.replaceChildren(
        rise(0, "h1", "", parts.title),
        rise(1, "p", "lede", parts.lede),
        ...(parts.why ? [rise(2, "p", "lede why", parts.why)] : []),
    );
}

// The first page: what to do with this machine. One action is highlighted,
// following the pointer and the focus; arrows walk them and numbers choose.
let actions = [], modeAs = "";
function fillMode(page, already) {
    // The one pressed is shown as under way until installerd answers.
    const mark = () => actions.forEach((button, n) => button.classList.toggle("pending", view?.waiting === page.actions[n].ref));
    const as = JSON.stringify(page);
    if (already && as === modeAs) return mark();
    modeAs = as;
    const help = rise(2 + page.actions.length, "p", "menu-help", "", { id: "menu-help" });
    actions = page.actions.map((action, n) => {
        const button = el("button", `act${action.primary ? " primary" : ""}`, [
            el("span", "key", String(n + 1), { "aria-hidden": "true" }),
            el("span", "name", action.name),
            svg("0 0 18 18", "M3.5 9h11M10 4.5 14.5 9 10 13.5", "go"),
        ], { type: "button", "aria-keyshortcuts": String(n + 1) });
        if (!action.enabled) {
            button.setAttribute("aria-disabled", "true");
            button.setAttribute("aria-describedby", "menu-help");
        }
        const highlight = () => {
            for (const other of actions) other.classList.toggle("hl", other === button);
            help.textContent = action.enabled ? "" : action.help ?? "";
        };
        button.addEventListener("pointermove", (e) => {
            const r = button.getBoundingClientRect();
            button.style.setProperty("--mx", `${e.clientX - r.left}px`);
            button.style.setProperty("--my", `${e.clientY - r.top}px`);
        });
        button.addEventListener("pointerenter", highlight);
        button.addEventListener("focus", highlight);
        button.addEventListener("click", () => {
            if (!action.enabled) return;
            // The page it leads to is not drawn yet, and an answer would move
            // everyone looking on to a page nobody can see.
            if (action.unbuilt) return toast(el("b", "", action.name), " is not built yet.");
            ask({ press: action.ref });
        });
        return button;
    });
    turn.replaceChildren(
        rise(0, "h1", "", page.title),
        rise(1, "p", "lede", page.intro),
        el("ol", "menu", actions.map((button, n) => rise(2 + n, "li", "", [button]))),
        help,
    );
    const first = actions.find((button, n) => page.actions[n].primary && page.actions[n].enabled)
        ?? actions.find((button, n) => page.actions[n].enabled)
        ?? actions[0];
    first?.classList.add("hl");
    mark();
}

// The stars behind everything, which the intro gathers and one page evens out.
const field = createField($("field"), els.introLockup.querySelector(".mark"), reduced);
const diskPage = createDiskPage({ turn, el, rise, ask, toast: (text) => toast(text), reduced });
const confirmPage = createConfirmPage({
    turn, stage: els.stage, el, rise, ask, toast: (text) => toast(text), say, field, flash: els.flash, reduced,
});
const progressPage = createProgressPage({
    turn, stage: els.stage, el, rise, ask, say, retitle, field, reduced, daemon: firstBoot ? "oobed" : "installerd",
});
// Where the page goes once setup has finished: what it says of that is drawn
// on setup's job page, and said. It goes there by giving way to GXWI's
// sign-in page, which takes up its stars.
const outro = createOutro({ stage: els.stage, glows: [...document.querySelectorAll(".aurora i:not(.ember)")], field, reduced });
const ending = createEnding({
    onChange: ({ words, onward }) => {
        progressPage.following(words, onward);
        if (words) say(words);
        drawStatus();
    },
    go: (url) => outro.leave(url),
});
const welcomePage = createWelcomePage({ turn, el, rise, ask, toast: (text) => toast(text), reduced });
const networkPage = createNetworkPage({ turn, el, rise, ask, toast: (text) => toast(text), reduced });
const manualPage = createManualPage({ turn, el, rise, ask, reduced });
const accountPage = createAccountPage({ turn, el, rise, ask, toast: (text) => toast(text), reduced });
const namingPage = createNamingPage({ turn, el, rise, ask, toast: (text) => toast(text), reduced });

function menuKeys(e) {
    const at = actions.findIndex((button) => button.classList.contains("hl"));
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        actions[(at + (e.key === "ArrowDown" ? 1 : actions.length - 1)) % actions.length]?.focus();
    } else if (/^[1-9]$/.test(e.key) && +e.key <= actions.length) {
        actions[+e.key - 1].focus();
        actions[+e.key - 1].click();
    } else if (e.key === "Enter" && document.activeElement === document.body && at >= 0) {
        actions[at].click();
    }
}

/** Names the tab for a page headed `heading`. */
function retitle(heading) {
    document.title = heading && heading !== "Peios Setup" ? `${heading} · Peios Setup` : "Peios Setup";
}

// A page arriving: its heading is said and names the tab, and the keyboard
// starts at the first thing that can be answered.
function arrived() {
    // A heading that says something else to the eye (the welcome's greeting)
    // is named for what it is.
    const h1 = turn.querySelector("h1");
    const heading = (h1?.getAttribute("aria-label") ?? h1?.textContent ?? "").trim();
    retitle(heading);
    if (heading) say(heading);
    if (shown?.kind === "mode") actions.find((button) => button.classList.contains("hl"))?.focus({ preventScroll: true });
    if (shown?.kind === "disk") diskPage.focus();
    if (shown?.kind === "confirm") confirmPage.focus();
    if (shown?.kind === "progress") progressPage.focus();
    if (shown?.kind === "welcome") welcomePage.arrived();
    if (shown?.kind === "network") networkPage.focus();
    if (shown?.kind === "manual") manualPage.focus();
    if (shown?.kind === "account") accountPage.focus();
    if (shown?.kind === "naming") namingPage.focus();
}

// Where each page comes in the installation, or in setup, which decides the
// side it arrives from: a later page from the right, an earlier one from the
// left. The pages that are not steps (starting, lost, an ending) count as
// later.
const ORDER = { mode: 0, disk: 1, confirm: 2, progress: 3, welcome: 0, network: 1, manual: 2, account: 3, naming: 4 };
const order = (page) => ORDER[page?.kind] ?? 9;

// What is on the page now, and whether the intro has landed: until it has,
// nothing here can be seen, so a page is drawn without ceremony.
let shown = null, shownAs = "";
let landed = false, moving = false, lingering = 0;

// What is drawn, as one string: the page, and what on it is under way.
const drawnAs = (page) => JSON.stringify([page, view?.waiting ?? null]);

function drawPage() {
    const wanted = pageWanted();
    const as = drawnAs(wanted);
    if (as === shownAs || moving) return;
    if (!landed || wanted.kind === shown?.kind) {
        // The same page with something on it changed, or nobody looking yet.
        fill(wanted);
        [shown, shownAs] = [wanted, as];
        return;
    }
    // Another page: this one leaves, the next arrives. A page that has just
    // been begun asks for a moment first, for that to be seen.
    if (shown?.kind === "confirm") {
        const wait = confirmPage.linger();
        clearTimeout(lingering);
        if (wait > 0) return void (lingering = setTimeout(drawPage, wait));
        // Whatever it was in the middle of stops as it sets off.
        confirmPage.leave();
    }
    moving = true;
    els.page.style.setProperty("--dir", order(wanted) < order(shown) ? -1 : 1);
    turn.classList.remove("in", "first");
    turn.classList.add("out");
    setTimeout(() => {
        const next = pageWanted();
        turn.classList.remove("out");
        fill(next);
        [shown, shownAs] = [next, drawnAs(next)];
        void turn.offsetWidth;
        turn.classList.add("in");
        moving = false;
        arrived();
        // What changed while this was under way is drawn now.
        drawPage();
    }, reduced ? 0 : 420);
}

function draw() {
    // installerd has taken the restart, or this lost touch while it was
    // being asked for: the machine is going, and the page goes with it.
    if (!restart?.active && view && (view.restarting || (heard === "lost" && view.waiting === "act.reboot"))) {
        restart.begin({
            boot: view.boot,
            installed: view.page?.disk,
            leave: () => {
                if (turn.dataset.kind === "progress") progressPage.gone();
                clearTimeout(lingering);
            },
        });
    }
    if (restart?.active) return;
    // Setup complete: the moment it is left to be seen begins.
    if (firstBoot && view?.page?.kind === "progress" && view.page.job === "setup" && view.page.ended?.outcome === "complete") outro.complete();
    drawRelease();
    drawTicker();
    drawStatus();
    drawPage();
}

// ---- the intro ----
const intro = createIntro({
    els, field, reduced,
    onLines: (n) => { if (!restart?.active) { linesAllowed = n; drawTicker(); } },
    onLanded: () => { landed = true; arrived(); },
});
// What happens to the page once the machine is restarting.
const restart = createRestart({ els, el, rise, field, intro, say, retitle, reduced });

window.addEventListener("keydown", (e) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (!intro.settled) {
        if (["Shift", "Tab"].includes(e.key)) return;
        e.preventDefault();
        intro.skip();
        return;
    }
    if (!moving && shown?.kind === "mode") menuKeys(e);
});
els.stage.addEventListener("pointerdown", (e) => {
    if (!intro.settled && !e.target.closest("button")) intro.skip();
});
$("skip").addEventListener("click", () => intro.skip());
$("replay").addEventListener("click", () => { landed = false; intro.start(); });

draw();
listen();
// Arrived from the installer's page, which followed the machine's restart
// down and went on to this when first-boot setup answered.
let woke = false;
try {
    woke = sessionStorage.getItem(WOKE) === "1";
    sessionStorage.removeItem(WOKE);
} catch { /* no storage: the intro plays from the top */ }
// The lockups are measured only once the faces have arrived.
let begun = false;
const begin = () => { if (!begun) { begun = true; if (woke) intro.wake(); else intro.start(); } };
(document.fonts ? document.fonts.ready : Promise.resolve()).then(begin);
setTimeout(begin, 1500);
