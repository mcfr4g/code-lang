"use strict";

function codePlaygroundWorker() {
  self.onmessage = async ({ data: { bytecode, source, answers = [] } }) => {
    try {
      const encoder = new TextEncoder();
      const input = encoder.encode(source);
      const replies = encoder.encode(answers.length ? answers.join("\n") + "\n" : "");
      if (input.length > 64 * 1024) {
        self.postMessage({ output: "", error: "Source is larger than 64 KiB." });
        return;
      }
      if (replies.length > 64 * 1024) {
        self.postMessage({ output: "", error: "Input is larger than 64 KiB." });
        return;
      }
      const { instance } = await WebAssembly.instantiate(bytecode);
      const code = instance.exports;
      const inputPointer = code.code_input(input.length);
      new Uint8Array(code.memory.buffer, inputPointer, input.length).set(input);
      const repliesPointer = code.code_answers(replies.length);
      new Uint8Array(code.memory.buffer, repliesPointer, replies.length).set(replies);
      const result = code.code_run();
      const decoder = new TextDecoder();
      const read = (pointer, length) =>
        decoder.decode(new Uint8Array(code.memory.buffer, pointer, length));
      const output = read(code.code_output_ptr(), code.code_output_len());
      if (result === 2) {
        self.postMessage({ output, error: "", waitingForInput: true });
        return;
      }
      const error = result === 0 ? "" : read(code.code_error_ptr(), code.code_error_len());
      self.postMessage({ output, error });
    } catch {
      self.postMessage({ output: "", error: "The browser interpreter could not complete this run." });
    }
  };
}

function indentCode(editor) {
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const indent = "    ";
  if (start === end) {
    editor.setRangeText(indent, start, end, "end");
    return;
  }
  const firstLine = start === 0 ? 0 : editor.value.lastIndexOf("\n", start - 1) + 1;
  const lastCharacter = editor.value[end - 1] === "\n" ? end - 1 : end;
  const selection = editor.value.slice(firstLine, lastCharacter);
  const replacement = indent + selection.replace(/\n/g, "\n" + indent);
  editor.setRangeText(replacement, firstLine, lastCharacter);
  editor.setSelectionRange(start + indent.length, end + replacement.length - selection.length);
}

function attachCodePlayground() {
  const bundle = globalThis.CodePlaygroundBundle;
  const playground = document.getElementById("playground-controls");
  const unavailable = document.getElementById("playground-unavailable");
  if (!bundle || typeof WebAssembly !== "object" || typeof Worker !== "function") {
    unavailable.hidden = false;
    return;
  }

  const chooser = document.getElementById("playground-example");
  const editor = document.getElementById("playground-source");
  const output = document.getElementById("playground-output");
  const status = document.getElementById("playground-status");
  const runButton = document.getElementById("playground-run");
  const stopButton = document.getElementById("playground-stop");
  const resetButton = document.getElementById("playground-reset");
  const answerForm = document.getElementById("playground-input");
  const answerField = document.getElementById("playground-answer");
  const bytecode = Uint8Array.from(atob(bundle.wasm), (character) => character.charCodeAt(0));
  let activeRun = null;
  let runSource = "";
  const answers = [];
  let moveFocusOnTab = false;

  for (const name of Object.keys(bundle.examples)) {
    chooser.add(new Option(name, name));
  }

  function setStatus(text, state = "ready") {
    status.textContent = text;
    status.dataset.state = state;
    const inProgress = state === "running" || state === "waiting";
    runButton.disabled = inProgress;
    stopButton.disabled = !inProgress;
    editor.readOnly = inProgress;
    answerForm.hidden = state !== "waiting";
  }

  function closeRun() {
    if (!activeRun) return;
    clearTimeout(activeRun.timer);
    activeRun.worker.terminate();
    URL.revokeObjectURL(activeRun.url);
    activeRun = null;
  }

  function loadExample(name) {
    closeRun();
    chooser.value = name;
    editor.value = bundle.examples[name];
    output.textContent = "";
    setStatus(`${name} loaded.`);
  }

  function run() {
    if (runButton.disabled) return;
    runSource = editor.value;
    answers.length = 0;
    output.textContent = "";
    execute();
  }

  function execute() {
    let url;
    let worker;
    try {
      const workerScript = `(${codePlaygroundWorker.toString()})();`;
      url = URL.createObjectURL(new Blob([workerScript], { type: "text/javascript" }));
      worker = new Worker(url);
    } catch {
      if (url) URL.revokeObjectURL(url);
      setStatus("This browser couldn’t start the WebAssembly runner.", "error");
      return;
    }

    const job = { worker, url, timer: undefined };
    activeRun = job;
    setStatus("Running…", "running");

    worker.onmessage = ({ data }) => {
      if (activeRun !== job) return;
      closeRun();
      const separator = data.output && data.error && !data.output.endsWith("\n") ? "\n" : "";
      output.textContent = data.output + separator + data.error;
      if (data.waitingForInput) {
        output.scrollTop = output.scrollHeight;
        setStatus("Waiting for an answer.", "waiting");
        answerField.value = "";
        answerField.focus();
        return;
      }
      setStatus(data.error ? "Program stopped with an error." : "Run complete.", data.error ? "error" : "complete");
    };
    worker.onerror = (event) => {
      event.preventDefault();
      if (activeRun !== job) return;
      closeRun();
      setStatus("This browser couldn’t start the WebAssembly runner.", "error");
    };
    job.timer = setTimeout(() => {
      if (activeRun !== job) return;
      closeRun();
      setStatus("Stopped after three seconds.", "error");
    }, 3000);
    // Code has no external effects other than input and output. Replay with
    // the recorded answers and replace the displayed output after each run.
    worker.postMessage({ bytecode, source: runSource, answers });
  }

  chooser.addEventListener("change", () => loadExample(chooser.value));
  resetButton.addEventListener("click", () => loadExample(chooser.value));
  runButton.addEventListener("click", run);
  stopButton.addEventListener("click", () => {
    closeRun();
    setStatus("Stopped.");
  });
  answerForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (answerForm.hidden) return;
    answers.push(answerField.value);
    execute();
  });
  editor.addEventListener("blur", () => { moveFocusOnTab = false; });
  editor.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      moveFocusOnTab = true;
      return;
    }
    if (event.key === "Tab" && !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey) {
      if (!moveFocusOnTab && !editor.readOnly) {
        event.preventDefault();
        indentCode(editor);
      }
      moveFocusOnTab = false;
      return;
    }
    moveFocusOnTab = false;
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      run();
    }
  });

  for (const example of document.querySelectorAll("figure[data-program]")) {
    const name = example.dataset.program + ".code";
    if (!(name in bundle.examples)) continue;
    const caption = example.querySelector("figcaption");
    const copy = caption.querySelector("[data-copy]");
    const actions = document.createElement("div");
    actions.className = "code-actions";
    const tryButton = document.createElement("button");
    tryButton.type = "button";
    tryButton.className = "copy";
    tryButton.textContent = "Try it";
    tryButton.setAttribute("aria-label", "Open " + name + " in the browser playground");
    tryButton.addEventListener("click", () => {
      loadExample(name);
      window.location.hash = "playground";
      editor.focus({ preventScroll: true });
    });
    actions.append(tryButton);
    if (copy) actions.append(copy);
    caption.append(actions);
  }

  loadExample("apples.code");
  playground.hidden = false;
}

if (typeof document !== "undefined") attachCodePlayground();
