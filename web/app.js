// dsxray in the browser. The parser is Rust compiled to WebAssembly (dsxray.wasm, built from
// crates/dsxray-wasm). It has a C-style interface: copy the file into its memory, call
// dsx_inspect, read back a JSON answer. Everything stays on this page.

const $ = (id) => document.getElementById(id);
const MAX_HEX = 1024;
const SHORT = 140;
const KINDS = {
  header: "Header",
  allocator: "Allocator (block table, table of contents)",
  "tree-header": "Tree header (DSDB)",
  branch: "Branch node",
  leaf: "Leaf node",
  unused: "Unused or free",
};
const PARTS = [
  ["p-len", "name length"],
  ["p-name", "name (UTF-16)"],
  ["p-code", "code"],
  ["p-type", "type"],
  ["p-value", "value"],
];

const wasm = loadWasm();
let bytes = null;

async function loadWasm() {
  const response = await fetch("dsxray.wasm");
  try {
    return (await WebAssembly.instantiateStreaming(response.clone(), {})).instance.exports;
  } catch {
    return (await WebAssembly.instantiate(await response.arrayBuffer(), {})).instance.exports;
  }
}

async function inspect(data) {
  const x = await wasm;
  const ptr = x.dsx_alloc(data.length);
  new Uint8Array(x.memory.buffer, ptr, data.length).set(data);
  const packed = x.dsx_inspect(ptr, data.length);
  const outPtr = Number(packed >> 32n);
  const outLen = Number(packed & 0xffffffffn);
  const json = new TextDecoder().decode(new Uint8Array(x.memory.buffer, outPtr, outLen));
  x.dsx_free(outPtr, outLen);
  x.dsx_free(ptr, data.length);
  return JSON.parse(json);
}

async function open(data, label) {
  $("error").hidden = true;
  let answer;
  try {
    answer = await inspect(data);
  } catch (e) {
    return fail(`Couldn't run the parser: ${e.message}`);
  }
  if (!answer.ok) return fail(`${label}: ${answer.error}`);
  bytes = data;
  show(answer.store);
}

function fail(message) {
  $("error").textContent = message;
  $("error").hidden = false;
  $("result").hidden = true;
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "style") node.style.cssText = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) node.append(c);
  return node;
}

function show(store) {
  $("result").hidden = false;
  const nodes = store.regions.filter((r) => r.kind === "leaf" || r.kind === "branch").length;
  $("stats").replaceChildren(
    stat(store.names.length, "file names revealed"),
    stat(store.records.length, "records"),
    stat(nodes, nodes === 1 ? "tree node" : "tree nodes"),
    stat(store.file_size.toLocaleString(), "bytes"),
  );
  $("problem").hidden = !store.problem;
  $("problem").textContent = store.problem ? `Reading stopped early: ${store.problem}. Records before that are shown.` : "";

  $("names-count").textContent = `(${store.names.length})`;
  $("names").replaceChildren(...store.names.map((n) => el("li", {}, n)));

  drawByteMap(store);

  $("records-count").textContent = `(${store.records.length})`;
  const rows = store.records.map((r) => {
    const value = r.summary.length > SHORT ? r.summary.slice(0, SHORT - 1) + "…" : r.summary;
    const row = el("tr", {}, el("td", {}, r.name), el("td", {}, r.code), el("td", {}, value), el("td", {}, r.meaning ?? "unknown"));
    row.addEventListener("click", () => {
      select(row);
      showRecord(r);
    });
    return row;
  });
  $("records").tBodies[0].replaceChildren(...rows);
  if (rows.length) {
    select(rows[0]);
    showRecord(store.records[0]);
  }
  $("result").scrollIntoView({ behavior: "smooth", block: "start" });
}

function stat(value, label) {
  return el("div", { class: "stat" }, el("b", {}, String(value)), el("span", {}, label));
}

function select(node) {
  document.querySelectorAll(".on").forEach((n) => n.classList.remove("on"));
  node.classList.add("on");
}

function drawByteMap(store) {
  const parts = [];
  let at = 0;
  for (const r of store.regions) {
    if (r.start > at) parts.push({ start: at, end: r.start, kind: "unused", label: "unused or free space" });
    parts.push(r);
    at = Math.max(at, r.end);
  }
  if (at < store.file_size) parts.push({ start: at, end: store.file_size, kind: "unused", label: "unused or free space" });

  $("bytemap").replaceChildren(
    ...parts.map((p) => {
      const seg = el("button", {
        class: `seg k-${p.kind}`,
        style: `flex: ${p.end - p.start} 0 0`,
        title: `${p.label} · bytes ${p.start}–${p.end - 1}`,
        "aria-label": p.label,
        role: "listitem",
      });
      seg.addEventListener("click", () => {
        select(seg);
        showRegion(p, store);
      });
      return seg;
    }),
  );
  const kinds = [...new Set(parts.map((p) => p.kind))];
  $("legend").replaceChildren(...kinds.map((k) => el("li", {}, el("i", { class: `k-${k}` }), KINDS[k] ?? k)));
}

