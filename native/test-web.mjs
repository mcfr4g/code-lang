import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { once } from "node:events";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { Worker } from "node:worker_threads";

const binary = resolve(process.argv[2] ?? "./bin/code-lang");
const docs = new URL("../docs/", import.meta.url);
const examples = new URL("../examples/", import.meta.url);
const bundleScript = await readFile(new URL("code-web-data.js", docs), "utf8");
const runner = await readFile(new URL("playground.js", docs), "utf8");
const sandbox = {};
vm.runInNewContext(bundleScript, sandbox);
const bundle = sandbox.CodePlaygroundBundle;
const bytecode = Uint8Array.from(Buffer.from(bundle.wasm, "base64"));
assert.ok(WebAssembly.validate(bytecode));

function launchWorker() {
  return new Worker(`
    const { parentPort } = require("node:worker_threads");
    const self = { postMessage: (result) => parentPort.postMessage(result) };
    ${runner}
    codePlaygroundWorker();
    parentPort.on("message", (data) => self.onmessage({ data }));
  `, { eval: true });
}

async function run(source, answers = []) {
  const worker = launchWorker();
  const timeout = AbortSignal.timeout(5000);
  try {
    const response = once(worker, "message", { signal: timeout });
    worker.postMessage({ bytecode, source, answers });
    return (await response)[0];
  } finally {
    await worker.terminate();
  }
}

const exampleAnswers = { "ask.code": ["Y"], "gob.code": ["N"] };
for (const [name, source] of Object.entries(bundle.examples)) {
  const path = fileURLToPath(new URL(name, examples));
  assert.equal(source, await readFile(path, "utf8"), name + " is stale");
  const answers = exampleAnswers[name] ?? [];
  const nativeOutput = execFileSync(binary, [path], {
    encoding: "utf8",
    input: answers.length ? answers.join("\n") + "\n" : "",
    timeout: 5000,
  });
  const result = await run(source, answers);
  assert.deepEqual(result, { output: nativeOutput, error: "" }, name);
}

const gob = await run(bundle.examples["gob.code"], ["Y"]);
const gobPrompt = "Gob's Program: Y/N?\n";
assert.match(gob.error, /^Line 5: Output error: playground output limit reached/);
assert.ok(gob.output.startsWith(gobPrompt + "Penus Penus "));
assert.match(gob.output.slice(gobPrompt.length), /^(?:Penus )+$/);
assert.ok(!gob.output.endsWith("\n"));

const questions = 'ask "First?" and remember as first\nask "Second?" and remember as second\nsay first\nsay second';
assert.deepEqual(await run(questions), {
  output: "First?\n", error: "", waitingForInput: true,
});
assert.deepEqual(await run(questions, ["Y"]), {
  output: "First?\nSecond?\n", error: "", waitingForInput: true,
});
assert.deepEqual(await run(questions, ["Y", ""]), {
  output: "First?\nSecond?\nY\n\n", error: "",
});

assert.deepEqual(await run('say "Hi"\r\nsay missing\r\n'), {
  output: "Hi\n",
  error: "Line 2: Unknown variable 'missing'",
});
assert.deepEqual(await run('say "Hi"\nnote: §'), {
  output: "",
  error: "Line 2: We must never speak of §!",
});
const limited = await run('while true:\n    say "turnip"\nok\n');
assert.match(limited.error, /playground output limit reached/);
assert.ok(Buffer.byteLength(limited.output) <= 64 * 1024);

const loop = launchWorker();
try {
  await once(loop, "online", { signal: AbortSignal.timeout(5000) });
  let completed = false;
  loop.on("message", () => { completed = true; });
  loop.postMessage({ bytecode, source: "while true:\nok\n" });
  await sleep(200);
  assert.equal(completed, false);
  await loop.terminate();
} finally {
  await loop.terminate();
}

console.log(`WebAssembly matched ${Object.keys(bundle.examples).length} examples and passed input, error, output-limit, and worker-stop checks.`);
