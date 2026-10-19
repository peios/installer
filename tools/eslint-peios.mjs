/** Enforce Peios's optional-brace policy without changing code automatically. */
const braces = {
    meta: {
        type: "layout",
        schema: [],
        messages: {
            required:
                "Use braces: the complete clause must fit one line within 100 columns, and all branches need braces when one requires them.",
        },
    },
    create(context) {
        const source = context.sourceCode;

        function needsBraces(start, body, end = body) {
            if (body.type === "BlockStatement" && body.body.length !== 1) return true;
            if (start.loc.start.line !== end.loc.end.line) return true;
            // AST columns are UTF-16 offsets. Count source characters and expand tabs
            // to the common four-column tab stop for this single-line eligibility check.
            const prefix = source.lines[end.loc.end.line - 1].slice(0, end.loc.end.column);
            let width = 0;
            for (const character of prefix) {
                width += character === "\t" ? 4 - (width % 4) : 1;
            }
            return width > 100;
        }

        function report(body) {
            if (body.type !== "BlockStatement") {
                context.report({ node: body, messageId: "required" });
            }
        }

        function checkIf(node) {
            if (node.parent.type === "IfStatement" && node.parent.alternate === node) return;
            const branches = [];
            let current = node;
            let start = node;
            while (current) {
                branches.push({ start, body: current.consequent });
                const alternate = current.alternate;
                if (!alternate) break;
                start = source.getTokenBefore(alternate); // The `else` token.
                if (alternate.type !== "IfStatement") {
                    branches.push({ start, body: alternate });
                    break;
                }
                current = alternate;
            }
            if (branches.some(({ start, body }) => needsBraces(start, body))) {
                branches.forEach(({ body }) => report(body));
            }
        }

        function checkLoop(node) {
            const end = node.type === "DoWhileStatement" ? node : node.body;
            if (needsBraces(node, node.body, end)) report(node.body);
        }

        return {
            IfStatement: checkIf,
            WhileStatement: checkLoop,
            DoWhileStatement: checkLoop,
            ForStatement: checkLoop,
            ForInStatement: checkLoop,
            ForOfStatement: checkLoop,
        };
    },
};

export default { rules: { braces } };
