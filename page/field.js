// The field of stars behind everything.
//
// During the intro the stars drift, fly straight into the mark's core to
// make it, and are thrown back out when it bursts. After that they are the
// backdrop: a slow drift that answers the pointer a little, a few of them
// catching the light now and then.
//
// The field knows nothing of what the installer is doing. It is told when to
// gather and when to let go, and draws one frame when asked.

const PALETTE = [["#62d2ff", .55], ["#c8f1ff", .25], ["#9d8cff", .12], ["#4fd1c5", .08]];

const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const easeInOut = (k) => (k < .5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);
const pick = () => {
    let r = Math.random();
    for (const [colour, weight] of PALETTE) if ((r -= weight) <= 0) return colour;
    return PALETTE[0][0];
};

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
            parts.push({
                sx: Math.random() * W, sy: Math.random() * H, z,
                tx: q[0] + (Math.random() - .5) * .8, ty: q[1] + (Math.random() - .5) * .8,
                x: 0, y: 0, fx: 0, fy: 0, vx: 0, vy: 0, a: 0,
                delay: Math.random() * 450,
                ph: Math.random() * Math.PI * 2,
                col: pick(),
                // A few near stars catch the light now and then.
                glint: z > .55 && Math.random() < .0125,
            });
        }
        gatherAt = Infinity;
        captured = released = false;
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

    // The slow drift the field keeps once the burst has spent itself.
    function drift(p, t) {
        const dvx = Math.cos(p.ph + t * .00013) * .14 * (.3 + p.z);
        const dvy = Math.sin(p.ph * 1.3 + t * .00011) * .1 * (.3 + p.z) - .04 * p.z;
        p.vx += (dvx - p.vx) * .025;
        p.vy += (dvy - p.vy) * .025;
        p.x += p.vx;
        p.y += p.vy;
        if (p.x < -30) p.x += W + 60; else if (p.x > W + 30) p.x -= W + 60;
        if (p.y < -30) p.y += H + 60; else if (p.y > H + 30) p.y -= H + 60;
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
        smx += (mx - smx) * .04;
        smy += (my - smy) * .04;
        ctx.clearRect(0, 0, W, H);
        ctx.globalCompositeOperation = "lighter";

        // The moment the gathering starts, each star sets off from wherever
        // its drift has taken it.
        if (!captured && !released && t >= gatherAt) {
            for (const p of parts) { p.fx = driftX(p, t); p.fy = driftY(p, t); }
            captured = true;
        }

        for (const p of parts) {
            let x, y, a, s;
            if (released) {
                if (!reduced) drift(p, t);
                p.a += ((.1 + .5 * p.z * p.z) - p.a) * .03;
                x = p.x + smx * p.z * 22;
                y = p.y + smy * p.z * 22;
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
            if (p.glint && released && !reduced) glint(p, x, y, t);
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

    return { seed, gather, release, draw };
}
