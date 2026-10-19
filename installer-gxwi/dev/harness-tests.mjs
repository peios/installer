// Exercise shell wiring with fake binaries and files, never a VM or host disk.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
    chmodSync,
    cpSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    rmSync,
    writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scripts = dirname(fileURLToPath(import.meta.url));

function executable(path, source) {
    writeFileSync(path, source);
    chmodSync(path, 0o755);
}

function fixture(t) {
    const root = mkdtempSync(join(tmpdir(), "installer harness "));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    const dev = join(root, "installer-gxwi/dev");
    mkdirSync(dev, { recursive: true });
    for (const name of ["env.sh", "host.sh", "host-oobe.sh", "boot.sh", "push.sh", "overlay.sh"]) {
        cpSync(join(scripts, name), join(dev, name));
    }
    const sdk = join(root, "sdk");
    mkdirSync(join(sdk, "lib"), { recursive: true });
    mkdirSync(join(root, "bin"));
    const log = join(root, "calls");
    const env = { ...process.env, PEIOS_SDK: sdk, CALL_LOG: log };
    for (const name of ["installerd", "oobed", "installer-gxwi", "oobe-gxwi"]) {
        executable(
            join(root, "bin", name),
            `#!/bin/sh
${name.endsWith("d") ? "" : 'count=0; while [ ! -e "$CALL_LOG.ready" ]; do count=$((count + 1)); [ "$count" -lt 100 ] || exit 1; sleep 0.01; done'}
printf '%s\\n' '${name}' "$@" >> "$CALL_LOG"
${name.endsWith("d") ? 'touch "$CALL_LOG.ready"' : ""}
`,
        );
    }
    executable(
        join(root, "bazel"),
        `#!/bin/sh
if [ "$1" = cquery ]; then
    for arg do target=$arg; done
    name=\${target##*:}
    name=\${name##*/}
    printf '%s/bin/%s\\n' "$PWD" "$name"
fi
`,
    );
    return {
        root,
        dev,
        env,
        log,
        run(name, args = [], extra = {}) {
            return spawnSync("sh", [join(dev, name), ...args], {
                cwd: root,
                env: { ...env, ...extra },
                encoding: "utf8",
                timeout: 5000,
            });
        },
    };
}

for (const [script, daemon, frontend, port] of [
    ["host.sh", "installerd", "installer-gxwi", "7790"],
    ["host-oobe.sh", "oobed", "oobe-gxwi", "7791"],
]) {
    test(`${script} starts only a dry-run daemon and its matching frontend`, (t) => {
        const f = fixture(t);
        writeFileSync(join(f.dev, "net-status.txt"), "");
        const result = f.run(script);
        assert.equal(result.status, 0, result.stderr);
        const calls = readFileSync(f.log, "utf8");
        assert.match(calls, new RegExp(`${daemon}\\n--dry-run\\n--socket\\n`));
        assert.match(calls, new RegExp(`${frontend}\\n--socket\\n`));
        assert.match(calls, new RegExp(`--listen\\n127\\.0\\.0\\.1:${port}\\n`));
    });
}

test("boot passes explicit image and individual QEMU arguments to the runner", (t) => {
    const f = fixture(t);
    const image = join(f.root, "input image");
    writeFileSync(image, "fixture");
    executable(join(f.dev, "disks.sh"), "#!/bin/sh\nmkdir -p target/disks\n");
    const runner = join(f.root, "runner");
    executable(runner, '#!/bin/sh\nprintf "%s\\n" "$@" > "$CALL_LOG"\n');
    const result = f.run("boot.sh", [], { IMAGE: image, VM_RUNNER: runner });
    assert.equal(result.status, 0, result.stderr);
    const args = readFileSync(f.log, "utf8").trimEnd().split("\n");
    assert.equal(args[0], image);
    assert.equal(args[1], "-device");
    assert.equal(args[2], "ahci,id=sata");
    assert.ok(
        args.includes(
            `if=none,id=blank,format=raw,file=${f.root}/installer-gxwi/target/disks/blank.img`,
        ),
    );
    assert.equal(readFileSync(image, "utf8"), "fixture");
});

test("boot rejects missing explicit inputs before running a fixture builder", (t) => {
    const f = fixture(t);
    executable(join(f.dev, "disks.sh"), '#!/bin/sh\necho unexpected > "$CALL_LOG"\n');
    const result = f.run("boot.sh", [], { IMAGE: "", VM_RUNNER: "" });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /set IMAGE/);
});

test("push installs complete Bazel outputs into the explicit share", (t) => {
    const f = fixture(t);
    const share = join(f.root, "vm share");
    mkdirSync(share);
    const result = f.run("push.sh", [], { VM_SHARE: share });
    assert.equal(result.status, 0, result.stderr);
    for (const name of ["installerd", "installer-gxwi"]) {
        assert.deepEqual(readFileSync(join(share, name)), readFileSync(join(f.root, "bin", name)));
    }
});

test("overlay calls only the explicitly selected guest command runner", (t) => {
    const f = fixture(t);
    const runner = join(f.root, "guest runner");
    executable(runner, '#!/bin/sh\nprintf "%s\\n" "$@" > "$CALL_LOG"\n');
    const result = f.run("overlay.sh", ["on"], { VM_GUEST: runner });
    assert.equal(result.status, 0, result.stderr);
    assert.match(readFileSync(f.log, "utf8"), /OverlaySession sz:\/tmp\/installer-gxwi/);
});
