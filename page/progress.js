// A job as it runs, and how it ended: the installation, above all.
//
// The installer says what there is: which job this is and what it is being
// done to, its phases and how far along each is, the last of what the job has
// said of itself, and, once the job is over, how it ended. Nothing is
// answered while it runs: a job asks nothing. A job that finished is
// followed by installerd's offer to restart the machine, or to go back to the
// start, and those are drawn here, on the page of the job they follow. One
// that failed ends the conversation, and what is left to ask for then is
// another.
//
// How it is shown is this page's. The bars glide to what installerd reports
// rather than jumping to it. One figure is made of all the phases. The disk
// beside them is drawn being made, each phase moving its own part of the
// picture. And while the system is copied the stars drain into the disk, to
// come back out when it is done.
//
// Under the disk an install is making, the page says what two of its steps
// leave on it that nothing else shows: the security descriptor written to
// the root as it is formatted, which types itself out and then says who it
// lets do what, and, once the machine is being made to start from the disk,
// the one file the firmware starts.
//
// What the job and the disk say of themselves is put on the page as text,
// never as markup.
//
// First-boot setup's job, applying what it was told, is drawn here too, with
// no disk. Finished, it does not offer a restart but the sign-in page, which
// is what the machine goes back to; and the page follows the machine there by
// itself once its connection closes (ending.js), saying so here.
import { AGAIN, ONWARD, POWER, SAVE, glyph, icon, madeBar, partitionsText, picture, sizeText } from "./bits.js";

// How much of the whole each phase is taken to be, for the one figure. The
// shares are this page's, by the phase's ref: installerd says how far along
// each phase is and nothing of how long each takes. One not named here is
// given a middling share.
const SHARES = { "phase.partition": 4, "phase.format": 8, "phase.copy": 70, "phase.boot": 18, "phase.upgrade": 80 };
const share = (phase) => SHARES[phase.ref] ?? 20;
// Which part of the disk's picture each phase of an install moves, and which
// phase finishes each of the partitions it makes.
const DRAWS = { "phase.partition": "--made", "phase.format": "--fmt", "phase.copy": "--copy", "phase.boot": "--boot" };
const FINISHES = { esp: "phase.boot", root: "phase.copy" };
// The phase whose work is poured into the disk, which is when the stars are.
const POURS = "phase.copy";
// What a job that did not finish is called.
const STOPPED = { install: "Installation stopped", upgrade: "Upgrade stopped", repair: "Repair stopped", setup: "Setup stopped" };
// How warm the page is once a job has stopped.
const WARM = .3;
// How many of the job's lines the page keeps.
const KEPT = 200;

// The descriptor installerd writes to the root it makes, with mke2fs's
// `-E root_sddl=`, as installerd's real.rs has it (ROOT_SDDL), and the file
// it makes the machine start from on the EFI system partition (in
// write_boot_files). What installerd says it ran is taken over these, and
// they stand for a job that runs neither, as a dry run does.
const ROOT_SDDL = "O:SYG:SYD:(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)(A;OICI;GRGX;;;WD)(A;OICIIO;GA;;;S-1-3-0)";
const BOOT_FILE = "EFI/BOOT/BOOTX64.EFI";
const SAYS_SDDL = /^\$ mke2fs .*-E root_sddl=(\S+)/;
const SAYS_BOOT = /^\$ mkuki .*--out \S*?\/(EFI\/\S+)/;
// How far through formatting installerd is when it runs mke2fs, which
// writes the descriptor.
const STAMPED = .6;
// Who and what a descriptor's entries name, in words.
const WHO = {
    SY: "SYSTEM", "S-1-5-18": "SYSTEM", BA: "Administrators", "S-1-5-32-544": "Administrators",
    WD: "Everyone", "S-1-1-0": "Everyone", CO: "Creator owner", "S-1-3-0": "Creator owner",
    AU: "Authenticated users", "S-1-5-11": "Authenticated users", BU: "Users", "S-1-5-32-545": "Users",
};
const RIGHTS = {
    GA: "full control", FA: "full control", GRGX: "read & execute", GXGR: "read & execute",
    GR: "read", GX: "execute", GW: "write", GRGW: "read & write", GWGR: "read & write",
};
/** Who a descriptor lets do what, an entry of its DACL at a time, in words:
    an entry this page has no words for is given as written. */
