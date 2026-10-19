// First-boot setup's naming page: what the machine is called on a network.
//
// oobed offers a name, says what a name may be (`max`, and `pattern`, the
// same rule it checks), and checks what it is answered with when Finish is
// pressed: it says on the field what is wrong, the page staying, or applies
// setup. What is typed stays in this browser until then.
//
// As it is typed, the name is shown as the machine it will be, as the
// account's shell prompt will read on it, and whether it is one a network
// will carry yet, by oobed's own pattern. That is a hint and not a check:
// oobed has the last word. Names to pick from are offered under it: the one
// oobed offered, which can be had back with one press, and others made up
// here, which Shuffle makes up again; each is one oobed's pattern takes.
// Joining a domain oobed cannot do yet, and says why.
import { BACK, NOT_DRAWN, ONWARD, icon, picture } from "./bits.js";

const MACHINE = icon("0 0 24 24", 1.5, '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/><path d="M7 8.5h4"/>');
const DOMAIN = icon("0 0 16 16", 1.6, '<circle cx="8" cy="8" r="6.25"/><path d="M1.75 8h12.5M8 1.75c1.75 1.9 2.6 4 2.6 6.25S9.75 12.35 8 14.25C6.25 12.35 5.4 10.25 5.4 8S6.25 3.65 8 1.75z"/>');
const DICE = icon("0 0 16 16", 1.5, '<rect x="2" y="2" width="12" height="12" rx="3"/><path d="M5.5 5.5h.01M10.5 5.5h.01M8 8h.01M5.5 10.5h.01M10.5 10.5h.01"/>');
const OTHERS = { "domain.join": DOMAIN };

// What the names made up here are made of: two words, a hyphen between.
const ADJ = ["amber", "azure", "calm", "coral", "drift", "golden", "lunar", "misty", "north", "quiet", "salt", "silver", "still", "tidal", "velvet", "bright"];
const NOUN = ["beacon", "cove", "current", "dune", "gull", "harbour", "heron", "kelp", "lagoon", "lantern", "pebble", "reef", "shell", "shoal", "tern", "wave"];
// How many names are offered, oobed's among them.
const OFFERED = 4;
const any = (list) => list[crypto.getRandomValues(new Uint32Array(1))[0] % list.length];

/**
 * The naming page. `turn` is where a page is drawn; `el` and `rise` make its
 * parts; `ask` asks setup for something; `toast` says something in passing.
 */
