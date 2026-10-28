// Dictionary and Synonyms. The plugin is Rust compiled to dictionary_and_synonyms.wasm (core/); this file only hands it
// Obsidian. The .wasm carries its own JavaScript bindings in a custom section, "glue" (../pack), so this is the only
// JavaScript file.
const obsidian = require("obsidian");

module.exports = class DictionaryAndSynonyms extends obsidian.Plugin {
  async onload() {
    const bytes = await this.app.vault.adapter.readBinary(`${this.manifest.dir}/dictionary_and_synonyms.wasm`);
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
