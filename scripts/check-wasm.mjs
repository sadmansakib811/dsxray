// Load the browser build the way web/app.js does and check it reads the sample.
//   node scripts/check-wasm.mjs site/dsxray.wasm site/sample.DS_Store
import { readFileSync } from "node:fs";

const [wasmPath, samplePath] = process.argv.slice(2);
const module = new WebAssembly.Module(readFileSync(wasmPath));
if (WebAssembly.Module.imports(module).length) throw new Error("the module must not import anything");
const x = new WebAssembly.Instance(module, {}).exports;

const file = readFileSync(samplePath);
const ptr = x.dsx_alloc(file.length);
new Uint8Array(x.memory.buffer, ptr, file.length).set(file);
const packed = x.dsx_inspect(ptr, file.length);
const [outPtr, outLen] = [Number(packed >> 32n), Number(packed & 0xffffffffn)];
const answer = JSON.parse(new TextDecoder().decode(new Uint8Array(x.memory.buffer, outPtr, outLen)));
x.dsx_free(outPtr, outLen);
x.dsx_free(ptr, file.length);

if (!answer.ok || answer.store.records.length !== 16 || !answer.store.names.includes("holiday.jpg")) {
  throw new Error(`unexpected answer: ${JSON.stringify(answer).slice(0, 300)}`);
}
console.log(`ok: ${answer.store.records.length} records, ${answer.store.names.length} names, ${readFileSync(wasmPath).length} byte module`);