export function createNamingPage({ turn, el, rise, ask, toast, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what setup last said is on it
    let othersAs = "", errorAs = null;
    let shape = null;   // oobed's pattern, as a regular expression
    let offered = null; // the names to pick from, oobed's first, as last drawn

    function button(className, content, press, attributes) {
        const node = el("button", className, content, { type: "button", ...attributes });
        node.addEventListener("click", press);
        return node;
    }

    function finish() {
        if (!page.finish?.enabled) return;
        // Applying is not drawn yet, and an answer would take everyone
        // looking to it.
        if (page.finish.unbuilt) return toast(NOT_DRAWN);
        ask({ press: page.finish.ref, values: { [page.name.ref]: parts.input.value } });
    }

    function build() {
        const id = "field-hostname";
        parts = {
            title: rise(0, "h1", ""),
            machine: rise(1, "div", "machine", [
                el("span", "machine-glyph", [picture(MACHINE)], { "aria-hidden": "true" }),
                el("span", "machine-said", [el("span", "machine-name"), el("span", "machine-what")]),
                // The machine as the account's prompt will read on it.
                el("code", "name-prompt", [
                    el("span", "name-prompt-u"), el("span", "name-prompt-at", "@"), el("span", "name-prompt-h"), ":~$ ",
                    el("i", "name-caret", "", { "aria-hidden": "true" }),
                ]),
            ]),
            label: el("label", "", "", { for: id }),
            input: el("input", "input", "", {
                id, type: "text", autocomplete: "off", autocapitalize: "off", spellcheck: "false",
                "aria-describedby": `${id}-error ${id}-hint ${id}-help`,
            }),
            count: el("span", "count", "", { "aria-hidden": "true" }),
            error: el("p", "field-error", "", { id: `${id}-error`, "aria-live": "polite" }),
            hint: el("p", "field-hint", "", { id: `${id}-hint` }),
            help: el("p", "note", "", { id: `${id}-help` }),
            chips: el("div", "name-chips", null, { role: "group", "aria-labelledby": "name-suggest-label" }),
            shuffle: button("link name-shuffle", [picture(DICE), el("span", "", "Shuffle")], shuffle),
            others: el("span", "others"),
            why: el("p", "help", "", { id: "naming-help", "aria-live": "polite" }),
            back: button("btn quiet", [picture(BACK), el("span")], () => page.back?.enabled && ask({ press: page.back.ref })),
            finish: button("btn go-on", [el("span"), picture(ONWARD)], finish),
        };
        parts.input.addEventListener("input", edited);
        parts.input.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); finish(); } });
        turn.replaceChildren(
            parts.title, parts.machine,
            rise(2, "div", "fields", [el("div", "field", [
                el("div", "field-top", [parts.label, parts.count]),
                parts.input, parts.error, parts.hint, parts.help,
            ])]),
            rise(3, "div", "name-suggest", [
                el("span", "name-suggest-label", "Or pick one", { id: "name-suggest-label" }),
                parts.chips, parts.shuffle,
            ]),
            rise(4, "div", "aux", [parts.others, parts.why]),
            rise(5, "div", "nav", [parts.back, parts.finish]),
        );
        othersAs = "";
        errorAs = null;
        offered = null;
    }

    // The name has changed, by typing or by picking one: what oobed said of
    // the last one no longer stands.
    function edited() {
        parts.input.removeAttribute("aria-invalid");
        parts.error.textContent = "";
        typed();
    }

    // Whether `name` is one oobed would take, as far as the page can tell:
    // its pattern, its longest, and not localhost, which every machine is.
    function takes(name) {
        const max = page.name?.max;
        return (!shape || shape.test(name)) && (!max || name.length <= max) && name.toLowerCase() !== "localhost";
    }

    // The names to pick from: oobed's, and others made up to its rule. Each
    // is a button that puts it in the field; the one in the field is shown
    // pressed. `fresh` is whether they are new ones, which are seen to come.
    function suggest(fresh) {
        const theirs = page.name?.default;
        const names = theirs && takes(theirs) ? [theirs] : [];
        for (let tries = 0; names.length < OFFERED && tries < 100; tries++) {
            const name = `${any(ADJ)}-${any(NOUN)}`;
            if (takes(name) && !names.includes(name)) names.push(name);
        }
        offered = names;
        parts.chips.replaceChildren(...names.map((name, i) => {
            const chip = button(`name-chip${fresh ? " new" : ""}`, name, () => {
                parts.input.value = name;
                edited();
            }, { "aria-pressed": "false" });
            chip.style.setProperty("--i", i);
            return chip;
        }));
        pressChips();
    }

    function shuffle() {
        parts.shuffle.classList.remove("rolled");
        void parts.shuffle.offsetWidth;
        parts.shuffle.classList.add("rolled");
        suggest(true);
    }

    function pressChips() {
        const name = parts.input.value.trim().toLowerCase();
        for (const chip of parts.chips.children) chip.setAttribute("aria-pressed", String(chip.textContent.toLowerCase() === name));
    }

    function say(text) {
        parts.why.textContent = text;
        parts.why.classList.toggle("on", !!text);
    }

    // The machine as it will be, and whether what is typed is a name yet.
    function typed() {
        const value = parts.input.value;
        const name = value.trim();
        parts.machine.querySelector(".machine-name").textContent = name || "No name yet";
        parts.machine.classList.toggle("nobody", !name);
        const fits = !name || !shape || shape.test(name);
        parts.machine.classList.toggle("off", !fits);
        parts.machine.querySelector(".machine-what").textContent = fits ? "How this machine is known on a network" : "Not a name a network will carry yet";
        // Said in its place, it is not said again under it.
        parts.hint.textContent = fits ? "" : "Letters, digits and hyphens only, with no hyphen at either end.";
        parts.help.hidden = !page.name?.help || !fits;
        parts.input.toggleAttribute("data-off", !fits);
        const max = page.name?.max;
        parts.count.textContent = max ? `${name.length} / ${max}` : "";
        parts.count.classList.toggle("over", !!max && name.length > max);
        // The prompt, as the account will see it: whose it is where this
        // program heard the account named, and the machine.
        const prompt = parts.machine.querySelector(".name-prompt");
        prompt.querySelector(".name-prompt-u").textContent = page.account ?? "";
        prompt.querySelector(".name-prompt-at").hidden = !page.account;
        prompt.querySelector(".name-prompt-h").textContent = name || "—";
        if (offered) pressChips();
    }

    function drawOthers() {
        const as = JSON.stringify(page.others);
        if (as === othersAs) return;
        othersAs = as;
        parts.others.replaceChildren(...page.others.map((action) => {
            const why = () => say(action.enabled ? "" : action.help ?? "");
            const node = button("link", [picture(OTHERS[action.ref] ?? DOMAIN), el("span", "", action.name)], () => {
                if (!action.enabled) return why();
                if (action.unbuilt) return toast(NOT_DRAWN);
                ask({ press: action.ref });
            });
            node.setAttribute("aria-describedby", "naming-help");
            if (!action.enabled) node.setAttribute("aria-disabled", "true");
            for (const on of ["pointerenter", "focus"]) node.addEventListener(on, why);
            return node;
        }));
    }

    /** Draws `next`, the page as setup says it now is, keeping what has been
        typed; `waiting` is what was pressed and not yet answered. */
    function draw(next, waiting) {
        const fresh = !parts || !parts.title.isConnected;
        page = next;
        if (fresh) build();
        const field = page.name;
        parts.title.textContent = page.title;
        parts.label.textContent = field?.name ?? "";
        parts.help.textContent = field?.help ?? "";
        if (fresh) parts.input.value = field?.default ?? "";
        if (field?.max) parts.input.maxLength = field.max;
        try {
            shape = field?.pattern ? new RegExp(`^(?:${field.pattern})$`) : null;
        } catch {
            shape = null; // a pattern this browser cannot read is oobed's to check
        }
        parts.error.textContent = field?.error ?? "";
        if (field?.error) parts.input.setAttribute("aria-invalid", "true");
        else parts.input.removeAttribute("aria-invalid");
        // Made up once the rule is known, and again only if oobed offers
        // another name of its own.
        if (!offered || (field?.default && takes(field.default) && offered[0] !== field.default)) suggest(false);
        typed();
        drawOthers();
        const name = (node, action) => {
            node.hidden = !action;
            if (!action) return;
            node.querySelector("span").textContent = action.name;
            if (action.enabled) node.removeAttribute("aria-disabled");
            else node.setAttribute("aria-disabled", "true");
        };
        name(parts.back, page.back);
        name(parts.finish, page.finish);
        parts.finish.toggleAttribute("aria-busy", !!waiting && waiting === page.finish?.ref);
        // Turned down afresh: the keyboard goes to the name, chosen, to be
        // typed over.
        const error = field?.error ?? null;
        if (error !== errorAs) {
            errorAs = error;
            if (error) {
                parts.input.focus({ preventScroll: true });
                parts.input.select();
                parts.input.scrollIntoView({ block: "center", behavior: reduced ? "auto" : "smooth" });
            }
        }
    }

    /** The page is showing: the keyboard starts on the name, chosen, so that
        typing replaces the one offered. */
    function focus() {
        if (!parts) return;
        parts.input.focus({ preventScroll: true });
        parts.input.select();
    }

    function gone() {
        parts = page = null;
    }

    return { draw, focus, gone };
}
