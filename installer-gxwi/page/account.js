// First-boot setup's account: the machine's first, which administers it.
//
// oobed asks for a name, a password and the password again, and checks them
// when Next is pressed: it says on a field what is wrong, the page staying,
// or goes on. What is typed stays in this browser until then and goes to
// oobed with Next, never to anyone else looking: a password least of all.
//
// Some of it is said here as it is typed, and none of that is a check: how
// strong the password looks, that the two passwords match or do not yet,
// that Caps Lock is on, and who the account will be. And where this page
// reached the machine over a network, in the clear, it says that the
// password will cross it so.
import { BACK, NOT_DRAWN, ONWARD, icon, picture } from "./bits.js";

const EYE = icon("0 0 18 18", 1.5, '<path d="M1.75 9S4.5 3.75 9 3.75 16.25 9 16.25 9 13.5 14.25 9 14.25 1.75 9 1.75 9z"/><circle cx="9" cy="9" r="2.25"/>');
const SHUT = icon("0 0 18 18", 1.5, '<path d="M1.75 9S4.5 3.75 9 3.75c1.4 0 2.6.5 3.6 1.2M16.25 9s-.9 1.7-2.6 3.1M9 14.25c-1.3 0-2.5-.4-3.5-1.1"/><path d="M2.5 15.5 15.5 2.5"/>');
const OPEN = icon("0 0 16 16", 1.5, '<rect x="3" y="7.25" width="10" height="7" rx="1.5"/><path d="M5.25 7.25V5a2.75 2.75 0 0 1 5.4-.75"/>');
const TICK = icon("0 0 12 12", 2, '<path d="M2.5 6.2 5 8.5l4.5-5"/>');
const SHIELD = icon("0 0 16 16", 1.5, '<path d="M8 1.75 13.25 3.5v4c0 3.2-2.2 5.6-5.25 6.75C4.95 13.1 2.75 10.7 2.75 7.5v-4z"/><path d="M5.75 8 7.4 9.6l3-3.2"/>');

/** Whether this page is being looked at on the machine it came from, or over
    a network: a location that is the loopback crosses none. */
const local = (host) => /^(localhost|127(\.\d+){3}|\[::1\])$/i.test(host);

// Passwords that are guessed first, whatever they are made of.
const COMMON = ["password", "passw0rd", "12345678", "123456789", "qwertyui", "qwerty123", "letmein1", "iloveyou", "peios123", "admin123"];
const STRENGTHS = ["", "Weak", "Fair", "Strong", "Very strong"];

/** How strong `password` looks, 0 (nothing typed) to 4, and in words: by
    how many guesses it would take at most, from its length and the kinds
    of character in it; one that is common or is the account's own name is
    guessed at once. Only a guide: oobed takes any password that is not
    empty. */
function strength(password, name) {
    if (!password) return [0, ""];
    const lower = password.toLowerCase();
    if (COMMON.includes(lower) || lower === name.trim().toLowerCase()) return [1, "Easily guessed"];
    const pool = (/[a-z]/.test(password) ? 26 : 0) + (/[A-Z]/.test(password) ? 26 : 0)
        + (/\d/.test(password) ? 10 : 0) + (/[^A-Za-z0-9]/.test(password) ? 33 : 0);
    const bits = [...password].length * Math.log2(pool || 1);
    const level = bits < 36 ? 1 : bits < 50 ? 2 : bits < 70 ? 3 : 4;
    return [level, STRENGTHS[level]];
}

/**
 * The account page. `turn` is where a page is drawn; `el` and `rise` make its
 * parts; `ask` asks setup for something; `toast` says something in passing.
 */
