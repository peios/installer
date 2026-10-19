// The field of stars behind everything.
//
// During the intro the stars drift, fly straight into the mark's core to
// make it, and are thrown back out when it bursts. After that they are the
// backdrop: a slow drift that answers the pointer a little, a few of them
// catching the light now and then.
//
// The burst leaves them as a cloud round the middle of the screen. A page can
// have that evened out across the whole of it, by as much as it says, and let
// it fall back. It can have them drain into a place on the page, each star
// setting off at its own moment so that the flow is steady from the first
// second, and have what drained come back out at once. It can have them
// stop. And across a restart it can gather them into the mark again, as the
// intro did, put them out while the machine is away, and throw them out of
// the mark when it is back. Drifting, it can say where every star is, for
// the page that comes after this one to take them up.
//
// The field knows nothing of what the installer is doing. It is told when to
// gather, when to let go, how far to even out, where to drain to and when to
// stop, and draws one frame when asked.

const PALETTE = [["#62d2ff", .55], ["#c8f1ff", .25], ["#9d8cff", .12], ["#4fd1c5", .08]];

const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const easeInOut = (k) => (k < .5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
// A colour, by its place in the palette.
const pick = () => {
    let r = Math.random();
    for (let i = 0; i < PALETTE.length; i++) if ((r -= PALETTE[i][1]) <= 0) return i;
    return 0;
};
const round = (v, places) => Math.round(v * 10 ** places) / 10 ** places;

/**
 * @param canvas  where the stars are drawn, covering the viewport
 * @param mark    the intro's mark, whose core the stars make
 * @param reduced whether the person asked for less motion
 */
export function createField(canvas, mark, reduced) {
    const ctx = canvas.getContext("2d");
    let W = 0, H = 0;
    let parts = [];
    // Where the intro has got to: drifting, gathering into the mark (from
    // `gatherAt`, over `gatherFor`), or let go.
    let gatherAt = Infinity, gatherFor = 1;
    let captured = false, released = false;
    let markX = 0, markY = 0;
    // The pointer, from -1 to 1 across the viewport, and the same smoothed.
    let mx = 0, my = 0, smx = 0, smy = 0;
    // How far the field is evened out, 0 to 1, and how far it is heading.
    let spread = 0, spreadTo = 0;
    // What the field is doing once it is a backdrop: drifting ("ambient"),
    // draining into a place on the page ("stream"), stopped ("stall"),
    // gathering into the mark again ("gather"), or out ("dark").
    let mode = "ambient";
    // The gathering again: when it began, and when the stars began to fade
    // into the solid mark, if they have.
    let regatherAt = 0, fadeAt = 0;
    // The stream: where it goes (a function, asked each frame, since the
    // place moves with the page), how many stars are in it, how many it
    // settles at, and whether it has stopped taking new ones.
    let into = null, streaming = 0, streamCap = 0, closed = false;
    // The share of the stars a steady stream is made of. The rest drain in
    // once and wait unseen until they are let out again.
    const STREAM_SHARE = .4;
    // The clock `draw` was last given.
    let now = 0;

    // Where a drifting star is drawn: where it is, moved a little by the
    // pointer, and as far toward its place in an even field as the field is
    // evened out.
    function drawnAt(p) {
        let x = p.x + smx * p.z * 22, y = p.y + smy * p.z * 22;
        if (spread > .001 && p.ex !== undefined) {
            x += (p.ex - x) * spread;
            y += (p.ey - y) * spread;
        }
        return [x, y];
    }

    const driftX = (p, t) => p.sx + Math.sin(t * .0005 + p.ph) * 10 * p.z;
    const driftY = (p, t) => p.sy + Math.cos(t * .00043 + p.ph) * 10 * p.z;

    function resize() {
        const dpr = Math.min(window.devicePixelRatio || 1, 2);
        W = window.innerWidth;
        H = window.innerHeight;
        canvas.width = Math.round(W * dpr);
        canvas.height = Math.round(H * dpr);
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }

    // Points on the mark's core (the P, not the box around it), in page
    // pixels, where the intro's mark sits.
    function sampleMark() {
        const r = mark.getBoundingClientRect();
        markX = r.left + r.width / 2;
        markY = r.top + r.height / 2;
        const s = Math.max(24, Math.round(r.width));
        const off = document.createElement("canvas");
        off.width = off.height = s;
        const o = off.getContext("2d");
        o.scale(s / 24, s / 24);
        o.fillStyle = "#fff";
        o.beginPath();
        o.moveTo(8.5, 8); o.lineTo(16, 8); o.lineTo(16, 13); o.lineTo(12, 13); o.lineTo(12, 16.5); o.lineTo(8.5, 16.5);
        o.closePath();
        o.fill();
        const data = o.getImageData(0, 0, s, s).data;
        const points = [];
        for (let y = 0; y < s; y++)
            for (let x = 0; x < s; x++)
                if (data[(y * s + x) * 4 + 3] > 140) points.push([r.left + x * r.width / s, r.top + y * r.height / s]);
        for (let i = points.length - 1; i > 0; i--) {
            const j = (Math.random() * (i + 1)) | 0;
            [points[i], points[j]] = [points[j], points[i]];
        }
        return points.length ? points : [[markX, markY]];
    }

    /** A new field, drifting, with every star given its place in the mark. */
    function seed() {
        resize();
        const points = sampleMark();
        const n = W * H < 480000 ? 1200 : 1900;
        parts = [];
        for (let i = 0; i < n; i++) {
            const q = points[i % points.length];
            const z = Math.random();
            const ci = pick();
            parts.push({
                sx: Math.random() * W, sy: Math.random() * H, z,
                tx: q[0] + (Math.random() - .5) * .8, ty: q[1] + (Math.random() - .5) * .8,
                x: 0, y: 0, fx: 0, fy: 0, vx: 0, vy: 0, a: 0,
                delay: Math.random() * 450,
                ph: Math.random() * Math.PI * 2,
                ci, col: PALETTE[ci][0],
                // A few near stars catch the light now and then.
                glint: z > .55 && Math.random() < .0125,
            });
        }
        gatherAt = Infinity;
        captured = released = false;
        spread = spreadTo = 0;
        mode = "ambient";
        into = null;
    }

    /** The mark has moved (the window changed size): the stars aim at it where it now is. */
    function retarget() {
        if (released) return;
        const points = sampleMark();
        parts.forEach((p, i) => {
            const q = points[i % points.length];
            p.tx = q[0];
            p.ty = q[1];
        });
    }

    /** From `at`, the stars fly into the mark's core, arriving over `lasting`. Both in ms on the clock `draw` is given. */
    function gather(at, lasting) {
        gatherAt = at;
        gatherFor = lasting;
    }

    /** The mark bursts: every star is thrown out from it, and the field is a backdrop from then on. */
    function release() {
        if (released) return;
        released = true;
        for (const p of parts) {
            if (reduced) {
                p.x = p.sx; p.y = p.sy; p.vx = p.vy = 0;
                p.a = .1 + .5 * p.z * p.z;
                continue;
            }
            const angle = Math.atan2(p.y - markY, p.x - markX) + (Math.random() - .5) * .9;
            const speed = (3 + Math.random() * 10) * (.45 + p.z);
            p.vx = Math.cos(angle) * speed;
            p.vy = Math.sin(angle) * speed;
            p.a = 1;
        }
    }

    // Where each star goes for the field to be even. Every star keeps its
    // direction from the middle and its order outward among its neighbours,
    // so the cloud smooths out rather than stars trading places: directions
    // are shared out so that each part of the screen gets its share of stars,
    // and distances go by rank, square-rooted, so each direction fills evenly
    // from the middle to the edge.
    function evenTargets() {
        const cx = W / 2, cy = H / 2, hw = W / 2 - 10, hh = H / 2 - 10;
        // How far the edge of the screen is, in a direction.
        const edge = (angle) => Math.min(hw / Math.max(1e-6, Math.abs(Math.cos(angle))), hh / Math.max(1e-6, Math.abs(Math.sin(angle))));
        // The share of the screen's area that lies before each direction.
        const STEPS = 720;
        const before = new Float64Array(STEPS + 1);
        for (let i = 0; i < STEPS; i++) before[i + 1] = before[i] + edge(-Math.PI + (i + .5) * 2 * Math.PI / STEPS) ** 2;
        for (let i = 0; i <= STEPS; i++) before[i] /= before[STEPS];
        const angleAt = (share) => {
            let lo = 0, hi = STEPS;
            while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (before[mid] < share) lo = mid; else hi = mid; }
            return -Math.PI + (lo + (share - before[lo]) / ((before[hi] - before[lo]) || 1)) * 2 * Math.PI / STEPS;
        };
        const gap = Math.sqrt(W * H / parts.length);
        const stars = parts
            .map((p) => ({ p, angle: Math.atan2(p.y - cy, p.x - cx), out: Math.hypot(p.x - cx, p.y - cy) }))
            .sort((a, b) => a.angle - b.angle);
        stars.forEach((star, i) => (star.to = angleAt((i + .5) / stars.length)));
        const NEIGHBOURS = 28;
        for (let from = 0; from < stars.length; from += NEIGHBOURS) {
            const group = stars.slice(from, from + NEIGHBOURS).sort((a, b) => a.out - b.out);
            group.forEach(({ p, to }, rank) => {
                const out = edge(to) * Math.sqrt((rank + Math.random()) / group.length);
                p.ex = cx + Math.cos(to) * out + (Math.random() - .5) * gap * .5;
                p.ey = cy + Math.sin(to) * out + (Math.random() - .5) * gap * .5;
            });
        }
    }

    /** Evens the field out across the screen by `to`, from 0 (as it drifts)
        to 1 (wholly even). It gets there over a few frames. Only a drifting
        field is evened: one that is draining or stopped is left to that. */
    function even(to) {
        if (reduced || !released || mode !== "ambient") return;
        // Setting out from rest, each star is given its place afresh; one
        // that is still falling back carries on toward the place it had.
        if (to > 0 && spreadTo === 0 && spread < .02) evenTargets();
        spreadTo = to;
    }

    /** The stars drain into a place on the page. `where` is asked each frame
        where that is: `left`, `width`, `y` (its middle) and `h` in page
        pixels, and, while `edge` is set, the one point `ex` along it that
        they all make for. Nothing, once the place has gone. */
    function stream(where) {
        if (reduced || !released || mode === "stream") return;
        if (mode === "stall") wake();
        for (const p of parts) {
            // Each sets off from where it is drawn, all of them within six
            // seconds: about the rate the steady stream then runs at.
            [p.x, p.y] = drawnAt(p);
            p.vx = p.vy = 0;
            p.st = true;
            p.wait = now + Math.random() * 6000;
            p.tu = Math.random();
            p.tv = Math.random();
        }
        streaming = parts.length;
        streamCap = Math.round(parts.length * STREAM_SHARE);
        closed = false;
        spread = spreadTo = 0;
        into = where;
        mode = "stream";
    }

    /** No more stars set out: the place keeps the ones it has taken in. */
    function hold() {
        if (mode === "stream") closed = true;
    }

    // A star the stream has taken in sets out again, from far off.
    function respawn(p, place, t) {
        const angle = Math.random() * Math.PI * 2;
        const out = Math.hypot(W, H) * (.55 + Math.random() * .35);
        p.x = place.left + place.width / 2 + Math.cos(angle) * out;
        p.y = place.y + Math.sin(angle) * out;
        p.vx = p.vy = 0;
        p.a = 0;
        p.wait = t + Math.random() * 300;
        p.tu = Math.random();
        p.tv = Math.random();
    }

    /** What the stream carried comes back out of `from` (a rectangle, in
        page pixels) at once, and the field drifts again behind it. Whether
        there was a stream to let out. */
    function finale(from) {
        if (mode !== "stream") return false;
        const cx = from.left + from.width / 2, cy = from.top + from.height / 2;
        for (const p of parts) {
            let angle;
            if (p.st && now >= p.wait) {
                // On its way in: thrown back the way it came.
                angle = Math.atan2(p.y - cy, p.x - cx) + (Math.random() - .5) * .8;
            } else if (Math.random() < .45) {
                // Taken in: out of wherever in the place it went.
                p.x = from.left + Math.random() * from.width;
                p.y = from.top + Math.random() * from.height;
                angle = Math.random() * Math.PI * 2;
            } else {
                // The rest fade back in where they can be seen.
                p.x = Math.random() * W;
                p.y = Math.random() * H;
                p.vx = p.vy = p.a = 0;
                p.st = false;
                continue;
            }
            const speed = (2 + Math.random() * 9) * (.45 + p.z);
            p.vx = Math.cos(angle) * speed;
            p.vy = Math.sin(angle) * speed;
            p.a = 1;
            p.st = false;
        }
        into = null;
        mode = "ambient";
        return true;
    }

    /** Something went wrong. Stars the stream had taken in stay taken; the
        rest coast to a stop where they are, and dim. Out of the dark (a
        machine that never came back) they come up scattered and faint. */
    function stall() {
        if (reduced || !released || mode === "stall") return;
        for (const p of parts) {
            if (mode === "stream") {
                p.hid = !p.st;
            } else if (mode === "dark" || mode === "gather") {
                p.x = Math.random() * W;
                p.y = Math.random() * H;
                p.vx = p.vy = p.a = 0;
                p.hid = false;
            } else {
                [p.x, p.y] = drawnAt(p);
                p.hid = false;
            }
        }
        spread = spreadTo = 0;
        into = null;
        mode = "stall";
    }

    /** Every star, wherever it is, flies into the mark's core as it now
        sits, the way the intro made it: the machine is going down. */
    function regather() {
        if (reduced || !released) return;
        const points = sampleMark();
        parts.forEach((p, i) => {
            const q = points[i % points.length];
            if (mode === "stream" ? !p.st : mode === "stall" && p.hid) {
                // Out of sight: it comes in from wherever, faintly.
                p.fx = Math.random() * W;
                p.fy = Math.random() * H;
                p.fa = 0;
            } else {
                [p.fx, p.fy] = mode === "ambient" ? drawnAt(p) : [p.x, p.y];
                p.fa = p.a;
            }
            p.tx = q[0] + (Math.random() - .5) * .8;
            p.ty = q[1] + (Math.random() - .5) * .8;
            p.delay = Math.random() * 450;
            p.hid = p.st = false;
        });
        spread = spreadTo = 0;
        into = null;
        regatherAt = now;
        fadeAt = 0;
        mode = "gather";
    }

    /** The gathered stars fade into the solid mark. */
    function fade() {
        if (mode === "gather") fadeAt = now;
    }

    /** No stars at all: the machine is away. */
    function darken() {
        if (!reduced && released) mode = "dark";
    }

    /** A new field, out, as a restart leaves it while the machine is away:
        for a page that arrives while the mark sleeps, to wake with `burst`. */
    function asleep() {
        seed();
        // With less motion there is no burst to wake to: the field is simply
        // there, as it is after the intro.
        if (reduced) return release();
        released = true;
        mode = "dark";
    }

    /** The machine is back: every star is thrown out of the mark, as the
        intro's burst threw them, and the field drifts again. */
    function burst() {
        if (reduced || !released) return;
        const points = sampleMark();
        parts.forEach((p, i) => {
            const q = points[i % points.length];
            p.x = q[0];
            p.y = q[1];
            const angle = Math.atan2(p.y - markY, p.x - markX) + (Math.random() - .5) * .9;
            const speed = (3 + Math.random() * 10) * (.45 + p.z);
            p.vx = Math.cos(angle) * speed;
            p.vy = Math.sin(angle) * speed;
            p.a = 1;
            p.hid = p.st = false;
            p.ex = p.ey = undefined;
        });
        spread = spreadTo = 0;
        into = null;
        mode = "ambient";
    }

    /** Back to the ordinary drift, from a stream or a stop. Stars that were
        out of sight fade back in where they can be seen. */
    function wake() {
        if (mode === "ambient") return;
        if (mode === "gather" || mode === "dark") return burst();
        for (const p of parts) {
            if (mode === "stream" ? !p.st : p.hid) {
                p.x = Math.random() * W;
                p.y = Math.random() * H;
                p.vx = p.vy = p.a = 0;
            }
            p.hid = p.st = false;
        }
        into = null;
        mode = "ambient";
    }

    /** The field as it is drawn now, for a page that is to take it up where
        it is: the clock its drift and glints go by, and seven numbers a star
        (where it is drawn, how near, its phase, its colour's place in the
        palette, whether it glints, how bright), as GXWI's logon pages read
        them. Only a drifting field can be taken up; anything else is none. */
    function snapshot() {
        if (!released || mode !== "ambient") return null;
        const stars = [];
        for (const p of parts) {
            const [x, y] = drawnAt(p);
            stars.push(round(x, 1), round(y, 1), round(p.z, 3), round(p.ph, 3), p.ci, p.glint ? 1 : 0, round(clamp(p.a, 0, 1), 3));
        }
        return { w: W, h: H, t: now, stars };
    }

    // The slow drift the field keeps once the burst has spent itself. A
    // star's place in an even field drifts with it, so an evened field moves
    // as the cloud did.
    function drift(p, t) {
        const dvx = Math.cos(p.ph + t * .00013) * .14 * (.3 + p.z);
        const dvy = Math.sin(p.ph * 1.3 + t * .00011) * .1 * (.3 + p.z) - .04 * p.z;
        p.vx += (dvx - p.vx) * .025;
        p.vy += (dvy - p.vy) * .025;
        p.x += p.vx;
        p.y += p.vy;
        if (p.x < -30) p.x += W + 60; else if (p.x > W + 30) p.x -= W + 60;
        if (p.y < -30) p.y += H + 60; else if (p.y > H + 30) p.y -= H + 60;
        if (p.ex !== undefined) {
            p.ex += p.vx;
            p.ey += p.vy;
            if (p.ex < -30) p.ex += W + 60; else if (p.ex > W + 30) p.ex -= W + 60;
            if (p.ey < -30) p.ey += H + 60; else if (p.ey > H + 30) p.ey -= H + 60;
        }
    }

    // A four-point sparkle, brightest for a moment every few seconds.
    function glint(p, x, y, t) {
        const g = Math.pow(Math.max(0, Math.sin(t * .0011 + p.ph * 5)), 14);
        if (g < .02) return;
        const L = 1.5 + g * (5 + p.z * 7);
        ctx.globalAlpha = g * .95;
        ctx.fillStyle = "#e8fbff";
        ctx.fillRect(x - L, y - .5, L * 2, 1);
        ctx.fillRect(x - .5, y - L, 1, L * 2);
        ctx.globalAlpha = g * .5;
        ctx.fillRect(x - 1.5, y - 1.5, 3, 3);
    }

    /** One frame, at `t` ms on the intro's clock. */
    function draw(t) {
        now = t;
        smx += (mx - smx) * .04;
        smy += (my - smy) * .04;
        spread += (spreadTo - spread) * .12;
        ctx.clearRect(0, 0, W, H);
        ctx.globalCompositeOperation = "lighter";
        // Where the stream goes this frame. With nowhere to go, it is over.
        const place = mode === "stream" ? into() : null;
        if (mode === "stream" && !place) wake();

        // The moment the gathering starts, each star sets off from wherever
        // its drift has taken it.
        if (!captured && !released && t >= gatherAt) {
            for (const p of parts) { p.fx = driftX(p, t); p.fy = driftY(p, t); }
            captured = true;
        }

        for (const p of parts) {
            let x, y, a, s;
            if (released && mode === "dark") continue;
            if (released && mode === "gather") {
                // The intro's convergence, run again: from wherever each
                // star was, straight into the mark's core, and then, once
                // the mark is solid, gone into it.
                const e = easeInOut(clamp((t - regatherAt - p.delay) / 1500, 0, 1));
                x = p.fx + (p.tx - p.fx) * e;
                y = p.fy + (p.ty - p.fy) * e;
                a = (p.fa + (.95 - p.fa) * e) * (fadeAt ? Math.max(0, 1 - (t - fadeAt) / 600) : 1);
                s = (.7 + p.z * 1.5) * (1 - e) + 2.2 * e;
                p.x = x; p.y = y; p.a = a;
            } else if (released && mode === "stall") {
                // Whatever was moving coasts to a stop, keeping only the
                // faintest drift, and the field dims.
                if (p.hid) continue;
                p.vx += (Math.cos(p.ph + t * .00013) * .03 * (.3 + p.z) - p.vx) * .04;
                p.vy += (Math.sin(p.ph * 1.3 + t * .00011) * .02 * (.3 + p.z) - p.vy) * .04;
                p.x += p.vx;
                p.y += p.vy;
                if (p.x < -30) p.x += W + 60; else if (p.x > W + 30) p.x -= W + 60;
                if (p.y < -30) p.y += H + 60; else if (p.y > H + 30) p.y -= H + 60;
                p.a += ((.05 + .22 * p.z * p.z) - p.a) * .02;
                x = p.x;
                y = p.y;
                a = p.a;
                s = .7 + p.z * 1.3;
            } else if (released && mode === "stream") {
                if (!p.st) continue;
                if (t < p.wait) {
                    // Not set off yet: it drifts on from where it was.
                    drift(p, t);
                    p.a += ((.1 + .5 * p.z * p.z) - p.a) * .03;
                    x = p.x;
                    y = p.y;
                    a = p.a;
                    s = .7 + p.z * 1.5;
                } else {
                    // Pulled straight at its own spot in the place, or at
                    // the one point they all make for while there is one.
                    const tx = place.edge ? place.ex : place.left + p.tu * place.width;
                    const ty = place.y + (p.tv - .5) * place.h * .6;
                    const dx = tx - p.x, dy = ty - p.y;
                    const d = Math.hypot(dx, dy) || 1;
                    const pull = .38 * (.6 + p.z);
                    p.vx = p.vx * .95 + dx / d * pull;
                    p.vy = p.vy * .95 + dy / d * pull;
                    p.x += p.vx;
                    p.y += p.vy;
                    if (d < 12) {
                        // Taken in. It sets out again from far off, unless
                        // the stream is still bigger than it settles at, or
                        // the place is only keeping what it has.
                        if (streaming > streamCap || closed) { p.st = false; streaming--; continue; }
                        respawn(p, place, t);
                    }
                    p.a = Math.min(.95, p.a + .03);
                    x = p.x;
                    y = p.y;
                    a = p.a;
                    s = .8 + p.z * 1.4;
                    // A short tail, the way it came.
                    ctx.globalAlpha = a * .45;
                    ctx.strokeStyle = p.col;
                    ctx.lineWidth = s * .8;
                    ctx.beginPath();
                    ctx.moveTo(x - p.vx * 3, y - p.vy * 3);
                    ctx.lineTo(x, y);
                    ctx.stroke();
                }
            } else if (released) {
                if (!reduced) drift(p, t);
                p.a += ((.1 + .5 * p.z * p.z) - p.a) * .03;
                [x, y] = drawnAt(p);
                a = p.a;
                s = .7 + p.z * 1.5;
            } else if (!captured) {
                x = driftX(p, t);
                y = driftY(p, t);
                a = Math.min(1, t / 600) * (.15 + .55 * p.z);
                s = .6 + p.z * 1.8;
                p.x = x; p.y = y; p.a = a;
            } else {
                const e = easeInOut(clamp((t - gatherAt - p.delay) / gatherFor, 0, 1));
                x = p.fx + (p.tx - p.fx) * e;
                y = p.fy + (p.ty - p.fy) * e;
                const base = .15 + .55 * p.z;
                a = base + (.95 - base) * e;
                s = (.6 + p.z * 1.8) * (1 - e) + 2.2 * e;
                p.x = x; p.y = y; p.a = a;
            }
            ctx.globalAlpha = a;
            ctx.fillStyle = p.col;
            ctx.fillRect(x - s / 2, y - s / 2, s, s);
            if (p.glint && released && !reduced && mode === "ambient") glint(p, x, y, t);
        }
        ctx.globalAlpha = 1;
    }

    window.addEventListener("pointermove", (e) => {
        mx = (e.clientX / W - .5) * 2;
        my = (e.clientY / H - .5) * 2;
    });
    window.addEventListener("resize", () => {
        resize();
        retarget();
    });

    return { seed, gather, release, even, stream, hold, finale, stall, wake, regather, fade, darken, asleep, burst, draw, snapshot };
}
