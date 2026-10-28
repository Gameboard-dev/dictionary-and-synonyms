// Dictionary and Synonyms. The plugin is Rust compiled to dictionary_and_synonyms.wasm (core/); this file only hands it
// Obsidian. The .wasm carries its own JavaScript bindings in a custom section, "glue" (../pack), so this is the only
// JavaScript file. Source and licences (MIT; third-party: THIRD-PARTY-LICENSES.txt):
// https://github.com/Gameboard-dev/dictionary-and-synonyms
const obsidian = require("obsidian");
// A release has the module here, base64-encoded (pack: Obsidian installs only main.js, manifest.json and styles.css).
const WASM = null;

module.exports = class DictionaryAndSynonyms extends obsidian.Plugin {
  async onload() {
    const bytes = WASM
      ? Uint8Array.from(atob(WASM), (c) => c.charCodeAt(0))
      : await this.app.vault.adapter.readBinary(`${this.manifest.dir}/dictionary_and_synonyms.wasm`);
    const module = await WebAssembly.compile(bytes);
    const glue = new TextDecoder().decode(WebAssembly.Module.customSections(module, "glue")[0]);
    const rust = new Function(`${glue}\nreturn wasm_bindgen;`)();
    rust.initSync({ module });
    this.rust = rust.start(this, obsidian, typeof require === "function" ? require : null, module, glue);
  }

  onunload() {
    this.rust?.unload();
  }
};
