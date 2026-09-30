// The confirmation: the disk that is about to be erased, and a button that is
// held down rather than pressed.
//
// The installer says what there is: the sentence, the disk it is about, and
// the two actions. One of them is marked as destroying something, and that
// mark, not its name, is why it is held: a press that was not meant is let
// go of long before it counts. How far a hold has got is this browser's
// alone; the installer hears of it only when it completes, and then everyone
// looking is shown it under way.
//
// What the disk says of itself is put on the page as text, never as markup.
import { BACK, NOT_DRAWN, becomesLegend, diskBar, glyph, icon, partitionsText, picture } from "./bits.js";

// How long the button is held, how long letting go takes to undo it, and how
// long a second press has to follow a first for the two to count as one.
const HOLD_MS = 1500, DRAIN_MS = 380, AGAIN_MS = 5000;
// How long the page stays once it is begun, for that to be seen.
const LINGER_MS = 900;
// How warm the page is on arrival, before anything is held.
const WARM = .35;

const MARK = icon("0 0 16 16", 2, '<path d="M8 3.5v5.5M8 12.5h.01"/>');
const ERASE = icon("0 0 18 18", 1.6, '<path d="M3 5h12M7.5 5V3.5h3V5M4.5 5l.8 9.5h7.4l.8-9.5"/>');

/**
 * The confirmation page. `turn` is where a page is drawn and `stage` what is
 * behind every page; `el` and `rise` make a page's parts; `ask` asks the
 * installer for something; `toast` says something in passing and `say` says
 * it to a screen reader; `field` is the stars, and `flash` lights the screen.
 */
