// The disk page: which disk, and beside the disks what the chosen one will
// become, or what is on it.
//
// The installer says what there is: the disks, which is chosen, and under
// each disk's `detail` what installerd knows of it (its partitions, what an
// install would make of it, the system on it). This draws that, and asks the
// installer for whatever is pressed. Nothing is decided here: a disk is
// chosen when the installer says it is, which is how two people looking at
// once see the same one chosen.
//
// What a disk says of itself (its model, the labels of its filesystems) is
// whatever whoever made the disk wrote there. All of it is put on the page as
// text, never as markup.

const KIB = 2 ** 10, MIB = 2 ** 20, GIB = 2 ** 30, TIB = 2 ** 40;
/** installerd's own wording of a size, so that a size reads the same in a
    row, which it words, and in the panel, which this does. */
export const sizeText = (bytes) =>
    bytes >= TIB ? `${(bytes / TIB).toFixed(1)} TiB`
    : bytes >= GIB ? `${(bytes / GIB).toFixed(1)} GiB`
    : bytes >= MIB ? `${Math.floor(bytes / MIB)} MiB`
    : `${Math.floor(bytes / KIB)} KiB`;

const listText = (xs) => xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`;
const plural = (n, one) => `${n} ${one}${n === 1 ? "" : "s"}`;
/** `/dev/sda2` as a person says it. */
const short = (device) => device.replace(/^\/dev\//, "");

/** A picture from this file's own markup. Only ever given the constants
    below: nothing that came from the installer goes through here. */
function picture(markup) {
    const template = document.createElement("template");
    template.innerHTML = markup;
    return template.content.firstElementChild;
}
const icon = (viewBox, width, body) =>
    `<svg viewBox="${viewBox}" fill="none" stroke="currentColor" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const big = (body) => icon("0 0 24 24", 1.5, body);
// A disk, by how it is attached, as installerd names the bus.
const GLYPHS = {
    "NVMe": big('<rect x="2.5" y="8" width="19" height="8" rx="1.5"/><path d="M5.5 16v2M8.5 16v2M11.5 16v2M14.5 16v2"/><rect x="6" y="10.5" width="5" height="3" rx=".5"/><circle cx="17.5" cy="12" r="1"/>'),
    "SATA": big('<rect x="3.5" y="3.5" width="17" height="17" rx="3"/><circle cx="12" cy="11" r="4.5"/><circle cx="12" cy="11" r=".9"/><path d="M7 17.5h2.5"/>'),
    "USB": big('<rect x="7" y="9" width="10" height="12.5" rx="2"/><path d="M9 9V3.5h6V9"/><path d="M10.5 6h.01M13.5 6h.01"/>'),
    "SD/MMC": big('<path d="M7 3.5h8l4 4v12a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1z"/><path d="M9.5 6.5V9M12 6.5V9M14.5 6.5V9"/>'),
    "": big('<rect x="3.5" y="5.5" width="17" height="13" rx="2.5"/><path d="M7 15h3"/>'),
};
const TICK = icon("0 0 12 12", 2, '<path d="M2.5 6.2 5 8.5l4.5-5"/>');
const ALERT = icon("0 0 16 16", 1.5, '<path d="M8 1.75 15 14.25H1z"/><path d="M8 6.5v3.25M8 12h.01"/>');
const RESCAN = icon("0 0 16 16", 1.6, '<path d="M13.5 8a5.5 5.5 0 1 1-1.7-4"/><path d="M13.5 2.5v3h-3"/>');
const CUSTOM = icon("0 0 16 16", 1.6, '<rect x="2" y="3.5" width="12" height="9" rx="2"/><path d="M6.5 3.5v9"/>');
const BACK = icon("0 0 18 18", 1.6, '<path d="M14.5 9h-11M8 4.5 3.5 9 8 13.5"/>');
const ONWARD = icon("0 0 18 18", 1.6, '<path d="M3.5 9h11M10 4.5 14.5 9 10 13.5"/>');

