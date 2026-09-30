// First-boot setup's first page: a welcome, and the language.
//
// oobed says what is on it: its greeting, a sentence, the language (which it
// shows and cannot yet let be chosen, since Peios has no locale data), and
// Next. It also sends a keyboard layout, for the console, where a password is
// about to be typed on whatever the machine's keymap is. In a browser the
// keyboard is the browser's own, so that is not drawn here, and the page says
// why there is nothing to choose.
//
// Until a language can be chosen, the greeting goes round the languages Peios
// hopes to speak, in each one's own words. That is the page's own flourish:
// what the page is called, to a screen reader and in the tab, is oobed's.
import { NOT_DRAWN, ONWARD, picture } from "./bits.js";

// "Welcome to Peios", in each language's own words.
const GREETINGS = [
    "Willkommen bei Peios", "Bienvenido a Peios", "Bienvenue dans Peios", "Benvenuto in Peios", "Welkom bij Peios",
    "Witamy w Peios", "Bem-vindo ao Peios", "Välkommen till Peios", "Peios'a hoş geldiniz", "Ласкаво просимо до Peios",
    "Peiosへようこそ", "Peios에 오신 것을 환영합니다", "欢迎使用 Peios",
];
// The language a greeting is in, for it to be read and set as that language.
const LANGS = ["de", "es", "fr", "it", "nl", "pl", "pt-BR", "sv", "tr", "uk", "ja", "ko", "zh-CN"];
// How long each greeting stays, and how long the change between two takes.
const EVERY = 2600, CHANGE = 420;

/**
 * The welcome page. `turn` is where a page is drawn; `el` and `rise` make its
 * parts; `ask` asks setup for something; `toast` says something in passing.
 */
export function createWelcomePage({ turn, el, rise, ask, toast, reduced }) {
    let parts = null;   // the page's own elements, while it is the page drawn
    let page = null;    // what setup last said is on it
    let cycle = 0, swap = 0, at = 0;

    function build() {
        const next = el("button", "btn go-on", [el("span"), picture(ONWARD)], { type: "button" });
        next.addEventListener("click", () => {
            if (!page.next?.enabled) return;
            // The page it leads to is not drawn yet, and an answer would
            // take everyone looking to it.
            if (page.next.unbuilt) return toast(NOT_DRAWN);
            ask({ press: page.next.ref });
        });
        parts = {
            greet: el("span", "greet", "", { "aria-hidden": "true" }),
            title: null,
            lede: rise(1, "p", "lede"),
            language: el("div", "field"),
            next,
        };
        parts.title = rise(0, "h1", "", [parts.greet]);
        turn.replaceChildren(
            parts.title,
            parts.lede,
            rise(2, "div", "fields", [
                parts.language,
                el("p", "note", "Your keyboard is this device's own, so there is no layout to choose here."),
            ]),
            rise(3, "div", "nav", [parts.next]),
        );
    }

    // The language, as oobed has it: not a choice yet, and why.
    function drawLanguage() {
        const language = page.language;
        if (!language) return parts.language.replaceChildren();
        const chosen = language.choices[0]?.[1] ?? "English";
        const box = el("div", "select", [el("span", "sv", chosen)], {
            id: "welcome-language", role: "textbox", "aria-readonly": "true", "aria-labelledby": "welcome-language-label",
            ...(language.help ? { "aria-describedby": "welcome-language-help" } : {}),
            ...(language.enabled ? {} : { "aria-disabled": "true" }),
        });
        parts.language.replaceChildren(
            el("label", "", language.name, { id: "welcome-language-label" }),
            box,
            ...(language.help ? [el("p", "note", language.help, { id: "welcome-language-help" })] : []),
        );
    }

    // Some greetings are longer than the column. They are set smaller to stay
    // on one line, so that nothing under the heading moves.
    function fit() {
        const { greet } = parts;
        greet.style.fontSize = "";
        const room = parts.title.clientWidth, need = greet.scrollWidth;
        if (room && need > room) greet.style.fontSize = `${Math.floor(room / need * 100)}%`;
    }

    function greet(text, lang) {
        const { greet } = parts;
        greet.textContent = text;
        if (lang) greet.lang = lang; else greet.removeAttribute("lang");
        fit();
    }

    // From oobed's own greeting, round the others and back to it.
    function startGreeting() {
        stopGreeting();
        if (reduced) return;
        at = 0;
        cycle = setInterval(() => {
            if (!parts) return stopGreeting();
            at = (at + 1) % (GREETINGS.length + 1);
            parts.greet.classList.add("out");
            swap = setTimeout(() => {
                if (!parts) return;
                if (at === 0) greet(page.title, "");
                else greet(GREETINGS[at - 1], LANGS[at - 1]);
                parts.greet.classList.remove("out");
            }, CHANGE);
        }, EVERY);
    }

    function stopGreeting() {
        clearInterval(cycle);
        clearTimeout(swap);
        cycle = swap = 0;
    }

    /** Draws `next`, the welcome as setup says it now is; `waiting` is what
        was pressed and not yet answered. */
    function draw(next, waiting) {
        const fresh = !parts;
        if (fresh) build();
        const was = page;
        page = next;
        parts.title.setAttribute("aria-label", page.title);
        if (fresh || was.title !== page.title) {
            greet(page.title, "");
            // Round again from the new one, if it was going round.
            if (cycle) startGreeting();
        }
        parts.lede.textContent = page.intro;
        drawLanguage();
        parts.next.firstChild.textContent = page.next?.name ?? "Next";
        parts.next.hidden = !page.next;
        parts.next.toggleAttribute("aria-busy", !!waiting && waiting === page.next?.ref);
        if (page.next && !page.next.enabled) parts.next.setAttribute("aria-disabled", "true");
        else parts.next.removeAttribute("aria-disabled");
    }

    /** The page is showing: the keyboard starts on Next, the one thing there
        is to press, and the greeting sets off, having been oobed's until it
        could be seen. */
    function arrived() {
        if (!parts) return;
        parts.next.focus({ preventScroll: true });
        if (!cycle) startGreeting();
    }

    /** Another page has taken this one's place. */
    function gone() {
        stopGreeting();
        parts = page = null;
    }

    return { draw, arrived, gone };
}
