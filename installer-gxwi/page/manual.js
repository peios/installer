// First-boot setup's manual address: one interface given an address by hand.
//
// oobed says what there is to fill in: the interfaces (only a wired one can be
// chosen), the address, the gateway and the name servers, each with its
// example and why it would be turned down. What is typed and which interface
// is chosen stay in this browser until Save, and go to oobed with it: what one
// person is typing is not everyone's to see. oobed checks it, and either says
// on the field what is wrong, the page staying, or keeps it for the end of
// setup and goes back to the network page, which says so.
//
// Nothing is applied here. The page says as much where it can be seen: the
// address is given at the end of setup, after the account and the name, so
// the browser this is drawn in keeps reaching the machine until then.
import { BACK, CLOCK, icon, picture } from "./bits.js";

const TICK = icon("0 0 12 12", 2, '<path d="M2.5 6.2 5 8.5l4.5-5"/>');

/**
 * The manual page. `turn` is where a page is drawn; `el` and `rise` make its
 * parts; `ask` asks setup for something.
 */
export function createManualPage({ turn, el, rise, ask, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what setup last said is on it
    let chosen = null;  // the interface chosen here, until Save
    let rowsAs = "", errorsAs = "";

    function button(className, content, press) {
        const node = el("button", className, content, { type: "button" });
        node.addEventListener("click", press);
        return node;
    }

    const rows = () => [...parts.list.querySelectorAll(".iface")];

    function save() {
        if (!page.save?.enabled) return;
        const values = {};
        if (chosen) values["manual.interface"] = chosen;
        for (const [ref, { input }] of Object.entries(parts.fields)) values[ref] = input.value;
        ask({ press: page.save.ref, values });
    }

    function build() {
        parts = {
            title: rise(0, "h1", ""),
            lede: rise(1, "p", "lede"),
            when: rise(1, "p", "when", [picture(CLOCK), el("span", "", "Applied when setup finishes, not now.")]),
            list: el("div", "ifaces", null, { role: "radiogroup", "aria-label": "Interface" }),
            listError: el("p", "field-error", "", { id: "manual-interface-error" }),
            form: rise(3, "div", "fields"),
            fields: {},
            back: button("btn quiet", [picture(BACK), el("span")], () => page.back?.enabled && ask({ press: page.back.ref })),
            save: button("btn go-on", [el("span")], save),
        };
        parts.list.addEventListener("keydown", keys);
        turn.replaceChildren(
            parts.title, parts.lede, parts.when,
            rise(2, "div", "iface-wrap", [parts.list, parts.listError]),
            parts.form,
            rise(4, "div", "nav", [parts.back, parts.save]),
        );
        rowsAs = "";
        errorsAs = JSON.stringify([null, []]);
        chosen = null;
    }

    // One interface: what it is now, and whether it can be chosen.
    function row(iface) {
        const cells = iface.cells ?? {};
        const node = el("button", "iface", [
            el("span", "iface-name", cells.name ?? iface.value),
            el("span", "iface-now", [cells.state, cells.address].filter(Boolean).join(" · ")),
            ...(iface.note ? [el("span", "chip", iface.note)] : []),
            el("span", "tick", [picture(TICK)]),
        ], { type: "button", role: "radio" });
        node.dataset.value = iface.value;
        if (!iface.enabled) node.setAttribute("aria-disabled", "true");
        node.addEventListener("click", () => iface.enabled && choose(iface.value));
        return node;
    }

    function choose(value) {
        chosen = value;
        drawChosen();
    }

    function drawChosen() {
        const can = rows().filter((r) => r.getAttribute("aria-disabled") !== "true");
        const stop = can.find((r) => r.dataset.value === chosen) ?? can[0];
        for (const r of rows()) {
            r.setAttribute("aria-checked", String(r.dataset.value === chosen));
            r.tabIndex = r === stop ? 0 : -1;
        }
    }

    function keys(e) {
        const at = e.target.closest?.(".iface");
        const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
        if (!at || !step) return;
        e.preventDefault();
        const can = rows().filter((r) => r.getAttribute("aria-disabled") !== "true");
        const to = can[(can.indexOf(at) + step + can.length) % can.length];
        to?.focus();
        if (to) choose(to.dataset.value);
    }

    function drawRows() {
        const as = JSON.stringify(page.interfaces);
        if (as === rowsAs) return;
        rowsAs = as;
        parts.list.replaceChildren(...page.interfaces.map(row));
        if (!page.interfaces.length) parts.list.append(el("p", "no-disks", page.empty));
        // What oobed starts with, unless something here was chosen that is
        // still there to choose.
        const can = (value) => page.interfaces.some((i) => i.value === value && i.enabled);
        if (!can(chosen)) chosen = can(page.assumed) ? page.assumed : null;
    }

    // Each line to fill in, made once and kept, so that what is typed stays
    // while oobed says what is wrong with it.
    function drawFields() {
        for (const field of page.fields) {
            let made = parts.fields[field.ref];
            if (!made) {
                const id = `field-${field.ref.replace(/\W/g, "-")}`;
                const input = el("input", "input", "", {
                    id, type: "text", autocomplete: "off", autocapitalize: "off", spellcheck: "false",
                    inputmode: field.ref === "manual.dns" ? "text" : "decimal",
                    "aria-describedby": `${id}-help ${id}-error`,
                });
                input.value = field.default ?? "";
                input.addEventListener("input", () => { input.removeAttribute("aria-invalid"); made.error.textContent = ""; });
                input.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); save(); } });
                made = {
                    input,
                    label: el("label", "", "", { for: id }),
                    help: el("p", "note", "", { id: `${id}-help` }),
                    error: el("p", "field-error", "", { id: `${id}-error`, "aria-live": "polite" }),
                };
                parts.fields[field.ref] = made;
                parts.form.append(el("div", "field", [made.label, input, made.error, made.help]));
            }
            made.label.textContent = field.required ? field.name : `${field.name} (optional)`;
            made.input.placeholder = field.placeholder ?? "";
            made.help.textContent = field.help ?? "";
            made.help.hidden = !field.help;
            made.error.textContent = field.error ?? "";
            if (field.error) made.input.setAttribute("aria-invalid", "true");
            else made.input.removeAttribute("aria-invalid");
            made.input.required = field.required;
        }
    }

    /** Draws `next`, the page as setup says it now is, keeping what has been
        typed; `waiting` is what was pressed and not yet answered. */
    function draw(next, waiting) {
        page = next;
        if (!parts || !parts.title.isConnected) build();
        parts.title.textContent = page.title;
        parts.lede.textContent = page.intro;
        drawRows();
        drawChosen();
        parts.listError.textContent = page.error ?? "";
        drawFields();
        const name = (node, action) => {
            node.hidden = !action;
            if (!action) return;
            node.querySelector("span").textContent = action.name;
            if (action.enabled) node.removeAttribute("aria-disabled");
            else node.setAttribute("aria-disabled", "true");
        };
        name(parts.back, page.back);
        name(parts.save, page.save);
        parts.save.toggleAttribute("aria-busy", !!waiting && waiting === page.save?.ref);
        // Turned down afresh: the keyboard goes to the first thing wrong.
        const errors = JSON.stringify([page.error, page.fields.map((f) => f.error)]);
        if (errors !== errorsAs) {
            errorsAs = errors;
            const wrong = page.error ? rows().find((r) => r.tabIndex === 0)
                : Object.values(parts.fields).find(({ input }) => input.getAttribute("aria-invalid") === "true")?.input;
            // Into the middle, with what is wrong with it under it: the
            // bottom bar would cover a field only scrolled to the edge.
            wrong?.focus({ preventScroll: true });
            wrong?.scrollIntoView({ block: "center", behavior: reduced ? "auto" : "smooth" });
        }
    }

    /** The page is showing: the keyboard starts on the address, or on the
        interfaces if none is chosen yet. */
    function focus() {
        if (!parts) return;
        if (!chosen) return rows().find((r) => r.tabIndex === 0)?.focus({ preventScroll: true });
        parts.fields["manual.address"]?.input.focus({ preventScroll: true });
    }

    function gone() {
        parts = page = null;
        chosen = null;
    }

    return { draw, focus, gone };
}