// A node: its 8-byte header, then its records in alternating shades, so the boundaries show.
function showRegion(p, store) {
  const node = p.kind === "leaf" || p.kind === "branch";
  const inside = node ? store.records.filter((r) => r.offset >= p.start && r.offset < p.end) : [];
  const marks = node ? [[p.start, p.start + 8, "r-head"]] : [];
  inside.forEach((r, i) => marks.push([r.offset, r.offset + r.length, i % 2 ? "r-b" : "r-a"]));
  showBytes(p.label, `bytes ${p.start.toLocaleString()} to ${(p.end - 1).toLocaleString()}`, p.start, p.end, marks);
  $("insp-legend").replaceChildren(
    ...(node ? [el("li", {}, el("i", { class: "r-head" }), "node header: right-most child, record count"), el("li", {}, el("i", { class: "r-a" }), el("i", { class: "r-b" }), "records, one shade each")] : []),
  );
  $("insp-legend").hidden = !node;
  $("decoded").hidden = true;
}

function showRecord(r) {
  const view = new DataView(bytes.buffer, bytes.byteOffset);
  const units = view.getUint32(r.offset);
  const nameEnd = r.offset + 4 + 2 * units;
  const marks = [
    [r.offset, r.offset + 4, "p-len"],
    [r.offset + 4, nameEnd, "p-name"],
    [nameEnd, nameEnd + 4, "p-code"],
    [nameEnd + 4, nameEnd + 8, "p-type"],
    [nameEnd + 8, r.offset + r.length, "p-value"],
  ];
  showBytes(`${r.code} · ${r.name}`, `${r.meaning ?? "Unknown code"} · ${r.type} · bytes ${r.offset.toLocaleString()} to ${(r.offset + r.length - 1).toLocaleString()}`, r.offset, r.offset + r.length, marks);
  $("insp-legend").replaceChildren(...PARTS.map(([cls, label]) => el("li", {}, el("i", { class: cls }), label)));
  $("insp-legend").hidden = false;
  const decoded = r.decoded ?? null;
  $("decoded").hidden = decoded === null;
  $("decoded").textContent = decoded === null ? "" : JSON.stringify(decoded, null, 2);
}

function showBytes(title, sub, start, end, marks) {
  $("insp-title").textContent = title;
  $("insp-sub").textContent = sub;
  const stop = Math.min(end, start + MAX_HEX);
  const classAt = (i) => marks.find(([a, b]) => i >= a && i < b)?.[2];
  const lines = [];
  for (let line = start - (start % 16); line < stop; line += 16) {
    const row = [el("span", { class: "off" }, line.toString(16).padStart(8, "0") + "  ")];
    let ascii = "";
    for (let i = line; i < line + 16; i++) {
      const inside = i >= start && i < stop;
      const text = inside ? bytes[i].toString(16).padStart(2, "0") : "  ";
      const cls = inside ? classAt(i) : undefined;
      row.push(cls ? el("span", { class: cls }, text) : text, i % 16 === 7 ? "  " : " ");
      ascii += inside ? (bytes[i] >= 32 && bytes[i] < 127 ? String.fromCharCode(bytes[i]) : ".") : " ";
    }
    row.push(" ", ascii, "\n");
    lines.push(...row);
  }
  if (stop < end) lines.push(`… ${(end - stop).toLocaleString()} more bytes\n`);
  $("hex").replaceChildren(...lines);
}

async function readFile(file) {
  if (file.size > 64 * 1024 * 1024) return fail("That file is over 64 MB; a .DS_Store is usually a few kilobytes.");
  open(new Uint8Array(await file.arrayBuffer()), file.name);
}

const drop = $("drop");
$("choose").addEventListener("click", () => $("file").click());
$("file").addEventListener("change", (e) => e.target.files[0] && readFile(e.target.files[0]));
$("sample").addEventListener("click", async () => {
  const response = await fetch("sample.DS_Store");
  open(new Uint8Array(await response.arrayBuffer()), "sample.DS_Store");
});
drop.addEventListener("keydown", (e) => (e.key === "Enter" || e.key === " ") && e.target === drop && $("file").click());
["dragenter", "dragover"].forEach((t) => drop.addEventListener(t, (e) => (e.preventDefault(), drop.classList.add("over"))));
["dragleave", "drop"].forEach((t) => drop.addEventListener(t, () => drop.classList.remove("over")));
drop.addEventListener("drop", (e) => {
  e.preventDefault();
  const file = e.dataTransfer.files[0];
  if (file) readFile(file);
});