// Where an offset lands on a bar, in percent. Partitions get a floor of room
// so the small ones can be seen (an EFI system partition is a two-thousandth
// of a terabyte disk); the gaps between them get only their true share; the
// rest is shared out in proportion. `spans` are [start, length] pairs.
function barMap(total, spans, floor) {
    const xs = [...new Set([0, total, ...spans.flatMap(([at, length]) => [at, at + length])])]
        .filter((x) => x >= 0 && x <= total)
        .sort((a, b) => a - b);
    const covered = (x) => spans.some(([at, length]) => x > at && x < at + length);
    const widths = [];
    for (let i = 1; i < xs.length; i++) {
        const length = xs[i] - xs[i - 1];
        widths.push(covered((xs[i] + xs[i - 1]) / 2) ? Math.max(length / total, floor) : length / total);
    }
    const sum = widths.reduce((a, b) => a + b, 0) || 1;
    const stops = [0];
    for (const width of widths) stops.push(stops[stops.length - 1] + width / sum * 100);
    return (x) => {
        let i = 0;
        while (i < xs.length - 2 && x > xs[i + 1]) i++;
        return stops[i] + (x - xs[i]) / (xs[i + 1] - xs[i] || 1) * (stops[i + 1] - stops[i]);
    };
}

// What a person would miss if the disk were erased: a system, by name, then
// anything with files on it. What a machine starts from or repairs itself
// with is not missed in its own right.
const missed = (partitions) => partitions
    .filter((p) => p.fs && p.used && p.type !== "esp" && p.type !== "winre")
    .map((p) => p.holds || `“${p.title}” (${sizeText(p.used)} in use)`);

/**
 * The disk page. `turn` is where a page is drawn; `el` and `rise` make its
 * parts; `ask` asks the installer for something; `toast` says something in
 * passing.
 */
