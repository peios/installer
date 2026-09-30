// Peios Setup, in a browser.
//
// The installer holds the state: which page this is and what is on it. It
// sends the whole of it whenever it changes, and this draws it. How a page
// arrives and leaves, the intro included, is decided here and nowhere else.
import { createField } from "./field.js";
import { createIntro } from "./intro.js";

const $ = (id) => document.getElementById(id);
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
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

function listen() {
    const socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/live`);
    socket.onopen = () => { heard = "open"; draw(); };
    socket.onmessage = (message) => { view = JSON.parse(message.data); draw(); };
    // The installer has gone, or the machine has. The page stays as it is
    // and says so, and keeps asking: a restarted installer sends everything
    // again, so there is nothing here to put right.
    socket.onclose = () => {
        heard = "lost";
        draw();
        setTimeout(listen, 1500);
    };
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
    const [text, how] =
        heard === "lost" ? ["Lost touch with this machine", "bad"]
        : !link ? ["Starting", "wait"]
        : link.state === "connected" ? [`Connected to ${link.daemon}`, ""]
        : link.state === "lost" ? ["The installer is not answering", "bad"]
        : ["Reaching the installer", "wait"];
    els.statusText.textContent = text;
    els.status.classList.toggle("bad", how === "bad");
    els.status.classList.toggle("wait", how === "wait");
}

// ---- the page ----
// Which page is to be shown, from what the installer says and whether it can
// be heard at all. Every one has a kind, which is what a change of page is.
function pageWanted() {
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
            kind: "lost", title: "The installer is not answering",
            lede: "Peios Setup is trying again.", why: view.link.why,
        };
    }
    return { kind: "starting", title: "Peios Setup", lede: "Reaching the installer." };
}

const ENDED = { complete: "Done", cancelled: "Stopped", failed: "It did not finish" };

/** Draws `page` into the turn, replacing what was there. */
function fill(page) {
    if (page.kind === "mode") return fillMode(page);
    const parts =
        page.kind === "unbuilt" ? {
            title: page.title || "The next step",
            lede: "This step is not drawn by the graphical installer yet.",
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
let actions = [];
function fillMode(page) {
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
            // Nothing past this page is drawn yet, so nothing is answered:
            // an answer would move the installer on to a page nobody can see.
            toast(el("b", "", action.name), " is not built yet.");
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
}

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

// A page arriving: its heading is said and names the tab, and the keyboard
// starts at the first thing that can be answered.
function arrived() {
    const heading = turn.querySelector("h1")?.textContent.trim() ?? "";
    document.title = heading && heading !== "Peios Setup" ? `${heading} · Peios Setup` : "Peios Setup";
    if (heading) say(heading);
    if (shown?.kind === "mode") actions.find((button) => button.classList.contains("hl"))?.focus({ preventScroll: true });
}

// What is on the page now, and whether the intro has landed: until it has,
// nothing here can be seen, so a page is drawn without ceremony.
let shown = null, shownAs = "";
let landed = false, moving = false;

function drawPage() {
    const wanted = pageWanted();
    const as = JSON.stringify(wanted);
    if (as === shownAs || moving) return;
    if (!landed || wanted.kind === shown?.kind) {
        // The same page with something on it changed, or nobody looking yet.
        fill(wanted);
        [shown, shownAs] = [wanted, as];
        return;
    }
    // Another page: this one leaves, the next arrives.
    moving = true;
    turn.classList.remove("in", "first");
    turn.classList.add("out");
    setTimeout(() => {
        const next = pageWanted();
        turn.classList.remove("out");
        fill(next);
        [shown, shownAs] = [next, JSON.stringify(next)];
        void turn.offsetWidth;
        turn.classList.add("in");
        moving = false;
        arrived();
        // What changed while this was under way is drawn now.
        drawPage();
    }, reduced ? 0 : 420);
}

function draw() {
    drawRelease();
    drawTicker();
    drawStatus();
    drawPage();
}

// ---- the intro ----
const field = createField($("field"), els.introLockup.querySelector(".mark"), reduced);
const intro = createIntro({
    els, field, reduced,
    onLines: (n) => { linesAllowed = n; drawTicker(); },
    onLanded: () => { landed = true; arrived(); },
});

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
// The lockups are measured only once the faces have arrived.
let begun = false;
const begin = () => { if (!begun) { begun = true; intro.start(); } };
(document.fonts ? document.fonts.ready : Promise.resolve()).then(begin);
setTimeout(begin, 1500);
