// What the pages that show a disk have in common: how a size is worded, the
// pictures, and the disk drawn end to end as a bar.
//
// What a disk says of itself (its model, the labels of its filesystems) is
// whatever whoever made the disk wrote there. Nothing here puts any of it on
// a page as markup: `picture` is only ever given the constants below.

const KIB = 2 ** 10, MIB = 2 ** 20, GIB = 2 ** 30, TIB = 2 ** 40;
/** installerd's own wording of a size, so that a size reads the same where
    installerd words it and where a page does. */
export const sizeText = (bytes) =>
    bytes >= TIB ? `${(bytes / TIB).toFixed(1)} TiB`
    : bytes >= GIB ? `${(bytes / GIB).toFixed(1)} GiB`
    : bytes >= MIB ? `${Math.floor(bytes / MIB)} MiB`
    : `${Math.floor(bytes / KIB)} KiB`;

/** Said when what is pressed leads to a page that is not drawn yet: the
    installer holds such an action back, since an answer would move everyone
    looking on to a page nobody can see. */
export const NOT_DRAWN = "The step after this one is not drawn yet.";

export const listText =(xs) => xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`;
export const plural = (n, one) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** A picture from this program's own markup. Nothing that came from the
    installer goes through here. */
export function picture(markup) {
    const template = document.createElement("template");
    template.innerHTML = markup;
    return template.content.firstElementChild;
}
export const icon = (viewBox, width, body) =>
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
/** The picture of a disk attached by `bus`. */
export const glyph = (bus) => picture(GLYPHS[bus] ?? GLYPHS[""]);
export const ALERT = icon("0 0 16 16", 1.5, '<path d="M8 1.75 15 14.25H1z"/><path d="M8 6.5v3.25M8 12h.01"/>');
export const BACK = icon("0 0 18 18", 1.6, '<path d="M14.5 9h-11M8 4.5 3.5 9 8 13.5"/>');
export const ONWARD = icon("0 0 18 18", 1.6, '<path d="M3.5 9h11M10 4.5 14.5 9 10 13.5"/>');
export const AGAIN = icon("0 0 18 18", 1.6, '<path d="M3 9a6 6 0 1 0 1.8-4.3M3 3v3.5h3.5"/>');
export const SAVE = icon("0 0 16 16", 1.6, '<path d="M8 2v8M4.75 7 8 10.25 11.25 7M2.5 12v1.5h11V12"/>');
export const POWER = icon("0 0 18 18", 1.6, '<path d="M9 2.5v6"/><path d="M5.2 4.6a6 6 0 1 0 7.6 0"/>');
export const CLOCK = icon("0 0 18 18", 1.6, '<circle cx="9" cy="9" r="6.5"/><path d="M9 5.5V9l2.5 1.5"/>');

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

// Puts `node` where the partition `p` lies on a bar mapped by `at`.
function placed(node, at, p) {
    const left = at(p.start);
    node.style.setProperty("--l", left.toFixed(3));
    node.style.setProperty("--w", (at(p.start + p.bytes) - left).toFixed(3));
    return node;
}

/**
 * A disk as an install is making it, from installerd's `detail` of it: only
 * what it becomes, each part with something inside it for the page to fill
 * as the part is filled. What was on the disk is not drawn: by now it is
 * going or gone.
 */
export function madeBar(el, detail) {
    const becomes = detail.becomes ?? [];
    const at = barMap(detail.bytes, becomes.map((b) => [b.start, b.bytes]), .05);
    const bar = el("div", "layout made", becomes.map((b) => placed(el("i", `ls r-${b.role}`, [el("i", "fill")]), at, b)));
    bar.setAttribute("role", "img");
    return bar;
}

/**
 * A disk end to end, from installerd's `detail` of it: what is on it, and
 * over that, where an install is what it is for, what the install makes of
 * it. How the second comes to be seen is the page's: it starts out of sight.
 *
 * `el` makes an element. `as` gives the class a partition is drawn with
 * where the disk is being kept; on one being installed onto they are all
 * drawn as going.
 */
export function diskBar(el, detail, as = () => "was") {
    const partitions = detail.partitions ?? [], becomes = detail.becomes ?? [];
    const at = barMap(detail.bytes, [...partitions, ...becomes].map((p) => [p.start, p.bytes]), .05);
    const piece = (p, className) => placed(el("i", `ls ${className}`), at, p);
    const bar = el("div", "layout", [
        el("div", "old", partitions.map((p) => piece(p, becomes.length ? "doomed" : as(p)))),
    ]);
    if (becomes.length) bar.append(el("div", "new", becomes.map((b) => piece(b, `r-${b.role}`))), el("div", "edge"));
    bar.setAttribute("role", "img");
    return bar;
}

/** Partitions in words, for whoever cannot see the bar: each by what it is
    called and how big. */
export const partitionsText = (partitions) =>
    partitions.length ? listText(partitions.map((p) => `${p.title}, ${sizeText(p.bytes)}`)) : "no partitions";

/** What the colours of a bar stand for: `items` are [class, title, what]. */
export const legend = (el, items) => el("ul", "legend", items.map(([className, title, what]) =>
    el("li", "", [el("i", className), el("b", "", title), el("span", "", what)])));

/** The legend of what an install makes of a disk. */
export const becomesLegend = (el, becomes) =>
    legend(el, becomes.map((b) => [`r-${b.role}`, b.title, `${sizeText(b.bytes)} · ${b.fs}`]));
