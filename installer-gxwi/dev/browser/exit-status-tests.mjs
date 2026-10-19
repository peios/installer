// Check the actual final status statements without launching any browser/daemon.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

const directory = new URL("./", import.meta.url);
for (const name of readdirSync(directory).filter((name) => name.endsWith(".mjs"))) {
    const source = readFileSync(new URL(name, directory), "utf8");
    if (!/^const chrome = await browser\(/m.test(source)) continue;
    test(`${name} preserves exceptions while reporting explicit assertion failures`, () => {
        assert.doesNotMatch(source, /process\.exit\(/);
        const finalStatus = source.match(/process\.exitCode = [^;]+;/)?.[0];
        assert.ok(finalStatus, "the browser harness must report its assertions");
        for (const [body, failed, status] of [
            ['throw new Error("unexpected harness error");', [], 1],
            ["", [], 0],
            ["", ["assertion failed"], 1],
        ]) {
            const result = spawnSync(
                process.execPath,
                [
                    "--input-type=module",
                    "-e",
                    `const failed = ${JSON.stringify(failed)}; try { ${body} } finally { ${finalStatus} }`,
                ],
                { encoding: "utf8", timeout: 5000 },
            );
            assert.equal(result.status, status, result.stderr);
            if (body) assert.match(result.stderr, /unexpected harness error/);
        }
    });
}
