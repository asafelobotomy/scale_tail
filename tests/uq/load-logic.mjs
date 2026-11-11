// Loads uq/pages/scale-tail/logic.js for Node without Qt: the `.pragma` line is dropped and the
// file runs in a bare context, whose top-level functions and vars are the page's API.
import { readFileSync } from "node:fs";
import vm from "node:vm";

const root = new URL("../../uq/pages/scale-tail/", import.meta.url);

export function loadLogic() {
    const source = readFileSync(new URL("logic.js", root), "utf8")
        .split("\n")
        .filter(line => !/^\s*\.(pragma|import)\b/.test(line))
        .join("\n");
    const context = vm.createContext({});
    vm.runInContext(source, context);
    return context;
}

export function readPage(name) {
    return readFileSync(new URL(name, root), "utf8");
}
