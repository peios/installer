// First-boot setup's network page: what the machine's network is.
//
// oobed says what there is: a sentence for the machine, and under its
// `detail` each interface as netd has it (where it has got to, its addresses,
// its gateway and name servers, its hardware). Nothing on the page has to be
// answered. Checking again asks oobed to ask netd again, and the page changes
// in place for everyone looking; what it would take to join a wireless
// network or address one by hand oobed cannot do yet, and says why.
//
// Someone reading this page reached the machine over one of its interfaces,
// by one of its addresses. Where the address in this page's own location is
// one an interface has, that interface is marked as the way in: whatever
// would be changed on it is what this page is carried over.
import { BACK, NOT_DRAWN, ONWARD, icon, picture } from "./bits.js";

const RESCAN = icon("0 0 16 16", 1.6, '<path d="M13.5 8a5.5 5.5 0 1 1-1.7-4"/><path d="M13.5 2.5v3h-3"/>');
const WIFI = icon("0 0 16 16", 1.6, '<path d="M1.75 6a9 9 0 0 1 12.5 0M4 8.5a5.75 5.75 0 0 1 8 0M6.25 11a2.5 2.5 0 0 1 3.5 0"/><path d="M8 13.25h.01"/>');
const MANUAL = icon("0 0 16 16", 1.6, '<path d="M2.5 4.5h6M11.5 4.5h2M2.5 11.5h2M7.5 11.5h6"/><circle cx="10" cy="4.5" r="1.5"/><circle cx="6" cy="11.5" r="1.5"/>');
const big = (body) => icon("0 0 24 24", 1.5, body);
// An interface, by what kind it is: a wireless one by its name (`wl…`), any
// other as a port a cable goes into.
const PORT = big('<rect x="3.5" y="5.5" width="17" height="13" rx="2.5"/><path d="M8 18.5v-4h8v4M10 14.5v-2h4v2"/>');
const RADIO = big('<path d="M3 9.5a13 13 0 0 1 18 0M6 12.75a8.5 8.5 0 0 1 12 0M9 16a4 4 0 0 1 6 0"/><path d="M12 19.25h.01"/>');
const OTHERS = { "network.wifi": WIFI, "network.static": MANUAL };

// Where an interface has got to, in a word or two, and how it is shown:
// `ok`, `wait`, or nothing much.
const STATES = {
    connected: ["Connected", "ok"],
    local: ["This network only", "wait"],
    connecting: ["Connecting", "wait busy"],
    unplugged: ["Not connected", ""],
    unused: ["Not used", ""],
    off: ["Turned off", ""],
};

/** An address without the length of its prefix, and without the brackets
    an IPv6 address has in a location. */
const bare = (address) => address.replace(/\/\d+$/, "").replace(/^\[(.*)\]$/, "$1").toLowerCase();

/**
 * The network page. `turn` is where a page is drawn; `el` and `rise` make its
 * parts; `ask` asks setup for something; `toast` says something in passing.
 */
