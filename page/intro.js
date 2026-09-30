// The intro: about six seconds in which the stars make the mark, the mark
// bursts, the name unfurls beside it, and the two glide to the corner as the
// first page rises.
//
// It is the page's own and the installer knows nothing of it. While it plays,
// the installer has usually long since said which page this is, and that
// page is drawn underneath, waiting to be shown.

// The timeline, in ms from the start.
const T = {
    // when each line of what the installer did may appear in the corner
    lines: [250, 800, 1400, 2000, 2700, 3400],
    gather: 650, gatherFor: 1500,
    formed: 2450, outline: 2700, aurora: 3500, burst: 3600,
    word: 4000, named: 4550, settle: 6000,
};
// How long the lockup takes to reach the corner once it sets off.
const GLIDE = 1080;

/**
 * @param els      stage, introStack, introLockup, headLockup, edition, page, turn
 * @param field    the star field (field.js)
 * @param reduced  whether the person asked for less motion
 * @param onLines  called with how many lines of the ticker may now show
 * @param onLanded called once the page is showing and can be used
 */
export function createIntro({ els, field, reduced, onLines, onLanded }) {
    const { stage, introStack, introLockup, headLockup, edition, page, turn } = els;
    let cues = [], timers = [];
    let t0 = 0, raf = 0;
    let settled = false;

    function frame(ts) {
        raf = requestAnimationFrame(frame);
        const t = ts - t0;
        for (const cue of cues) if (!cue.done && t >= cue.at) { cue.done = true; cue.run(); }
        field.draw(t);
    }

    // The lockup glides to where the header's copy sits, which then takes
    // its place, and the page underneath is shown.
    function settle() {
        if (settled) return;
        settled = true;
        const from = introLockup.getBoundingClientRect();
        const to = headLockup.getBoundingClientRect();
        introLockup.style.transformOrigin = "0 0";
        introLockup.style.transform = `translate(${to.left - from.left}px, ${to.top - from.top}px) scale(${to.height / from.height})`;
        stage.classList.add("settled");
        turn.classList.add("in");
        timers.push(setTimeout(() => {
            headLockup.classList.add("shown");
            edition.classList.add("shown");
            introLockup.classList.add("gone");
            page.classList.add("live");
            onLanded();
        }, reduced ? 0 : GLIDE));
    }

    function makeCues() {
        return [
            ...T.lines.map((at, i) => ({ at, run: () => onLines(i + 1) })),
            { at: T.formed, run: () => introLockup.classList.add("formed") },
            { at: T.outline, run: () => introLockup.classList.add("drawn") },
            { at: T.aurora, run: () => stage.classList.add("aurora-on") },
            { at: T.burst, run: () => { field.release(); introLockup.classList.add("popped"); } },
            { at: T.word, run: () => introLockup.classList.add("word-in") },
            { at: T.named, run: () => introStack.classList.add("named") },
            { at: T.settle, run: settle },
        ].sort((a, b) => a.at - b.at).map((cue) => ({ ...cue, done: false }));
    }

    /** Straight to the end: everything the intro would have done is done at once. */
    function skip() {
        if (settled) return;
        stage.classList.add("instant");
        for (const cue of cues) if (!cue.done && cue.run !== settle) { cue.done = true; cue.run(); }
        void stage.offsetWidth;
        stage.classList.remove("instant");
        for (const cue of cues) cue.done = true;
        t0 = performance.now() - T.settle;
        settle();
    }

    /** From the top, whatever had been played before. */
    function start() {
        cancelAnimationFrame(raf);
        timers.forEach(clearTimeout);
        timers = [];
        stage.classList.add("instant");
        stage.classList.remove("settled", "aurora-on");
        introLockup.classList.remove("formed", "drawn", "popped", "word-in", "gone");
        introStack.classList.remove("named");
        introLockup.style.transform = "";
        headLockup.classList.remove("shown");
        edition.classList.remove("shown");
        page.classList.remove("live");
        turn.classList.remove("in", "out");
        turn.classList.add("first");
        onLines(0);
        void stage.offsetWidth;
        stage.classList.remove("instant");
        settled = false;
        field.seed();
        field.gather(T.gather, T.gatherFor);
        cues = makeCues();
        t0 = performance.now();
        raf = requestAnimationFrame(frame);
        if (reduced) skip();
    }

    return { start, skip, get settled() { return settled; } };
}
