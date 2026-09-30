// A restart, from the page's side.
//
// installerd restarts the machine, and this process goes down with it. The
// page does not: it is a browser on another device, and it is what is left to
// say what happens next. It shows the machine going (the page leaves and the
// stars gather back into the mark, as the intro made it), waits while nothing
// answers, and lands on whatever does:
//
// - Peios, from the disk it was installed on. Its first-boot setup is on the
//   machine's own screen, since there is none in a browser yet.
// - The installer again: the machine started from the medium, not the disk.
// - Nothing, for long enough to say so, with what the machine's own screen
//   might be showing.
//
// What answers is told apart by asking the address for `/hello`, which only
// this installer answers, with the boot of the machine it is on. The same
// installer on the same boot is the machine that has not gone yet. Anything
// else counts only once nothing at all has answered, so that the rest of the
// live system, still answering as it goes down, is not taken for Peios.
//
// Nothing here goes back to the installer's state: once the machine is going,
// what the installer said is over.
import { AGAIN, CLOCK, ONWARD, picture } from "./bits.js";

// How often the address is asked, how long one asking may take, and how long
// before the page says the machine has not come back.
const EVERY = 1500, ASKING = 4000, PATIENCE = 3 * 60 * 1000;
// The going down, in ms from the restart being taken.
const T = { centre: 480, formed: 2450, drawn: 2700, fade: 3400, away: 4000 };
// Coming back: the burst, the name, and the landing.
const BACK = { name: 500, land: 1900 };

/**
 * `els` are the page's fixed parts (stage, page, turn, ticker, status,
 * statusText, introLockup); `el` and `rise` make a page's parts; `field` is
 * the stars and `intro` the lockup's moves; `say` and `retitle` speak and name
 * the tab.
 */