function aces(sddl) {
    const dacl = sddl.indexOf("D:");
    if (dacl < 0) return [];
    return [...sddl.slice(dacl).matchAll(/\(([^)]*)\)/g)].map(([, ace]) => {
        const [type, flags = "", rights = "", , , trustee = ""] = ace.split(";");
        const who = WHO[trustee] ?? trustee;
        let what = RIGHTS[rights] ?? rights;
        // Inherited only: it is what is made inside that it applies to.
        if ((flags.match(/../g) ?? []).includes("IO")) what += who === "Creator owner" ? " of what they create" : " of what is made inside";
        return { who, what: type === "D" ? `denied ${what}` : what };
    });
}

const SHIELD = icon("0 0 16 16", 1.5, '<path d="M8 1.75 13.5 4v4c0 3.2-2.4 5.4-5.5 6.25C4.9 13.4 2.5 11.2 2.5 8V4z"/>');
const CHIP = icon("0 0 16 16", 1.5, '<rect x="3.5" y="3.5" width="9" height="9" rx="1.5"/><path d="M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2"/>');

const STATE = icon("0 0 22 22", 1.5,
    '<circle class="ring" cx="11" cy="11" r="9.5"/><circle class="arc" cx="11" cy="11" r="9.5"/>'
    + '<g class="okay"><circle cx="11" cy="11" r="10"/><path d="M6.8 11.3l2.8 2.8 5.6-6"/></g>'
    + '<g class="stop"><circle cx="11" cy="11" r="10"/><path d="M7.8 7.8l6.4 6.4M14.2 7.8l-6.4 6.4"/></g>');
const DONE = '<svg class="done-check" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="10" pathLength="1" transform="rotate(-90 12 12)"/><path d="M7.5 12.5l3 3 6-6.5" pathLength="1"/></svg>';
const OK = '<svg class="ok" viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="9"/><path d="M6 10.3l2.6 2.6 5.4-5.8"/></svg>';

const lower = (text) => text.charAt(0).toLowerCase() + text.slice(1);
/** installerd's last word is one string. Its first sentence heads the page,
    and the rest, what the job found or what comes next, goes under it. */
function sentences(message) {
    const cut = message.indexOf(". ");
    return cut < 0 ? [message.replace(/\.$/, ""), ""] : [message.slice(0, cut), message.slice(cut + 2)];
}

/**
 * The page a job runs on. `turn` is where a page is drawn and `stage` what is
 * behind every page; `el` and `rise` make a page's parts; `ask` asks the
 * installer for something; `say` says something to a screen reader and
 * `retitle` names the tab; `field` is the stars.
 */
