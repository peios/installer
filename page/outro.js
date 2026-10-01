// Setup's last moment in the browser: going on to GXWI's sign-in page
// without a cut.
//
// The sign-in page is GXWI's, served at this same address once setup has
// gone, and it stands where this page does: the same dark ground, the same
// three glows, stars drawn the same way, and the lockup in the same corner.
// So nothing of that need change as one gives way to the other, and nothing
// does. Once setup is complete it is left to be seen for a moment; then
// everything on this page but the ground, the stars and the lockup fades,
// the stars are left where GXWI's pages look for them (in this tab's session
// storage, which the two pages share by sharing an address), and the browser
// goes on. The sign-in page takes the stars and the glows up where they
// were, and its own parts rise into place.
//
// GXWI serves its own files under /.gxwi/ whatever else it is doing, so the
// sign-in page's stylesheet and faces are asked for while setup is finishing,
// for that page to be drawn in them from its first frame.
//
// At another address (one given to the machine by hand) there is nothing to
// share and nothing already there: the page fades the same, and the sign-in
// page there starts a field of its own.

// Where GXWI's logon pages look for a field to take up, and the shape they
// read (gxwi-server's logon.js).
const KEY = "gxwi.field";
const VERSION = 1;
// How long "Setup is complete" is left to be read before anything goes, and
// how long the going takes.
const HOLD = 2200, FADE = 700;
const AHEAD = ["/.gxwi/logon.css", "/.gxwi/fonts/manrope.woff2", "/.gxwi/fonts/schibsted-grotesk.woff2"];

/**
 * `stage` is the whole page, `glows` the backdrop's glows, `field` the stars;
 * `go` takes the browser somewhere else.
 */
export function createOutro({ stage, glows, field, reduced, go = (url) => location.assign(url) }) {
    let completeAt = 0, fetched = false, leaving = false;

    /** Asks for what the sign-in page will be drawn in, once. */
    function ahead() {
        if (fetched) return;
        fetched = true;
        for (const url of AHEAD) fetch(url).catch(() => { /* drawn when it comes, then */ });
    }

    /** Setup has said it is complete: from now, it is left to be seen. */
    function complete() {
        if (completeAt) return;
        completeAt = performance.now();
        ahead();
    }

    // The stars and the glows as they are, where the sign-in page looks.
    function handOver() {
        const snapshot = field.snapshot();
        if (!snapshot) return;
        const phases = glows.map((glow) => {
            const running = glow.getAnimations?.().find((a) => a.animationName);
            return running ? (Number(running.currentTime) || 0) - (running.effect.getTiming().delay || 0) : 0;
        });
        try {
            sessionStorage.setItem(KEY, JSON.stringify({ v: VERSION, at: Date.now(), ...snapshot, glows: phases, rise: true }));
        } catch { /* no storage: the sign-in page starts a field of its own */ }
    }

    /** Goes on to `url`, once setup has been seen to be complete. */
    function leave(url) {
        if (leaving) return;
        leaving = true;
        ahead();
        const here = new URL(url, location.href).origin === location.origin;
        const held = completeAt ? Math.max(0, HOLD - (performance.now() - completeAt)) : 0;
        setTimeout(() => {
            // The lockup is where the sign-in page has its own only with the
            // page at its top.
            window.scrollTo({ top: 0, behavior: reduced ? "auto" : "smooth" });
            stage.classList.add("leaving");
            setTimeout(() => {
                if (here) handOver();
                go(url);
            }, reduced ? 0 : FADE);
        }, reduced ? 0 : held);
    }

    return { complete, leave, get leaving() { return leaving; } };
}