export function createAccountPage({ turn, el, rise, ask, toast, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what setup last said is on it
    let errorsAs = "";
    let shown = false;  // the passwords are shown as typed

    function button(className, content, press, attributes) {
        const node = el("button", className, content, { type: "button", ...attributes });
        node.addEventListener("click", press);
        return node;
    }

    const input = (ref) => parts?.fields[ref]?.input;

    function next() {
        if (!page.next?.enabled) return;
        // The page after this is not drawn yet, and an answer would move
        // everyone looking on to a page nobody here can see.
        if (page.next.unbuilt) return toast(NOT_DRAWN);
        const values = {};
        for (const [ref, { input }] of Object.entries(parts.fields)) values[ref] = input.value;
        ask({ press: page.next.ref, values });
    }

    function build() {
        const cleartext = location.protocol === "http:" && !local(location.hostname);
        parts = {
            title: rise(0, "h1", ""),
            lede: rise(1, "p", "lede"),
            acct: rise(2, "div", "acct", [
                el("span", "acct-face", "", { "aria-hidden": "true" }),
                el("span", "acct-said", [el("span", "acct-name"), el("span", "acct-what", "Administrator of this machine")]),
                el("span", "acct-badge", [picture(SHIELD)], { "aria-hidden": "true" }),
            ]),
            // A form, for a password manager to know the fields for what
            // they are. It is never sent: Next asks setup.
            form: rise(3, "form", "fields", null, { novalidate: "", "aria-label": "Account" }),
            fields: {},
            clear: cleartext ? rise(4, "p", "clear", [
                picture(OPEN),
                el("span", "", "This page reaches the machine unencrypted, so the password crosses the network as it is sent. On a network you do not trust, set it on the machine's own screen instead."),
            ]) : null,
            back: button("btn quiet", [picture(BACK), el("span")], () => page.back?.enabled && ask({ press: page.back.ref })),
            next: button("btn go-on", [el("span"), picture(ONWARD)], next),
        };
        parts.form.addEventListener("submit", (e) => { e.preventDefault(); next(); });
        turn.replaceChildren(
            parts.title, parts.lede, parts.acct, parts.form,
            ...(parts.clear ? [parts.clear] : []),
            rise(5, "div", "nav", [parts.back, parts.next]),
        );
        errorsAs = JSON.stringify([]);
        shown = false;
    }

    // Who the account will be, as the name is typed: its first letter, in a
    // colour of its own.
    function drawWho() {
        const name = (input("account.name")?.value ?? "").trim();
        const face = parts.acct.querySelector(".acct-face");
        face.textContent = (name[0] ?? "").toUpperCase();
        let hue = 0;
        for (const c of name.toLowerCase()) hue = (hue * 31 + c.charCodeAt(0)) % 360;
        parts.acct.style.setProperty("--hue", hue);
        parts.acct.classList.toggle("nobody", !name);
        parts.acct.querySelector(".acct-name").textContent = name || "No name yet";
    }

    // Whether the password typed again matches, while it is being typed: said
    // only once it is as long as the first, or where it has already gone wrong.
    function drawMatch() {
        const first = input("account.password"), again = input("account.confirm");
        const hint = parts.fields["account.confirm"]?.hint;
        if (!first || !again || !hint) return;
        const [a, b] = [first.value, again.value];
        // What oobed said of it stands on its own.
        const said = again.getAttribute("aria-invalid") === "true";
        const state = !b || said ? "" : a === b ? "match" : b.length >= a.length || !a.startsWith(b) ? "differ" : "";
        if (hint.dataset.state === state) return;
        hint.dataset.state = state;
        hint.replaceChildren(...(state === "match" ? [picture(TICK), el("span", "", "Matches")]
            : state === "differ" ? [el("span", "", "Does not match yet")] : []));
    }

    // How strong the password looks, as it is typed: four bars and a word,
    // said again to a screen reader only when the word changes.
    function drawStrength() {
        const meter = parts.fields["account.password"]?.strength;
        if (!meter) return;
        const [level, words] = strength(input("account.password").value, input("account.name")?.value ?? "");
        if (meter.dataset.level === String(level)) return;
        meter.dataset.level = level;
        meter.querySelector(".acct-strength-say").textContent = words;
    }

    function reveal() {
        shown = !shown;
        for (const ref of ["account.password", "account.confirm"]) {
            const made = parts.fields[ref];
            if (!made?.secret) continue;
            made.input.type = shown ? "text" : "password";
        }
        const eye = parts.fields["account.password"]?.eye;
        if (eye) {
            eye.replaceChildren(picture(shown ? SHUT : EYE));
            eye.setAttribute("aria-pressed", String(shown));
            eye.setAttribute("aria-label", shown ? "Hide the password" : "Show the password");
        }
    }

    // Each line to fill in, made once and kept, so that what is typed stays
    // while oobed says what is wrong with it.
    function drawFields() {
        const refs = page.fields.map((f) => f.ref);
        for (const field of page.fields) {
            let made = parts.fields[field.ref];
            if (!made) {
                const id = `field-${field.ref.replace(/\W/g, "-")}`;
                const input = el("input", "input", "", {
                    id, type: field.secret ? "password" : "text",
                    // So a password manager offers to make one and keep it,
                    // and knows whose it is.
                    autocomplete: field.secret ? "new-password" : "username",
                    autocapitalize: "off", spellcheck: "false",
                    "aria-describedby": `${id}-help ${id}-error ${id}-hint`,
                });
                input.value = field.default ?? "";
                const rated = field.ref === "account.password" && field.secret;
                made = {
                    input, secret: field.secret,
                    label: el("label", "", "", { for: id }),
                    help: el("p", "note", "", { id: `${id}-help` }),
                    error: el("p", "field-error", "", { id: `${id}-error`, "aria-live": "polite" }),
                    hint: el("p", "field-hint", "", { id: `${id}-hint`, "aria-live": "polite" }),
                    caps: field.secret ? el("p", "field-caps", "Caps Lock is on", { hidden: "" }) : null,
                    strength: rated ? el("div", "acct-strength", [
                        el("span", "acct-meter", [el("i"), el("i"), el("i"), el("i")], { "aria-hidden": "true" }),
                        el("span", "acct-strength-say"),
                    ], { id: `${id}-strength`, "aria-live": "polite", "data-level": "0" }) : null,
                };
                if (rated) input.setAttribute("aria-describedby", `${id}-strength ${input.getAttribute("aria-describedby")}`);
                input.addEventListener("input", () => {
                    input.removeAttribute("aria-invalid");
                    made.error.textContent = "";
                    drawWho();
                    drawMatch();
                    drawStrength();
                });
                input.addEventListener("keydown", (e) => {
                    if (field.secret) caps(made, e.getModifierState?.("CapsLock"));
                    if (e.key !== "Enter") return;
                    e.preventDefault();
                    // On to the next line, and from the last, on.
                    const after = refs[refs.indexOf(field.ref) + 1];
                    if (after && parts.fields[after]) parts.fields[after].input.focus();
                    else next();
                });
                if (field.secret) {
                    input.addEventListener("keyup", (e) => caps(made, e.getModifierState?.("CapsLock")));
                    input.addEventListener("blur", () => caps(made, false));
                }
                let box = input;
                // The first password carries the way to see what is typed,
                // for both.
                if (field.secret && !Object.values(parts.fields).some((f) => f.secret)) {
                    made.eye = button("eye", [picture(EYE)], reveal, { "aria-pressed": "false", "aria-label": "Show the password" });
                    box = el("div", "secret", [input, made.eye]);
                }
                parts.fields[field.ref] = made;
                parts.form.append(el("div", "field", [made.label, box, ...(made.strength ? [made.strength] : []), made.error,
                    ...(made.caps ? [made.caps] : []), made.hint, made.help]));
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
        drawWho();
        drawMatch();
        drawStrength();
    }

    // Caps Lock, which a password typed blind is most often wrong by. Said
    // while the field is being typed in, and not once it is left.
    function caps(made, on) {
        if (made.caps) made.caps.hidden = !on;
    }

    /** Draws `next`, the page as setup says it now is, keeping what has been
        typed; `waiting` is what was pressed and not yet answered. */
    function draw(nextPage, waiting) {
        page = nextPage;
        if (!parts || !parts.title.isConnected) build();
        parts.title.textContent = page.title;
        parts.lede.textContent = page.intro;
        drawFields();
        const name = (node, action) => {
            node.hidden = !action;
            if (!action) return;
            node.querySelector("span").textContent = action.name;
            if (action.enabled) node.removeAttribute("aria-disabled");
            else node.setAttribute("aria-disabled", "true");
        };
        name(parts.back, page.back);
        name(parts.next, page.next);
        parts.next.toggleAttribute("aria-busy", !!waiting && waiting === page.next?.ref);
        // Turned down afresh: the keyboard goes to the first thing wrong, with
        // what was typed in it chosen, to be typed over.
        const errors = JSON.stringify(page.fields.map((f) => f.error));
        if (errors !== errorsAs) {
            errorsAs = errors;
            const wrong = Object.values(parts.fields).find(({ input }) => input.getAttribute("aria-invalid") === "true")?.input;
            wrong?.focus({ preventScroll: true });
            wrong?.select();
            wrong?.scrollIntoView({ block: "center", behavior: reduced ? "auto" : "smooth" });
        }
    }

    /** The page is showing: the keyboard starts on the name, chosen, so that
        typing replaces the one offered. */
    function focus() {
        const name = input("account.name");
        if (!name) return;
        name.focus({ preventScroll: true });
        name.select();
    }

    function gone() {
        // What was typed goes with the page, and is not left in the document.
        for (const { input } of Object.values(parts?.fields ?? {})) input.value = "";
        parts = page = null;
    }

    return { draw, focus, gone };
}