export function createProgressPage({ turn, stage, el, rise, ask, say, retitle, field, reduced, daemon = "installerd" }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what the installer last said is on it
    let rows = [];      // one for each phase, in order
    let phasesAs = "", diskAs = "";
    // How far along each phase is drawn, 0 to 100, by its ref. It follows
    // what installerd reports, a little behind.
    let shown = new Map();
    let frame = 0;
    let figure = -1;    // the one figure, as last written
    let heard = 0;      // how many of the job's lines have been put on the page
    let nowAt = -1;     // the phase last said to be under way
    let over = null;    // how the job ended, as last drawn
    let headed = "";
    let pouring = false;
    let onward = { words: "", url: "/" };  // where a finished setup goes, and how it is getting there
    // Under the disk: what is shown there ("sd", "boot" or nothing, null
    // before it is first drawn), the descriptor and the boot file as last
    // heard, and the descriptor typing itself out.
    let spotAs = null, sddl = ROOT_SDDL, bootFile = BOOT_FILE, typing = 0;

    const here = () => parts?.title.isConnected;
    const fraction = (phase) => phase.max > 0 ? Math.min(1, Math.max(0, phase.value / phase.max)) : 0;
    const outcome = () => page.ended?.outcome ?? null;
    // The first phase that is not done: the one under way, or the one the
    // job stopped in. None when all of them are.
    const reached = () => page.phases.findIndex((phase) => fraction(phase) < 1);

    // How each phase stands, from how far along installerd says it is and
    // how the job ended: done, under way, the one it stopped in, never
    // started, or not yet.
    function standing() {
        const ended = outcome(), at = reached();
        return page.phases.map((phase, i) =>
            ended === "complete" || at < 0 || i < at ? "done"
            : i > at ? (ended ? "skipped" : "")
            : ended ? "failed" : "active");
    }

    function build() {
        const title = el("span"), againName = el("span");
        const num = el("span", "num", "0");
        const pct = el("div", "pct", [num, el("span", "sign", "%"), picture(DONE)], {
            role: "progressbar", "aria-label": "Overall progress", "aria-valuemin": "0", "aria-valuemax": "100", "aria-valuenow": "0",
        });
        const nowName = el("b"), nowCount = el("span");
        const log = el("ol", "log", null, { "aria-label": "What the job has said" });
        const busy = (button) => button.getAttribute("aria-disabled") === "true";
        const again = el("button", "btn", [picture(AGAIN), againName], { type: "button", "data-way": "again" });
        again.addEventListener("click", () => {
            if (!page.ended || busy(again)) return;
            // Back to the start is installerd's to offer after a job that
            // finished; after one that failed, it is another conversation.
            ask(page.ended.start ? { press: page.ended.start.ref } : { again: true });
        });
        const rebootName = el("span");
        const reboot = el("button", "btn go-on", [picture(POWER), rebootName], { type: "button", "data-way": "reboot" });
        reboot.addEventListener("click", () => {
            if (page.ended?.reboot && !busy(reboot)) ask({ press: page.ended.reboot.ref });
        });
        const refused = el("p", "err", "", { role: "alert" });
        // Setup's way on: the sign-in page, wherever the machine now is.
        const signIn = el("a", "btn go-on", [el("span", "", "Sign in"), picture(ONWARD)], { href: "/", "data-way": "sign-in" });
        const going = el("p", "going", "", { role: "status" });
        parts = {
            title, num, pct, nowName, nowCount, log, again, againName, reboot, rebootName, refused, signIn, going,
            // The heading takes the keyboard when the page arrives: nothing
            // on it can be pressed while the job runs.
            heading: rise(0, "h1", "", [title], { tabindex: "-1" }),
            lede: rise(1, "p", "lede"),
            said: el("code"),
            phases: rise(3, "ol", "phases"),
            nav: rise(5, "div", "nav ends", [signIn, reboot, again, refused, going]),
            disk: rise(2, "aside", "build"),
            bar: null, made: [], spot: null,
        };
        parts.saidBox = rise(1, "p", "said", [`${daemon} said `, parts.said]);
        turn.replaceChildren(
            el("div", "col", [
                parts.heading,
                parts.lede,
                parts.saidBox,
                rise(2, "div", "overall", [pct, el("div", "overall-now", [nowName, nowCount])]),
                parts.phases,
                rise(4, "div", "details", [
                    el("div", "details-top", [
                        el("p", "details-head", "Details"),
                        el("a", "link", [picture(SAVE), "Save the log"], { href: "log.txt", download: "peios-setup.log" }),
                    ]),
                    log,
                ]),
                parts.nav,
            ]),
            parts.disk,
        );
        rows = [];
        phasesAs = diskAs = headed = "";
        shown = new Map();
        figure = nowAt = -1;
        heard = 0;
        // A page that arrives already ended is drawn as it stands, without
        // the moment of its ending.
        over = outcome();
        pouring = false;
        sddl = ROOT_SDDL;
        bootFile = BOOT_FILE;
    }

    // The phases, in installerd's order: a mark for how each stands, its
    // name, how far along it is, and a bar while it is the one under way.
    function drawPhases() {
        const as = JSON.stringify(page.phases.map((phase) => [phase.ref, phase.name]));
        if (as === phasesAs) return;
        phasesAs = as;
        shown = new Map();
        rows = page.phases.map((phase) => {
            const val = el("span", "pval", "", { "aria-hidden": "true" });
            const fill = el("i");
            const bar = el("span", "ptrack", [fill], {
                role: "progressbar", "aria-label": phase.name, "aria-valuemin": "0", "aria-valuemax": "100", "aria-valuenow": "0",
            });
            const li = el("li", "phase", [el("span", "st", [picture(STATE)]), el("span", "pname", phase.name), val, bar]);
            return { li, val, bar, fill, state: "", said: null };
        });
        parts.phases.replaceChildren(...rows.map((row) => row.li));
    }

    // The disk being made, beside the phases: what it is, and what it
    // becomes, each part filling as the job gets to it. Only a disk an
    // install is making anew is drawn; the jobs on a system already there
    // say which disk in words.
    function drawDisk() {
        const disk = page.disk;
        const as = JSON.stringify(disk ?? null);
        if (as === diskAs) return;
        diskAs = as;
        const detail = disk?.detail ?? {};
        const becomes = detail.bytes ? detail.becomes ?? [] : [];
        parts.disk.hidden = !becomes.length;
        turn.classList.toggle("alone", !becomes.length);
        parts.bar = null;
        parts.made = [];
        parts.spot = null;
        clearInterval(typing);
        typing = 0;
        spotAs = null;
        if (!becomes.length) return parts.disk.replaceChildren();
        parts.bar = madeBar(el, detail);
        parts.bar.setAttribute("aria-label", `${disk.device} as it is being made: ${partitionsText(becomes)}.`);
        parts.made = becomes.map((b) => ({
            finishes: FINISHES[b.role],
            li: el("li", "part", [
                el("i", `sw r-${b.role}`),
                el("div", "", [el("b", "", b.title), el("span", "", `${sizeText(b.bytes)} · ${b.fs}`)]),
                picture(OK),
            ]),
        }));
        parts.disk.replaceChildren(
            el("div", "build-head", [
                el("span", "glyph", [glyph(disk.bus)]),
                el("div", "plan-title", [el("b", "", disk.model), el("span", "", [disk.device, disk.size].filter(Boolean).join(" · "))]),
            ]),
            parts.bar,
            el("ul", "parts", parts.made.map((made) => made.li)),
            ...(page.job === "install" ? [spot()] : []),
        );
    }

    // What two of an install's steps leave on the disk, in the card's last
    // place, one at a time: the root's descriptor and who it lets do what,
    // and then the file the firmware starts.
    function spot() {
        const head = (id, svg, words) => el("h2", "prog-spot-head", [picture(svg), words], { id });
        const sd = el("section", "prog-sd", [
            head("prog-sd-head", SHIELD, "The root's security descriptor"),
            el("code", "prog-sddl"),
            el("ul", "prog-aces", null, { "aria-label": "Who it lets do what" }),
            el("p", "", "Written as the root is formatted. Everything created inside inherits from it."),
        ], { "aria-labelledby": "prog-sd-head" });
        const boot = el("section", "prog-boot", [
            head("prog-boot-head", CHIP, "Boot"),
            el("code", "prog-boot-file"),
            el("p", "", "One file on the EFI partition holds the kernel, the initramfs and the command line. The firmware starts it directly, with no boot manager in between."),
        ], { "aria-labelledby": "prog-boot-head" });
        parts.spot = el("div", "prog-spot", [sd, boot], { "data-show": "" });
        // Filled from the start, unseen, so that the place kept for it is
        // the size it will be.
        fill();
        return parts.spot;
    }

    // The descriptor, whole, its parts' letters marked, and who it lets do
    // what, not yet shown.
    function fill() {
        parts.spot.querySelector(".prog-sddl").replaceChildren(...sddl.split(/([OGD]:)/).filter(Boolean)
            .map((piece) => /^[OGD]:$/.test(piece) ? el("span", "prog-k", piece) : piece));
        parts.spot.querySelector(".prog-aces").replaceChildren(...aces(sddl).map(({ who, what }, i) => {
            const li = el("li", "prog-ace", [el("b", "", who), ` ${what}`]);
            li.style.setProperty("--i", i);
            return li;
        }));
    }

    // The descriptor, whole, and who it lets do what said under it, each
    // seen to come.
    function stamped() {
        const sd = parts.spot.querySelector(".prog-sd");
        sd.classList.remove("decoded");
        fill();
        void sd.offsetWidth;
        sd.classList.add("decoded");
    }

    // The descriptor types itself out, a little at a time, and then is
    // shown whole.
    function typeOut() {
        const code = parts.spot.querySelector(".prog-sddl");
        const sd = parts.spot.querySelector(".prog-sd");
        sd.classList.remove("decoded");
        let n = 0;
        clearInterval(typing);
        typing = setInterval(() => {
            if (!parts?.spot?.isConnected) {
                clearInterval(typing);
                typing = 0;
                return;
            }
            n = Math.min(sddl.length, n + 2);
            if (n < sddl.length) return code.replaceChildren(sddl.slice(0, n), el("i", "prog-caret", "", { "aria-hidden": "true" }));
            clearInterval(typing);
            typing = 0;
            stamped();
        }, 30);
    }

    // Which of the two is shown, from how far the job has got: the
    // descriptor from when installerd runs mke2fs, part way through
    // formatting, and the boot file from when the copy is done and the
    // machine is being made to start from the disk. A page that arrives
    // with either already there shows it as it stands.
    function drawSpot() {
        if (!parts.spot) return;
        const at = reached();
        const index = (ref) => page.phases.findIndex((phase) => phase.ref === ref);
        const format = index("phase.format"), boot = index("phase.boot");
        const past = (i) => i >= 0 && (at < 0 || at > i);
        const booting = boot >= 0 && (at < 0 || at >= boot);
        const written = past(format) || (format >= 0 && at === format && fraction(page.phases[format]) >= STAMPED);
        const show = booting ? "boot" : written ? "sd" : "";
        parts.spot.querySelector(".prog-boot-file").textContent = bootFile;
        const arriving = spotAs === null;
        if (show !== spotAs) {
            parts.spot.dataset.show = show;
            if (show && !typing && (arriving || spotAs === "")) {
                if (show === "sd" && !arriving && !reduced) typeOut();
                else stamped();
            }
            spotAs = show;
        }
    }

    // What the job has said of itself: a command it ran, what that printed,
    // or something it said in its own words.
    function line(text) {
        if (text.startsWith("$ ")) return el("li", "cmd", [el("span", "pr", "$ "), text.slice(2)], { title: text });
        // An error, as the tool or the kernel wrote it.
        if (text.startsWith("! ")) return el("li", "prog-err", text.slice(2), { title: text });
        return el("li", text.startsWith("  ") ? "" : "say", text, { title: text });
    }

    // Only the lines that are new are added. Whether the last of them is to
    // be kept in view: it is while the job runs, and once it is over the
    // list is the reader's to scroll, so only if that is where they are.
    function drawLog() {
        const log = parts.log;
        if (page.said < heard) { log.replaceChildren(); heard = 0; }
        const fresh = Math.min(page.said - heard, page.lines.length);
        const follows = !heard || !over || log.scrollTop + log.clientHeight >= log.scrollHeight - 4;
        heard = page.said;
        if (fresh <= 0) return follows;
        const lines = page.lines.slice(page.lines.length - fresh);
        log.append(...lines.map(line));
        // What it says it wrote is what the disk's card says it wrote.
        const was = sddl;
        for (const text of lines) {
            sddl = SAYS_SDDL.exec(text)?.[1] ?? sddl;
            bootFile = SAYS_BOOT.exec(text)?.[1] ?? bootFile;
        }
        if (sddl !== was && parts.spot && !typing) (spotAs ? stamped : fill)();
        while (log.children.length > KEPT) log.firstChild.remove();
        return follows;
    }

    // The words: the heading, what is under it, which phase is under way,
    // and once it is over the way to start again. `waiting` is what has been
    // asked for and not yet come.
    function drawWords(waiting) {
        const ended = outcome();
        const at = reached(), count = page.phases.length;
        const phase = at < 0 ? null : page.phases[at];
        let heading = page.title, lede = page.summary, said = "";
        if (ended === "complete") {
            [heading, lede] = sentences(page.ended.message);
            heading ||= "Done";
        } else if (ended) {
            heading = STOPPED[page.job] ?? "Stopped";
            lede = ended === "cancelled" ? "It was stopped before it finished."
                : phase ? `It stopped while ${lower(phase.name)}.`
                : "It stopped before it finished.";
            said = page.ended.message;
        }
        if (heading !== headed) {
            // A heading that changes while the page is up is seen to.
            if (headed) {
                parts.heading.classList.remove("swap");
                void parts.heading.offsetWidth;
                parts.heading.classList.add("swap");
            }
            headed = heading;
            parts.title.textContent = heading;
        }
        parts.lede.textContent = lede;
        parts.lede.hidden = !lede;
        parts.said.textContent = said;
        parts.saidBox.hidden = !said;
        turn.classList.toggle("finished", ended === "complete");
        turn.classList.toggle("stopped", !!ended && ended !== "complete");

        parts.nowName.textContent = ended === "complete" ? "Complete" : phase?.name ?? page.title;
        parts.nowCount.textContent = count ? `${ended === "complete" || at < 0 ? count : at + 1} of ${count}` : "";
        if (!ended && at !== nowAt) {
            // The first is said with the page's heading as it arrives.
            if (nowAt >= 0 && phase) say(`${phase.name}, step ${at + 1} of ${count}.`);
            nowAt = at;
        }

        parts.nav.hidden = !ended;
        const reboot = page.ended?.reboot, start = page.ended?.start;
        // Setup that finished has nothing to start again: what is left is to
        // sign in.
        const signing = page.job === "setup" && ended === "complete";
        parts.signIn.hidden = !signing;
        parts.signIn.href = onward.url;
        parts.going.textContent = signing ? onward.words : "";
        parts.going.hidden = !signing || !onward.words;
        parts.again.hidden = signing;
        parts.reboot.hidden = !reboot;
        parts.rebootName.textContent = waiting === reboot?.ref ? "Restarting…" : reboot?.name ?? "";
        parts.again.className = ended === "complete" ? "btn quiet" : "btn go-on";
        parts.againName.textContent = waiting === "again" || (start && waiting === start.ref) ? "Starting again…"
            : start?.name ?? (ended === "complete" ? "Back to the start" : "Start again");
        // One thing at a time: while either is under way, neither is pressed.
        for (const button of [parts.reboot, parts.again]) {
            if (waiting) button.setAttribute("aria-disabled", "true");
            else button.removeAttribute("aria-disabled");
        }
        parts.refused.textContent = page.ended?.error ?? "";
        parts.refused.hidden = !page.ended?.error;
        // Once it is over the list can be scrolled, by a keyboard too.
        if (ended) parts.log.setAttribute("tabindex", "0");
        else parts.log.removeAttribute("tabindex");
    }

    // The moment the job ends: it is said, the tab is named for it, and the
    // keyboard is taken to the way on, unless it is busy somewhere else.
    function finished() {
        const ended = outcome();
        retitle(headed);
        say(ended === "complete" ? page.ended.message : [`${headed}.`, parts.lede.textContent, page.ended.message].filter(Boolean).join(" "));
        parts.log.scrollTop = parts.log.scrollHeight;
        const focused = document.activeElement;
        if (!focused || focused === document.body || focused === parts.heading) wayOn().focus({ preventScroll: true });
    }

    // What the keyboard is taken to once the job is over: the restart, where
    // there is one, and otherwise the way back.
    const wayOn = () => (page.ended?.reboot ? parts.reboot : !parts.signIn.hidden ? parts.signIn : parts.again);

    // Where the stars drain to: the whole of the disk's bar, and while the
    // system is being copied, the leading edge of what has been copied.
    function where() {
        if (!parts?.bar?.isConnected) return null;
        const bar = parts.bar.getBoundingClientRect();
        const place = { left: bar.left, width: bar.width, y: bar.top + bar.height / 2, h: bar.height, edge: false, ex: 0 };
        const filled = pouring ? parts.bar.querySelector(".r-root .fill") : null;
        if (filled) {
            place.edge = true;
            place.ex = filled.getBoundingClientRect().right;
        }
        return place;
    }

    // What the stars and the glow behind the page do, which follows how the
    // job stands. Every call but the ending's says the same thing as the
    // last, so it can be made as often as the page is drawn. `ending` is
    // whether the job has this moment ended.
    function mood(ending) {
        const ended = outcome();
        if (ended === "complete") {
            // What the disk took in comes back out of it.
            if (ending && !(parts.bar && field.finale(parts.bar.getBoundingClientRect()))) field.even(0);
            return;
        }
        if (ended) {
            field.stall();
            stage.style.setProperty("--heat", WARM);
            return;
        }
        const at = reached(), pours = page.phases.findIndex((phase) => phase.ref === POURS);
        pouring = pours >= 0 && at === pours && !!parts.bar;
        if (pouring) return field.stream(where);
        // Before the copy the field is evened out, ready to drain; after
        // it, the disk keeps what it took in until the job is over.
        if (pours >= 0 && (at < 0 || at > pours)) field.hold();
        field.even(1);
    }

    // One frame: what is drawn moves toward what installerd reports.
    function tick() {
        frame = 0;
        if (!here()) return;
        const stands = standing(), complete = outcome() === "complete";
        let moving = false, whole = 0, shares = 0;
        page.phases.forEach((phase, i) => {
            const row = rows[i];
            const to = complete ? 100 : fraction(phase) * 100;
            // The page arrives with each phase where installerd says it is;
            // it is what is reported after that which is glided to.
            let at = shown.get(phase.ref) ?? to;
            at = reduced || Math.abs(to - at) < .05 ? to : at + (to - at) * .12;
            if (at !== to) moving = true;
            shown.set(phase.ref, at);
            whole += share(phase) * at / 100;
            shares += share(phase);
            // A phase that is done is shown as under way until its bar has
            // got there.
            const state = stands[i] === "done" && at < 99.5 ? "active" : stands[i];
            if (row.state !== state) {
                row.state = state;
                row.li.className = `phase ${state}`.trim();
            }
            row.fill.style.width = `${at.toFixed(2)}%`;
            const said = state === "failed" ? `Stopped at ${Math.floor(at)}%`
                : state === "skipped" ? "Not started"
                : phase.max > 0 ? `${Math.floor(at)}%` : "";
            // The bar tells assistive technology only when the words change.
            if (row.said !== said) {
                row.said = said;
                row.val.textContent = said;
                row.bar.setAttribute("aria-valuenow", Math.floor(at));
                row.bar.setAttribute("aria-valuetext", said || phase.name);
            }
            if (DRAWS[phase.ref]) parts.disk.style.setProperty(DRAWS[phase.ref], (at / 100).toFixed(4));
            for (const made of parts.made) if (made.finishes === phase.ref) made.li.classList.toggle("done", state === "done");
        });
        const now = complete ? 100 : Math.min(99, Math.floor(shares ? whole / shares * 100 : 0));
        if (now !== figure) {
            figure = now;
            parts.num.textContent = now;
            parts.pct.setAttribute("aria-valuenow", now);
        }
        // What the disk's picture is doing, from which phase is under way.
        const under = outcome() ? null : page.phases[reached()]?.ref;
        parts.disk.classList.toggle("copying", under === POURS);
        parts.disk.classList.toggle("booting", under === "phase.boot");
        parts.disk.classList.toggle("charged", !outcome() && (shown.get(POURS) ?? 0) >= 99.5);
        if (moving) frame = requestAnimationFrame(tick);
    }

    /** Draws the page as the installer says it is, changing only what
        changed. `waiting` is what has been asked for and not yet come. */
    function draw(next, waiting) {
        page = next;
        if (!here()) build();
        drawPhases();
        drawDisk();
        const follows = drawLog();
        drawSpot();
        drawWords(waiting);
        // Once the words are drawn, since a job that is over lets its long
        // lines wrap, which makes the list longer.
        if (follows) parts.log.scrollTop = parts.log.scrollHeight;
        const ending = outcome() !== over;
        over = outcome();
        mood(ending);
        if (ending) finished();
        if (!frame) frame = requestAnimationFrame(tick);
    }

    /** Where the keyboard starts on this page: on the heading while the job
        runs, and on the way on once it is over. The stars are told again
        what to do, since the intro may only now have let them go. */
    function focus() {
        if (!here()) return;
        (page.ended ? wayOn() : parts.heading).focus({ preventScroll: true });
        mood(false);
    }

    /** Where a finished setup is going, and how it is getting there, in
        words (ending.js): drawn under the way on. */
    function following(words, url) {
        onward = { words, url };
        if (here() && page) drawWords(null);
    }

    /** The page has gone, and leaves nothing of itself behind. */
    function gone() {
        if (!parts) return;
        cancelAnimationFrame(frame);
        frame = 0;
        clearInterval(typing);
        typing = 0;
        parts = page = null;
        field.wake();
        field.even(0);
        stage.style.setProperty("--heat", 0);
        turn.classList.remove("finished", "stopped", "alone");
    }

    return { draw, focus, gone, following };
}