export function createRestart({ els, el, rise, field, intro, say, retitle, reduced }) {
    const { stage, page, turn, ticker, status, statusText, introLockup } = els;
    let active = false;
    // Where it has got to: "down" (going, and then waiting), "back" (it
    // answered, and the page is on its way in), "lost" (nothing answered in
    // time), or a page landed on: "running" or "again".
    let at = "";
    let timers = [];
    let asking = 0;
    let away = false;       // the going down has finished: the mark is asleep
    let gone = false;       // nothing at all has answered since the restart
    let boot = "";          // the boot the restart was asked on
    let since = 0;          // when the waiting began
    let disk = null;        // what Peios was installed on, as the job said
    let closedSaid = false;
    let answer = null;      // what came back, while the going down finishes

    const later = (ms, fn) => timers.push(setTimeout(fn, reduced ? 0 : ms));
    const stop = () => { timers.forEach(clearTimeout); timers = []; clearTimeout(asking); };

    // A line in the corner, as the intro's were. The last is the one being
    // done; `waiting` gives it the dots of something still under way.
    function line(what, to, waiting = false) {
        for (const li of ticker.querySelectorAll(".waiting")) li.classList.remove("waiting");
        ticker.append(el("li", waiting ? "waiting" : "", [el("span", "k", what), ` ${to}`]));
        while (ticker.children.length > 5) ticker.firstChild.remove();
    }

    function statusOf(text, how) {
        statusText.textContent = text;
        status.classList.toggle("bad", how === "bad");
        status.classList.toggle("wait", how === "wait");
    }

    /**
     * The restart has been taken and the machine is going. `boot` is the
     * boot it was asked on, `installed` the disk the job was about, if it
     * said, and `leave` clears away the page that was showing.
     */
    function begin({ boot: asked, installed, leave }) {
        if (active) return;
        active = true;
        boot = asked;
        disk = installed ?? null;
        down(leave);
    }

    // The page leaves, the lockup comes back to the middle, the stars gather
    // into the mark and fade into it, and it sleeps.
    function down(leave) {
        stop();
        at = "down";
        away = gone = false;
        answer = null;
        since = Date.now();
        ticker.replaceChildren();
        stage.classList.add("rebooting");
        stage.classList.remove("stalled");
        page.classList.remove("live");
        turn.classList.remove("in", "first");
        turn.classList.add("out");
        line("reboot", "asked of installerd");
        say("Restarting the machine.");
        retitle("Restarting");
        later(T.centre, () => {
            leave?.();
            turn.classList.remove("out");
            turn.replaceChildren();
            turn.dataset.kind = "restart";
            intro.recall();
            field.regather();
        });
        later(T.formed, () => introLockup.classList.add("formed"));
        later(T.drawn, () => introLockup.classList.add("drawn"));
        later(T.fade, () => field.fade());
        later(T.away, () => {
            field.darken();
            introLockup.classList.add("sleeping");
            stage.classList.remove("aurora-on");
            line("waiting", "for the machine to come back", true);
            say("Waiting for the machine to come back.");
            away = true;
            // It answered while this was under way, and waited for it.
            if (answer) comeBack(answer);
        });
        ask();
    }

    /** The connection to the installer has closed, as it does. */
    function closed() {
        if (!active || closedSaid) return;
        closedSaid = true;
        if (at === "down") line("closed", "the connection to the installer", !away);
    }

    // Asks the address who is there. Nothing, if nothing answers in time.
    async function hello() {
        const stopAsking = new AbortController();
        const late = setTimeout(() => stopAsking.abort(), ASKING);
        try {
            const response = await fetch("/hello", { cache: "no-store", redirect: "manual", signal: stopAsking.signal });
            let said = null;
            if (response.ok) {
                try { said = await response.json(); } catch { said = null; }
            }
            return { answered: true, installer: typeof said?.installer === "string", boot: said?.boot };
        } catch {
            return { answered: false };
        } finally {
            clearTimeout(late);
        }
    }

    async function ask() {
        const heard = await hello();
        if (!active || (at !== "down" && at !== "lost")) return;
        let back = null;
        if (!heard.answered) gone = true;
        else if (heard.installer) back = heard.boot === boot ? null : "again";
        else if (gone) back = "running";
        if (back) {
            if (at === "lost") return landFromLost(back);
            answer = back;
            if (away) return comeBack(back);
        } else if (at === "down" && away && Date.now() - since > PATIENCE) {
            return lost();
        }
        asking = setTimeout(ask, EVERY);
    }

    // It answered: the mark bursts as the intro's did, the name comes back
    // beside it, and the lockup takes its corner over the page landed on.
    function comeBack(what) {
        stop();
        at = "back";
        line("answered", location.host);
        introLockup.classList.remove("sleeping");
        introLockup.classList.add("popped");
        stage.classList.add("aurora-on");
        field.burst();
        later(BACK.name, () => introLockup.classList.add("word-in"));
        later(BACK.land, () => land(what));
    }

    // A page arrives under the lockup as it glides to the corner.
    function land(what) {
        at = what;
        fill(what);
        turn.classList.remove("out");
        turn.classList.add("first");
        void turn.offsetWidth;
        turn.classList.add("in");
        intro.reland(() => {
            stage.classList.remove("rebooting");
            page.classList.add("live");
            arrived();
        });
    }

    // Nothing came back in time. The page says so over a dim, stopped field,
    // and goes on asking, more slowly than before.
    function lost() {
        stop();
        const minutes = Math.round((Date.now() - since) / 60000);
        line("waiting", `${minutes} minutes, and nothing has answered`);
        field.stall();
        stage.classList.add("stalled");
        introLockup.classList.remove("sleeping");
        land("lost");
        asking = setTimeout(ask, EVERY * 2);
    }

    // It answered while the page was saying it had not.
    function landFromLost(what) {
        stop();
        stage.classList.remove("stalled");
        field.wake();
        turn.classList.remove("in", "first");
        turn.classList.add("out");
        later(420, () => {
            at = what;
            fill(what);
            turn.classList.remove("out");
            void turn.offsetWidth;
            turn.classList.add("in");
            arrived();
        });
    }

    // Keep waiting: back to the middle, the mark asleep, asking again.
    function waitAgain() {
        stop();
        turn.classList.remove("in", "first");
        turn.classList.add("out");
        later(420, () => {
            turn.classList.remove("out");
            turn.replaceChildren();
            stage.classList.remove("stalled");
            stage.classList.add("rebooting");
            page.classList.remove("live");
            intro.recall();
            introLockup.classList.add("formed", "drawn", "sleeping");
            field.darken();
            ticker.replaceChildren();
            line("waiting", "for the machine to come back", true);
            say("Waiting for the machine to come back.");
            at = "down";
            away = true;
            answer = null;
            since = Date.now();
            ask();
        });
    }

    function arrived() {
        const heading = turn.querySelector("h1")?.textContent.trim() ?? "";
        retitle(heading);
        say([heading, turn.querySelector(".lede")?.textContent].filter(Boolean).join(". "));
        (turn.querySelector(".btn") ?? turn.querySelector("h1"))?.focus({ preventScroll: true });
        statusOf(...{
            running: [`Peios is running at ${location.hostname}`, ""],
            again: ["The installer is running again", "wait"],
            lost: ["Waiting for the machine", "wait"],
        }[at] ?? ["", ""]);
    }

    const where = () => (disk?.device ? `${disk.model ? `${disk.model} (${disk.device})` : disk.device}` : "the disk it was installed on");

    function fill(what) {
        turn.dataset.kind = what;
        const button = (className, svg, name, onClick) => {
            const b = el("button", `btn ${className}`, [picture(svg), name], { type: "button" });
            b.addEventListener("click", onClick);
            return b;
        };
        if (what === "running") {
            turn.replaceChildren(
                rise(0, "h1", "", "Peios is running"),
                rise(1, "p", "lede", `The machine restarted from ${where()}.`),
                rise(2, "p", "lede", "First-boot setup is waiting on the machine's own screen: a language and keyboard, the network, "
                    + "your account and a name for the machine. Once it is done, sign in here with the account you made."),
                rise(3, "div", "nav ends", [button("go-on", ONWARD, "Sign in", () => location.assign("/"))]),
            );
        } else if (what === "again") {
            turn.replaceChildren(
                rise(0, "h1", "", "It started the installer again"),
                rise(1, "p", "lede", `The machine started from the install medium instead of ${where()}. `
                    + "Peios is installed there, and its first-boot setup has not run."),
                rise(2, "p", "lede", "Take the medium out and restart the machine, or choose the disk from the firmware's boot menu "
                    + "as it starts: often F12, F11 or Esc."),
                rise(3, "div", "nav ends", [button("quiet", AGAIN, "Back to the installer", () => location.reload())]),
            );
        } else {
            const minutes = Math.max(1, Math.round((Date.now() - since) / 60000));
            const address = el("input", "input", "", {
                id: "lost-addr", placeholder: "192.168.1.61", spellcheck: "false", autocomplete: "off",
                "aria-label": "The machine's address", "aria-describedby": "lost-err",
            });
            const wrong = el("p", "err", "", { id: "lost-err" });
            const go = () => {
                const to = address.value.trim();
                // An address or a name, and nothing a URL would take for more.
                if (!/^[A-Za-z0-9.-]+$/.test(to)) {
                    address.setAttribute("aria-invalid", "true");
                    wrong.textContent = "An address, like 192.168.1.61, or the machine's name.";
                    return address.focus();
                }
                location.assign(`${location.protocol}//${to}${location.port ? `:${location.port}` : ""}/`);
            };
            address.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); go(); } });
            address.addEventListener("input", () => { address.removeAttribute("aria-invalid"); wrong.textContent = ""; });
            const cases = [
                ["A message about Secure Boot", "The firmware won't start Peios' boot file, because it isn't signed with a key the firmware trusts. "
                    + "Turn Secure Boot off in the firmware's settings, then restart."],
                ["“No bootable device”, or nothing at all", "The firmware isn't trying the disk. Open its boot menu as the machine starts "
                    + "(often F12, F11 or Esc) and choose the disk Peios is on."],
                ["“Welcome to Peios”", "Peios is up, and its first-boot setup is waiting there. It may have come back at another address; "
                    + "if you know it, go there:"],
            ].map(([title, body], i) => el("li", "", [
                el("b", "", title),
                el("p", "", body),
                ...(i === 2 ? [el("div", "addr", [address, button("quiet sm", ONWARD, "Go", go)]), wrong] : []),
            ]));
            turn.replaceChildren(
                el("div", "col", [
                    rise(0, "h1", "", "The machine hasn't come back"),
                    rise(1, "p", "lede", `It was asked to restart ${minutes === 1 ? "a minute" : `${minutes} minutes`} ago, and nothing has answered `
                        + `at ${location.hostname} since. This page runs on another device, so it can't see the machine's screen, `
                        + "and what that screen shows says what happened."),
                    rise(2, "div", "nav ends", [button("go-on", CLOCK, "Keep waiting", waitAgain)]),
                ]),
                rise(3, "aside", "", [el("p", "plan-label", "What the machine's screen might show"), el("ol", "cases", cases)]),
            );
        }
    }

    return { begin, closed, get active() { return active; } };
}