export function createNetworkPage({ turn, el, rise, ask, toast, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what setup last said is on it
    let listAs = "", othersAs = "";
    let hint = "";      // why something cannot be pressed, while it is pointed at
    let checking = 0, checkTimer = 0;

    function button(className, content, press) {
        const node = el("button", className, content, { type: "button" });
        node.addEventListener("click", press);
        return node;
    }

    function build() {
        parts = {
            title: rise(0, "h1", ""),
            lede: rise(1, "p", "lede"),
            list: el("div", "nets"),
            words: rise(2, "p", "net-words"),
            note: rise(4, "p", "note"),
            refresh: button("link", [picture(RESCAN), el("span")], () => page.refresh?.enabled && ask({ press: page.refresh.ref })),
            others: el("span", "others"),
            help: el("p", "help", "", { id: "network-help", "aria-live": "polite" }),
            back: button("btn quiet", [picture(BACK), el("span")], () => page.back?.enabled && ask({ press: page.back.ref })),
            next: button("btn go-on", [el("span"), picture(ONWARD)], () => {
                if (!page.next?.enabled) return say(page.next?.help ?? "");
                // The page it leads to is not drawn yet, and an answer would
                // take everyone looking to it.
                if (page.next.unbuilt) return toast(NOT_DRAWN);
                ask({ press: page.next.ref });
            }),
        };
        turn.replaceChildren(
            parts.title, parts.lede, parts.list, parts.words, parts.note,
            rise(5, "div", "aux", [parts.refresh, parts.others, parts.help]),
            rise(6, "div", "nav", [parts.back, parts.next]),
        );
        listAs = othersAs = hint = "";
    }

    function say(text) {
        hint = text;
        parts.help.textContent = text;
        parts.help.classList.toggle("on", !!text);
    }

    // What else oobed offers, each saying why it cannot be pressed when
    // pointed at, and what it leads to not being drawn when it can.
    function drawOthers() {
        const as = JSON.stringify(page.others);
        if (as === othersAs) return;
        othersAs = as;
        parts.others.replaceChildren(...page.others.map((action) => {
            const why = () => say(action.enabled ? "" : action.help ?? "");
            const node = button("link", [picture(OTHERS[action.ref] ?? MANUAL), el("span", "", action.name)], () => {
                if (!action.enabled) return why();
                if (action.unbuilt) return toast(NOT_DRAWN);
                ask({ press: action.ref });
            });
            node.setAttribute("aria-describedby", "network-help");
            if (!action.enabled) node.setAttribute("aria-disabled", "true");
            for (const on of ["pointerenter", "focus"]) node.addEventListener(on, why);
            return node;
        }));
    }

    // One interface: what it is, where it has got to, and what it has.
    function row(net, n, here) {
        const [said, how] = STATES[net.state] ?? [net.state, ""];
        const wireless = /^wl/.test(net.name);
        const facts = [];
        if (net.addresses?.length) facts.push(el("span", "", net.addresses.join(" · ")));
        if (net.gateway) facts.push(el("span", "", `via ${net.gateway}`));
        if (net.dns?.length) facts.push(el("span", "", `DNS ${net.dns.join(", ")}`));
        const hardware = [net.hardware, net.driver].filter(Boolean).join(" · ");
        const node = rise(2 + Math.min(n, 3), "div", `net ${how}`.trim(), [
            el("span", "glyph", [picture(wireless ? RADIO : PORT)]),
            el("span", "who", [
                el("span", "model", [
                    net.name,
                    ...(net.network ? [el("span", "net-on", ` on ${net.network}`)] : []),
                ]),
                el("span", "where", [
                    ...(hardware ? [el("span", "", hardware)] : []),
                    ...(here ? [el("span", "chip here", "Your way in")] : []),
                ]),
                ...(facts.length ? [el("span", "net-facts", facts)] : []),
                ...(net.warning ? [el("span", "net-warn", net.warning)] : []),
            ]),
            el("span", "net-state", [el("i", "net-dot"), el("span", "", said)]),
        ], { role: "listitem", "aria-label": `${net.name}: ${said}${here ? ", the way this page reaches the machine" : ""}` });
        return node;
    }

    function drawList() {
        const nets = page.interfaces ?? [];
        const host = bare(location.hostname);
        const as = JSON.stringify([nets, host]);
        if (as === listAs) return;
        listAs = as;
        parts.list.setAttribute("role", "list");
        parts.list.setAttribute("aria-label", "Network interfaces");
        parts.list.replaceChildren(...nets.map((net, n) => row(net, n, (net.addresses ?? []).some((a) => bare(a) === host))));
        parts.list.hidden = !nets.length;
    }

    // While oobed asks netd again a light passes down the list, for at least
    // one pass however quick it was.
    function drawChecking(waiting) {
        const under = waiting && waiting === page.refresh?.ref;
        if (under && !checking) {
            checking = Date.now();
            clearTimeout(checkTimer);
            parts.list.classList.add("scanning");
            parts.refresh.classList.add("spin");
        } else if (!under && checking) {
            const left = reduced ? 0 : Math.max(0, 900 - (Date.now() - checking));
            checking = 0;
            const { list, refresh } = parts;
            checkTimer = setTimeout(() => { list.classList.remove("scanning"); refresh.classList.remove("spin"); }, left);
        }
    }

    /** Draws `next`, the page as setup says it now is, changing only what
        changed; `waiting` is what was pressed and not yet answered. */
    function draw(next, waiting) {
        page = next;
        if (!parts || !parts.title.isConnected) build();
        parts.title.textContent = page.title;
        // oobed's first line is the machine; the rest, a line an interface,
        // is what the list draws. Where there is no list, its words are
        // what there is.
        const [first, ...rest] = page.status.split("\n");
        parts.lede.textContent = first;
        parts.words.textContent = rest.join("\n");
        parts.words.hidden = !!page.interfaces || !rest.length;
        parts.note.textContent = page.note;
        parts.note.hidden = !page.note;
        drawList();
        const name = (node, action) => {
            node.hidden = !action;
            if (!action) return;
            node.querySelector("span").textContent = action.name;
            if (action.enabled) node.removeAttribute("aria-disabled");
            else node.setAttribute("aria-disabled", "true");
        };
        name(parts.refresh, page.refresh);
        name(parts.back, page.back);
        name(parts.next, page.next);
        drawOthers();
        drawChecking(waiting);
    }

    /** The page is showing: the keyboard starts on Next, since nothing here
        has to be answered. */
    function focus() {
        parts?.next.focus({ preventScroll: true });
    }

    return { draw, focus };
}