export function createDiskPage({ turn, el, rise, ask, toast, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what the installer last said is on it
    let rowsAs = "", planAs = "";
    let hint = "";      // something said in passing, until a disk is chosen
    let scanning = 0, scanTimer = 0, wipeTimer = 0;

    const rows = () => [...parts.disks.querySelectorAll(".disk")];
    const enabled = () => rows().filter((row) => row.getAttribute("aria-disabled") !== "true");
    const chosen = () => page.disks.find((disk) => disk.value === page.chosen);

    function button(className, content, press) {
        const node = el("button", className, content, { type: "button" });
        node.addEventListener("click", press);
        return node;
    }

    function build() {
        const say = (text) => { hint = text; drawHelp(); };
        const custom = button("link", [picture(CUSTOM), el("span")], () => say(page.custom?.help ?? ""));
        custom.setAttribute("aria-describedby", "disk-help");
        for (const on of ["pointerenter", "focus"]) custom.addEventListener(on, () => say(page.custom?.help ?? ""));
        parts = {
            title: rise(0, "h1", ""),
            lede: rise(1, "p", "lede"),
            disks: el("div", "disks", null, { role: "radiogroup" }),
            trouble: rise(5, "p", "trouble", [picture(ALERT), el("span")]),
            rescan: button("link", [picture(RESCAN), el("span")], () => page.rescan?.enabled && ask({ press: page.rescan.ref })),
            custom,
            help: el("p", "help", "", { id: "disk-help", "aria-live": "polite" }),
            back: button("btn quiet", [picture(BACK), el("span")], () => page.back?.enabled && ask({ press: page.back.ref })),
            next: button("btn go-on", [el("span"), picture(ONWARD)], () => {
                if (!page.next?.enabled) return say(page.next?.help ?? "");
                if (!page.chosen) return say("Choose a disk first.");
                // The page this leads to is not drawn yet, and an answer
                // would take everyone looking to it.
                if (page.next.unbuilt) return toast("The step after this one is not drawn yet.");
                ask({ press: page.next.ref });
            }),
            label: el("p", "plan-label"),
            body: el("div", "plan-body"),
        };
        parts.disks.addEventListener("keydown", keys);
        turn.replaceChildren(
            el("div", "col", [
                parts.title, parts.lede, parts.disks, parts.trouble,
                rise(6, "div", "aux", [parts.rescan, parts.custom, parts.help]),
                rise(7, "div", "nav", [parts.back, parts.next]),
            ]),
            rise(3, "aside", "plan", [parts.label, parts.body], { "aria-live": "polite" }),
        );
        rowsAs = planAs = hint = "";
    }

    // One disk: what it is and how big, then where it is and what is on it.
    function row(disk, n) {
        const partitions = disk.detail?.partitions ?? [];
        const system = partitions.find((p) => p.holds && p.type !== "esp")?.holds;
        const has = disk.enabled ? system ?? (partitions.length ? plural(partitions.length, "partition") : "") : "";
        const node = el("button", "disk", [
            el("span", "glyph", [picture(GLYPHS[disk.bus] ?? GLYPHS[""])]),
            el("span", "who", [
                el("span", "model", disk.model),
                el("span", "where", [
                    [disk.device, disk.bus, has].filter(Boolean).join(" · "),
                    ...(disk.note ? [el("span", "chip", disk.note)] : []),
                ], { id: `disk-where-${n}` }),
            ]),
            el("span", "size", disk.size),
            el("span", "tick", [picture(TICK)]),
        ], {
            type: "button", role: "radio",
            // Named by what it is and how big; the rest describes it.
            "aria-label": `${disk.model}, ${disk.size}`, "aria-describedby": `disk-where-${n}`,
        });
        node.dataset.value = disk.value;
        if (!disk.enabled) node.setAttribute("aria-disabled", "true");
        node.addEventListener("click", () => disk.enabled && choose(disk.value));
        const wrap = rise(2 + Math.min(n, 3), "div", "", [node]);
        return wrap;
    }

    function choose(value) {
        hint = "";
        if (value !== page.chosen) ask({ choose: value });
    }

    function drawRows() {
        const as = JSON.stringify([page.disks, page.empty]);
        if (as !== rowsAs) {
            // A rescan replaces the rows; the keyboard stays where it was.
            const held = document.activeElement?.closest?.(".disk")?.dataset.value;
            parts.disks.replaceChildren(...page.disks.map(row));
            if (!page.disks.some((disk) => disk.enabled)) {
                parts.disks.append(rise(3, "p", "no-disks", page.empty || "No disks found."));
            }
            rowsAs = as;
            if (held !== undefined) (rows().find((r) => r.dataset.value === held) ?? enabled()[0])?.focus({ preventScroll: true });
        }
        // A radio group is one stop for Tab: the chosen disk, or the first
        // that can be chosen. The arrows move between them.
        const stop = enabled().find((r) => r.dataset.value === page.chosen) ?? enabled()[0];
        for (const r of rows()) {
            r.setAttribute("aria-checked", String(r.dataset.value === page.chosen));
            r.tabIndex = r === stop ? 0 : -1;
        }
    }

    function keys(e) {
        const at = e.target.closest?.(".disk");
        if (!at || e.metaKey || e.ctrlKey || e.altKey) return;
        const all = enabled(), i = all.indexOf(at);
        const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
        if (step && all.length) {
            e.preventDefault();
            const to = all[(i + step + all.length) % all.length];
            to.focus();
            choose(to.dataset.value);
        } else if (e.key === "Enter" && at.dataset.value === page.chosen) {
            // Enter on the disk already chosen goes on with it.
            e.preventDefault();
            parts.next.click();
        }
    }

    function drawHelp() {
        // installerd's word for an answer with no disk in it is "required",
        // which is its to say and ours to put as a sentence.
        const text = hint || (page.error === "required" ? "Choose a disk first." : page.error ?? "");
        parts.help.textContent = text;
        parts.help.classList.toggle("on", !!text);
    }

    // While installerd looks at the disks again a light passes down the list,
    // for at least one pass however quick it was.
    function drawScanning(waiting) {
        const under = waiting && waiting === page.rescan?.ref;
        if (under && !scanning) {
            scanning = Date.now();
            clearTimeout(scanTimer);
            parts.disks.classList.add("scanning");
            parts.rescan.classList.add("spin");
        } else if (!under && scanning) {
            const left = reduced ? 0 : Math.max(0, 900 - (Date.now() - scanning));
            scanning = 0;
            const { disks, rescan } = parts;
            scanTimer = setTimeout(() => { disks.classList.remove("scanning"); rescan.classList.remove("spin"); }, left);
        }
    }

    // ---- the panel beside the disks ----
    function layout(detail, disk) {
        const partitions = detail.partitions ?? [], becomes = detail.becomes ?? [];
        const at = barMap(detail.bytes, [...partitions, ...becomes].map((p) => [p.start, p.bytes]), .05);
        const piece = (p, className) => {
            const node = el("i", `ls ${className}`);
            const left = at(p.start);
            node.style.setProperty("--l", left.toFixed(3));
            node.style.setProperty("--w", (at(p.start + p.bytes) - left).toFixed(3));
            return node;
        };
        const bar = el("div", "layout", [
            el("div", "old", partitions.map((p) => piece(p, becomes.length ? "doomed" : role(p, detail)))),
        ]);
        if (becomes.length) {
            // What it becomes is drawn over what it is, and wiped into view.
            bar.append(el("div", "new", becomes.map((b) => piece(b, `r-${b.role}`))), el("div", "edge"));
            wipeTimer = setTimeout(() => bar.classList.add("wiped"), reduced ? 0 : 260);
        }
        bar.setAttribute("role", "img");
        bar.setAttribute("aria-label", becomes.length
            ? `${disk.device} as it will be: ${listText(becomes.map((b) => `${b.title}, ${sizeText(b.bytes)}`))}`
            : `${disk.device} as it is: ${partitions.length ? listText(partitions.map((p) => `${p.title}, ${sizeText(p.bytes)}`)) : "no partitions"}`);
        return bar;
    }
    // A partition of a disk that is being kept: the Peios system on it and
    // what that starts from are picked out, the rest is just there.
    const role = (p, detail) =>
        !detail.system ? "was" : p.device === detail.system.on ? "r-root" : p.type === "esp" ? "r-esp" : "was";

    const legend = (items) => el("ul", "legend", items.map(([className, title, what]) =>
        el("li", "", [el("i", className), el("b", "", title), el("span", "", what)])));

    function becoming(disk, detail) {
        const out = [];
        if (detail?.becomes) {
            out.push(layout(detail, disk), legend(detail.becomes.map((b) => [`r-${b.role}`, b.title, `${sizeText(b.bytes)} · ${b.fs}`])));
        }
        const partitions = detail?.partitions ?? [];
        const lost = missed(partitions);
        const including = lost.length ? `, including ${listText(lost)}`
            : partitions.length ? `, including its ${plural(partitions.length, "partition")}` : "";
        out.push(el("p", "erase", [picture(ALERT), el("span", "", `Everything on ${disk.device} will be erased${including}.`)]));
        if (detail?.becomes) {
            out.push(el("p", "note", "The root is formatted with its security descriptor already in place, so it is protected from the moment it exists."));
        }
        return out;
    }

    function holding(disk, detail) {
        if (!detail) return [];
        const partitions = detail.partitions ?? [];
        const out = [layout(detail, disk)];
        if (partitions.length) {
            // The system's own two are called what they are to it; the rest
            // by what they call themselves.
            const called = { "r-root": "Peios root", "r-esp": "EFI system partition" };
            out.push(legend(partitions.map((p) => [
                role(p, detail), called[role(p, detail)] ?? p.title,
                `${p.used != null ? `${sizeText(p.used)} of ` : ""}${sizeText(p.bytes)}${p.fs ? ` · ${p.fs}` : ""}`,
            ])));
        }
        const { system, upgrade } = detail;
        if (system) {
            const on = partitions.find((p) => p.device === system.on);
            if (upgrade) {
                // What is on the disk, and what this medium would move it to.
                const side = (className, label, version) => el("div", className, [el("small", "", label), el("b", "", version)]);
                out.push(el("div", `rel mini${upgrade.blocked ? " alone" : ""}`, [
                    side("rel-v", "On the disk", system.version),
                    ...(upgrade.blocked ? [] : [el("div", "rel-line", [el("i")]), side("rel-v to", "This medium", upgrade.version)]),
                ]));
                out.push(el("p", "note", `${system.text}, on ${short(system.on)}.${upgrade.blocked ? ` ${upgrade.blocked}` : ""}`));
            } else {
                out.push(el("p", "note", [
                    el("b", "", system.text), `, on ${short(system.on)}${on?.used != null ? `, with ${sizeText(on.used)} in use` : ""}.`,
                ]));
            }
        } else if (detail.no_system) {
            out.push(el("p", "note", `No Peios system here: ${detail.no_system}.${page.purpose === "upgrade" ? " Nothing to upgrade." : ""}`));
        }
        return out;
    }

    function drawPlan() {
        const disk = chosen();
        const install = page.purpose !== "upgrade" && page.purpose !== "repair";
        const as = JSON.stringify([page.purpose, disk ?? null, page.disks.some((d) => d.enabled)]);
        parts.label.textContent = install ? "What the disk becomes" : "What's on the disk";
        if (as === planAs) return;
        planAs = as;
        clearTimeout(wipeTimer);
        if (!disk) {
            parts.body.replaceChildren(
                el("div", "layout empty"),
                el("p", "plan-empty", !page.disks.some((d) => d.enabled) ? "Nothing to show until a disk turns up."
                    : install ? "Choose a disk to see how it will be laid out." : "Choose a disk to see what is on it."),
            );
            return;
        }
        const detail = disk.detail;
        parts.body.replaceChildren(
            el("div", "plan-title", [el("b", "", disk.model), el("span", "", `${disk.device} · ${disk.size}`)]),
            ...(install ? becoming(disk, detail) : holding(disk, detail)),
        );
        parts.body.classList.remove("swap");
        void parts.body.offsetWidth;
        parts.body.classList.add("swap");
    }

    /** Draws the page as the installer says it is, changing only what changed. */
    function draw(next, waiting) {
        page = next;
        if (!parts || !parts.title.isConnected) build();
        parts.title.textContent = page.title;
        parts.lede.textContent = page.intro;
        parts.disks.setAttribute("aria-label", "Target disk");
        drawRows();
        parts.trouble.hidden = !page.trouble;
        parts.trouble.lastChild.textContent = page.trouble ?? "";
        const name = (node, action, text) => {
            node.hidden = !action;
            if (!action) return;
            node.querySelector("span").textContent = text ?? action.name;
            if (action.enabled) node.removeAttribute("aria-disabled");
            else node.setAttribute("aria-disabled", "true");
        };
        name(parts.rescan, page.rescan);
        // Laying a disk out by hand is for installing onto one.
        name(parts.custom, page.purpose === "install" || !page.purpose ? page.custom : null);
        name(parts.back, page.back);
        name(parts.next, page.next);
        if (page.next && !page.chosen) parts.next.setAttribute("aria-disabled", "true");
        drawHelp();
        drawScanning(waiting);
        drawPlan();
    }

    /** Where the keyboard starts on this page: the chosen disk, or the first
        that can be chosen. */
    function focus() {
        if (!parts) return;
        (enabled().find((r) => r.tabIndex === 0) ?? parts.rescan).focus({ preventScroll: true });
    }

    return { draw, focus };
}
