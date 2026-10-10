// The end of first-boot setup, from the page's side.
//
// Setup finishing takes this page's ground away. oobed removes the overlay
// as its last act, so GXWI goes back to its sign-in page and the process
// that served this one is ended with it; and where an interface was given an
// address by hand, that address is applied last, so a page reached through
// that interface loses the machine as it is. Either way the connection
// closes, and this is what the page does then: it keeps the page as it was,
// and asks until something answers.
//
// - The address this page came from answers with something other than
//   setup: the sign-in page is back, and the page goes to it.
// - The address given by hand answers: the machine is there now, and the
//   page goes to it there. Only whether it answers can be known, the answer
//   being another origin's; what is found there is the sign-in page, or
//   setup's last moments, which come round to the sign-in page the same way.
// - Setup answers again where it was: it had not finished going, and the
//   page goes on as before, the connection being tried again meanwhile.
// - Nothing, for long enough to say so: the page says where the machine was
//   going, and what its own screen shows.

// How often the machine is asked, how long one asking may take, and how long
// before the page says nothing has answered.
const EVERY = 1500, ASKING = 4000, PATIENCE = 2 * 60 * 1000;
// What GXWI answers with while an overlay it is to send everyone to is not up:
// setup, on its way back.
const UNAVAILABLE = 503;

/** An address oobed gives an interface, as a host a location can name: the
    length of its network left off, and an IPv6 one in brackets. */
export function hostOf(address) {
    const bare = String(address).replace(/\/\d+$/, "");
    return bare.includes(":") ? `[${bare}]` : bare;
}

/**
 * `onChange` is told whenever what the ending has to say changes, to draw it;
 * `go` takes the browser somewhere else.
 */
export function createEnding({ onChange, go = (url) => location.assign(url) }) {
    let active = false, timer = 0, since = 0;
    let there = null;   // the origin of the address given by hand, if any
    let state = { stage: "", words: "", onward: "/" };

    function set(next) {
        state = { ...state, ...next };
        onChange(state);
    }

    async function ask(url, options) {
        try {
            return await fetch(url, { cache: "no-store", signal: AbortSignal.timeout(ASKING), ...options });
        } catch {
            return null; // nothing answered
        }
    }

    async function round() {
        timer = 0;
        if (!active) return;
        const [here, moved] = await Promise.all([
            ask("/hello"),
            there ? ask(`${there}/hello`, { mode: "no-cors" }) : null,
        ]);
        if (!active) return;
        if (here && here.status !== UNAVAILABLE) {
            const said = here.ok ? await here.json().catch(() => null) : null;
            // Setup again: it has not gone yet, and the connection, tried
            // again meanwhile, will say so.
            if (!said?.setup) return arrive("/");
        }
        if (moved) return arrive(`${there}/`);
        if (Date.now() - since > PATIENCE && state.stage !== "lost") {
            set({
                stage: "lost",
                words: there
                    ? `Nothing has answered at ${there.replace(/^\w+:\/\//, "")}. The machine's own screen says where it is.`
                    : "The machine has not answered. Its own screen says where it is.",
            });
        }
        timer = setTimeout(round, EVERY);
    }

    function arrive(url) {
        active = false;
        set({ stage: "going", words: url === "/" ? "Taking you to sign in." : `Taking you to this machine at ${url.replace(/^\w+:\/\/|\/$/g, "")}.`, onward: url });
        go(url);
    }

    /** The connection has closed while `page`, setup's job, is showing. */
    function begin(page) {
        if (active) return;
        const network = page.phases?.find((phase) => phase.ref === "phase.network");
        const address = network?.detail?.address;
        const host = address ? hostOf(address) : null;
        there = host && host !== location.hostname && `[${location.hostname}]` !== host
            ? `${location.protocol}//${host}${location.port ? `:${location.port}` : ""}` : null;
        active = true;
        since = Date.now();
        set({
            stage: "waiting",
            words: there ? `Following this machine to ${host}.` : "Waiting for the sign-in page.",
            onward: there ? `${there}/` : "/",
        });
        round();
    }

    /** The connection is back: setup had not gone, and the page goes on. */
    function stop() {
        if (!active) return;
        active = false;
        clearTimeout(timer);
        set({ stage: "", words: "" });
    }

    return {
        begin, stop,
        get active() { return active || state.stage === "going" || state.stage === "lost"; },
        get state() { return state; },
    };
}