export function createConfirmPage({ turn, stage, el, rise, ask, toast, say, field, flash, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what the installer last said is on it
    let diskAs = "";
    // The hold: how far it has got (0 to 1), whether the button is down, and
    // whether it got all the way. `heard` is whether the installer has been
    // seen to take it.
    let k = 0, holding = false, committed = false, heard = false;
    let frame = 0, last = 0, askedAt = 0, askTimer = 0, commitTimer = 0, committedAt = 0;

    const here = () => parts?.title.isConnected;
    const hint = (text) => { parts.hint.textContent = text; };
    // What the button says of itself when nothing is happening to it.
    const resting = () => page.begin?.enabled === false ? page.begin.help ?? "" : page.begin?.destructive ? "Press and hold" : "";

    // Everything that follows the hold does so from here: the button fills,
    // what the disk becomes is wiped in over what it is, the page warms, and
    // the stars even out.
    function setK(to) {
        k = to;
        turn.style.setProperty("--k", k.toFixed(4));
        stage.style.setProperty("--heat", committed ? 0 : (WARM + (1 - WARM) * k).toFixed(3));
        field.even(k);
    }

    function reset() {
        cancelAnimationFrame(frame);
        clearTimeout(askTimer);
        clearTimeout(commitTimer);
        frame = last = askedAt = 0;
        holding = committed = heard = false;
        turn.classList.remove("holding", "committed");
        setK(0);
        if (here()) hint(resting());
    }

    // Filling is steady; letting go drains it about four times as fast.
    function tick(now) {
        const passed = last ? Math.min(64, now - last) : 16;
        last = now;
        setK(holding ? Math.min(1, k + passed / HOLD_MS) : Math.max(0, k - passed / DRAIN_MS));
        if (holding && k >= 1) return held();
        if (holding || k > 0) frame = requestAnimationFrame(tick);
        else frame = last = 0;
    }
    const run = () => { if (!frame) { last = 0; frame = requestAnimationFrame(tick); } };

    function start() {
        if (committed || !here() || !page.begin?.enabled || !turn.classList.contains("in")) return;
        holding = true;
        turn.classList.add("holding");
        hint("Keep holding…");
        run();
    }

    function stop() {
        if (!holding) return;
        holding = false;
        turn.classList.remove("holding");
        if (!committed) hint(resting());
        if (k > 0) run();
    }

    // Held all the way, or pressed twice by something that cannot hold.
    function held() {
        frame = 0;
        holding = false;
        turn.classList.remove("holding");
        // What it leads to is not drawn yet, and an answer would take
        // everyone looking to it: it is said, and the hold falls back.
        if (page.begin.unbuilt) {
            hint(resting());
            toast(NOT_DRAWN);
            if (k > 0) run();
            return;
        }
        commit(true);
        ask({ press: page.begin.ref });
        // If the installer never takes it (the page had moved on under this
        // browser), the button is given back.
        commitTimer = setTimeout(() => { if (committed && !heard) reset(); }, 4000);
    }

    // It is under way: asked for from this browser, or by someone else
    // looking, who saw the flash that goes with it.
    function commit(mine) {
        committed = true;
        committedAt = performance.now();
        turn.classList.add("committed");
        setK(1);
        hint("Starting…");
        if (mine) {
            flash.classList.remove("go");
            void flash.offsetWidth;
            flash.classList.add("go");
        }
    }

    function build() {
        const backName = el("span"), beginName = el("span");
        const back = el("button", "btn quiet", [picture(BACK), backName], { type: "button" });
        back.addEventListener("click", () => page.back?.enabled && ask({ press: page.back.ref }));

        const destroy = el("button", "destroy", [
            el("span", "fill"),
            el("span", "label", [picture(ERASE), beginName]),
        ], { type: "button", "aria-describedby": "hold-hint confirm-summary" });
        // An action that destroys nothing is pressed like any other.
        const isHeld = () => page.begin?.destructive;
        destroy.addEventListener("pointerdown", (e) => {
            if (e.button !== 0 || !isHeld()) return;
            e.preventDefault();
            destroy.focus();
            start();
        });
        for (const on of ["pointerup", "pointerleave", "pointercancel", "blur"]) destroy.addEventListener(on, stop);
        destroy.addEventListener("contextmenu", (e) => e.preventDefault());
        destroy.addEventListener("keydown", (e) => {
            if ((e.key !== " " && e.key !== "Enter") || !isHeld()) return;
            e.preventDefault();
            if (!e.repeat) start();
        });
        destroy.addEventListener("keyup", (e) => {
            if ((e.key !== " " && e.key !== "Enter") || !isHeld()) return;
            e.preventDefault();
            stop();
        });
        // A screen reader's press, or a switch's, cannot be held. A click
        // that no pointer and no key made asks again instead, out loud, and
        // a second one soon after is taken as meant.
        destroy.addEventListener("click", (e) => {
            if (holding || committed || k > 0 || !page.begin?.enabled) return;
            if (!isHeld()) return held();
            if (e.detail !== 0) return;
            if (askedAt && performance.now() - askedAt < AGAIN_MS) {
                askedAt = 0;
                clearTimeout(askTimer);
                return held();
            }
            askedAt = performance.now();
            hint("Press again to confirm");
            say(`${page.begin.name}: press again to confirm. This cannot be undone.`);
            askTimer = setTimeout(() => {
                askedAt = 0;
                if (!committed && !holding && here()) hint(resting());
            }, AGAIN_MS);
        });

        parts = {
            title: el("span"),
            summary: rise(1, "p", "lede", "", { id: "confirm-summary" }),
            back, backName, destroy, beginName,
            hint: el("p", "hold-hint", "", { id: "hold-hint" }),
            disk: rise(2, "aside", "erased"),
        };
        turn.replaceChildren(
            el("div", "col", [
                rise(0, "h1", "", [el("span", "danger-badge", [picture(MARK)], { "aria-hidden": "true" }), parts.title]),
                parts.summary,
                rise(3, "div", "nav", [back, el("div", "hold-group", [destroy, parts.hint])]),
            ]),
            parts.disk,
        );
        diskAs = "";
        reset();
    }

    // The disk, beside the sentence: what it is, and what is on it, over
    // which the hold wipes what it becomes.
    function drawDisk() {
        const disk = page.disk;
        const as = JSON.stringify(disk ?? null);
        if (as === diskAs) return;
        diskAs = as;
        parts.disk.hidden = !disk;
        turn.classList.toggle("alone", !disk);
        if (!disk) return parts.disk.replaceChildren();
        const detail = disk.detail ?? {};
        const becomes = detail.becomes ?? [];
        const out = [
            el("div", "erased-head", [
                el("span", "glyph", [glyph(disk.bus)]),
                el("div", "plan-title", [el("b", "", disk.model), el("span", "", [disk.device, disk.size].filter(Boolean).join(" · "))]),
            ]),
        ];
        if (detail.bytes) {
            const bar = diskBar(el, detail);
            bar.classList.add("confirm-layout");
            bar.setAttribute("aria-label", `${disk.device} as it is: ${partitionsText(detail.partitions ?? [])}.`
                + (becomes.length ? ` As it will be: ${partitionsText(becomes)}.` : ""));
            out.push(bar);
        }
        if (becomes.length) out.push(becomesLegend(el, becomes));
        parts.disk.replaceChildren(...out);
    }

    /** Draws the page as the installer says it is, changing only what
        changed. `waiting` is the action the installer has yet to answer. */
    function draw(next, waiting) {
        page = next;
        if (!here()) build();
        parts.title.textContent = page.title;
        parts.summary.textContent = page.summary;
        const name = (node, label, action) => {
            node.hidden = !action;
            if (!action) return;
            label.textContent = action.name;
            if (action.enabled) node.removeAttribute("aria-disabled");
            else node.setAttribute("aria-disabled", "true");
        };
        name(parts.back, parts.backName, page.back);
        name(parts.destroy, parts.beginName, page.begin);
        parts.hint.hidden = !page.begin;
        drawDisk();
        if (page.begin && waiting === page.begin.ref) {
            heard = true;
            if (!committed) commit(false);
        } else if (committed && heard) {
            // The installer turned it down: the page is as it was.
            reset();
        } else if (!committed && !holding && !askedAt && k === 0) {
            hint(resting());
        }
    }

    /** Where the keyboard starts on this page: on the way back, not on the
        button that erases. */
    function focus() {
        if (here()) parts.back.focus({ preventScroll: true });
    }

    /** How long the page asks to stay before another takes its place, in ms:
        a moment once it is begun, for that to be seen. */
    function linger() {
        return committed && !reduced && here() ? Math.max(0, LINGER_MS - (performance.now() - committedAt)) : 0;
    }

    /** The page is setting off. A hold under way is let go and what it did
        to the stage is undone; a page that was begun leaves as it is, the
        disk shown as it will be. */
    function leave() {
        if (!parts) return;
        if (committed) {
            clearTimeout(commitTimer);
            field.even(0);
        } else {
            reset();
        }
        stage.style.setProperty("--heat", 0);
    }

    /** The page has gone, and leaves nothing of itself behind. */
    function gone() {
        if (!parts) return;
        reset();
        parts = null;
        stage.style.setProperty("--heat", 0);
        turn.style.removeProperty("--k");
        turn.classList.remove("alone");
    }

    return { draw, focus, linger, leave, gone };
}
