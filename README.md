# Dictionary and Synonyms

An Obsidian plugin: select a word (or a phrase of up to four words) in the editor and its synonyms drop down beneath it; click one to put it in place of the selection, in the selection's case. Select it with Ctrl held (Cmd on macOS) to see its definitions instead: by part of speech, with Wiktionary's labels ("informal", "US"), and *Open note*. The list is used with the mouse (hover and click); any other click, key or scroll closes it. There is no pane.

## Credit, and what differs from the plugin it replaces

Dictionary and Synonyms was made to replace [Dictionary](https://github.com/phibr0/obsidian-dictionary) by phibr0 (plugin id `obsidian-dictionary-plugin`, licensed AGPL-3.0), whose ideas it follows: synonyms offered for a selected word, and looked-up words kept as notes. It is not a copy of that plugin's code. It is a new program, written in Rust, and contains none of Dictionary's code, text or styles, so it has its own licence (MIT). What it does differently, and why:

- **Definitions that arrive.** Dictionary's English definitions come from the Free Dictionary API (`api.dictionaryapi.dev`). For the author of this plugin they failed with Dictionary's "Something went wrong.." message, and other users report the same failure ([issue #95](https://github.com/phibr0/obsidian-dictionary/issues/95): "request failed, status 404"). This plugin takes definitions from Wiktionary, then a Wikipedia summary for terms no dictionary has, with Datamuse's short definitions when Wikimedia is busy, or from a dictionary you have downloaded.
- **No lookup pane.** Dictionary shows definitions in a side pane (its *Look up* menu item). Here, selecting a word with Ctrl or Cmd held shows its definitions in a popover beside it. Selecting a word without a modifier shows its synonyms, as in Dictionary, but as a vertical list under the word, used with the mouse; a click puts the synonym in place in the word's case.
- **Every lookup kept as a note.** Each word looked up is saved as a note of properties, and the note is read before the network is asked again. Dictionary saves a note only when you ask it to.
- **Offline dictionaries downloaded only on request.** Open English WordNet, or one language's entries from Wiktionary, downloaded when you press Download, with its size shown first, a progress bar, Cancel and resume. Dictionary downloads its offline dictionary at the first lookup that needs it.
- **Sources with stated licences.** Datamuse, Wiktionary, Wikipedia, Open English WordNet and kaikki.org, each named with its licence in every note.

## Where the words come from

| What | Source | Licence |
|---|---|---|
| Synonyms | The Datamuse API (`/words?ml=…`, the entries tagged `syn`; <https://www.datamuse.com/api/>), free and without a key; English and Spanish | Datamuse's terms; its synonyms draw on WordNet 3.0 (Princeton's WordNet licence) |
| Definitions | Wiktionary through the MediaWiki Action API (`api.php`, <https://www.mediawiki.org/wiki/API:Revisions>), the API every Wikimedia site runs on | CC BY-SA 4.0 |
| Definitions Wiktionary lacks | Wikipedia's summary (TextExtracts): the exact title, else a search whose result must contain the selection's words; then Datamuse's definitions (`md=d`) | CC BY-SA 4.0 |

Wikimedia limits clients that do not identify themselves to about 10 requests a minute; every request therefore carries a User-Agent naming the plugin, its version and a contact (*Contact for Wikimedia* in the settings, by default this plugin's page), as Wikimedia's policy asks (<https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy>). A 429 is waited out by its Retry-After, and no other request is sent meanwhile. Measured (2026-10-24): 15 lookups in a row, all answered.

English Wiktionary describes every language's words, each in its own section, so a language is a row in `core/src/dict.rs` (`LANGUAGES`: its name, its Wiktionary section, its Datamuse vocabulary if Datamuse has one, and for English the labels that mark British and American senses; the selected variety's senses come first). Translation is not in scope.

## Lazy, and kept as notes

A word is looked up only when it is first selected: in this session's memory, then its note, then the downloaded dictionary if there is one, and only then the network. What comes back is written to the note `Dictionary/<language>/<word>.md` (the folder is a setting) as **properties only**, each shown by Obsidian on its own: `word`, `language`, `lemma`, `form-of`, the definitions by part of speech (`definitions-noun`, `definitions-verb` …, a definition's labels in brackets before it), the synonyms by part of speech (`synonyms-noun` …; a synonym given for two parts of speech is in both), the encyclopedia's `summary` (with `summary-title` and `summary-url`), `looked-up` (definitions, synonyms), `dictionary`, `sources` and `retrieved`. The note has no body, so nothing is repeated, and it is read back from these properties (edit them, and the popover shows your edit). A form ("running") also brings its lemma's senses ("run"). Delete a note to look the word up again. A single letter, or a phrase that starts or ends with one ("because i"), is not looked up: such a selection is a slip. Notes of the older form (the entry as one JSON property, `dict-entry`, then the same again as Markdown) are rewritten once when the plugin starts.

## Downloads (optional, desktop)

Settings → *Download a whole dictionary*, for lookups without the network. Nothing is downloaded until you press Download. Each row says the download's size before you start (asked of the server with a HEAD request, which transfers no data, when the settings tab is open), shows a progress bar while it downloads and then while it is processed, and has Download, Cancel, Redownload and Delete.

- **Open English WordNet 2025** (<https://en-word.net/>, CC BY 4.0): 11 MB to download, 44 MB kept; definitions and synonyms of about 150,000 English words and phrases, with no British or American labels.
- **Wiktionary, the selected language** (kaikki.org's extract, made with Wiktextract, <https://kaikki.org/dictionary/rawdata.html>; Wiktionary's text, CC BY-SA 4.0): 3.1 GB for every language; only the selected language's entries are kept (a few hundred MB), and the download is then deleted.

The plugin downloads with Node's `https` (Obsidian's desktop app), resuming with an HTTP Range request: Cancel keeps what has arrived, Download continues from there, and a dropped connection is retried with exponential backoff and jitter (backon, <https://docs.rs/backon>). The file is decompressed as it is read (`zlib`) and its pieces go to a Web Worker started from the same module, which keeps the language's entries and sorts them; the main thread only moves bytes. The result is one sorted, tab-separated file in the computer's cache folder, outside the vault (Windows `%LOCALAPPDATA%\dictionary-and-synonyms`, macOS `~/Library/Caches/dictionary-and-synonyms`, Linux `~/.cache/dictionary-and-synonyms`), and a lookup reads it by binary search without loading it. Measured (2026-10-25): WordNet downloaded and processed in 7 s.

## Settings

Synonyms on select; definitions on Ctrl+select; language (English (British), English (American), Spanish, French, German, Italian, Portuguese, Dutch); notes folder; Wikimedia contact; the two downloads.

## Layout and build

This repository is one plugin of a Rust workspace: clone [obsidian-cargo-lib](https://github.com/Gameboard-dev/obsidian-cargo-lib) (the shared crate and the build tool, pack), then clone this repository into it as `dictionary-and-synonyms`, and build from the obsidian-cargo-lib folder. The release assets are the three files Obsidian installs, `main.js` (with the WebAssembly module inlined, base64-encoded), `manifest.json` and `styles.css`; to install by hand, copy them into `<vault>/.obsidian/plugins/dictionary-and-synonyms/`. `pack -- dictionary-and-synonyms --release <folder>` writes them.

```bash
git clone https://github.com/Gameboard-dev/obsidian-cargo-lib
git clone https://github.com/Gameboard-dev/dictionary-and-synonyms obsidian-cargo-lib/dictionary-and-synonyms
```

Everything is Rust except `main.js` (the loader, which hands Obsidian and Node's `require` to `dictionary_and_synonyms.wasm`; the wasm-bindgen bindings are in the .wasm's custom section `glue`, and pack writes them into the main.js it deploys or releases, so a release evaluates no code from a string).

- `core/src/app.rs`: settings, the settings tab, the editor events, the worker. `lookup.rs`: the lookup order, the network sources and the dropdown and popover. `dict.rs`: languages, Wiktionary's wikitext, the notes. `files.rs`: downloads and the local file. `worker.rs`: filtering and sorting a download.
- The shared crate `obsidian-plugins/shared` (obsidian-kit): the JavaScript helpers and the settings-tab builder.

```bash
cd obsidian-plugins
cargo run -p pack --release -- dictionary-and-synonyms --deploy <vault>/.obsidian/plugins/dictionary-and-synonyms
cargo test --release -p dictionary-and-synonyms-core
```

Dependencies: wasm-bindgen, wasm-bindgen-futures, js-sys, serde, serde_json, serde-wasm-bindgen, backon (retries), quick-xml (WordNet's XML).

## Known limits

- Synonyms only for the languages Datamuse has (English, Spanish); other languages get definitions only.
- Desktop only (`isDesktopOnly`), because the downloads use Node's modules. Where those modules are missing or are stand-ins (Obsidian run in a browser), nothing is downloaded and words are looked up over the network.
- Datamuse says on its page (read 2026-10-28) that from early 2027 every request will need an API key. Until the plugin has a setting for one, synonyms for a word that has no note yet may stop arriving then; definitions do not depend on Datamuse.

## Disclosures and compliance

Dictionary and Synonyms is written to meet Obsidian's [Developer policies](https://docs.obsidian.md/Community+directory/Developer+policies), the [Submission requirements for plugins](https://docs.obsidian.md/community-directory/submission-requirements-for-plugins) and the [Plugin guidelines](https://docs.obsidian.md/Plugins/Releasing/Plugin+guidelines). The disclosures the policies ask for, in their order:

- **Payment:** none. The plugin and its sources are free.
- **Account:** none. No sign-in and no API key.
- **Network use:** the words come from online dictionaries, too large to ship with the plugin.
  - *When you select a word* (and it has no note yet), the plugin sends that word or phrase, and nothing else from your notes, to:
    - the Datamuse API (`api.datamuse.com`), for its synonyms (and short definitions when Wikimedia is busy);
    - Wiktionary (`en.wiktionary.org`), for its definitions;
    - Wikipedia (`<language>.wikipedia.org`), for an encyclopedia summary when no dictionary has the term.
  - Wikimedia's requests carry a User-Agent with the plugin's name, version and the *Contact for Wikimedia* setting (by default this repository's address, not anything about you), as Wikimedia's policy asks.
  - *When the settings tab is open*, a HEAD request to `en-word.net` and `kaikki.org` asks each download's size; no data is downloaded.
  - *Only when you press Download* in the settings, the plugin downloads Open English WordNet from `en-word.net` (11 MB) or kaikki.org's Wiktionary extract from `kaikki.org` (3.1 GB). It never downloads, installs or updates code.
  - Once a word has a note, or a whole dictionary is downloaded, that word needs no network.
- **Files outside the vault:** a downloaded dictionary is kept in your computer's cache folder, because a dictionary of hundreds of megabytes does not belong in a vault that may be synced: Windows `%LOCALAPPDATA%\dictionary-and-synonyms`, macOS `~/Library/Caches/dictionary-and-synonyms`, Linux `$XDG_CACHE_HOME/dictionary-and-synonyms` (by default `~/.cache/dictionary-and-synonyms`). Delete removes it, or delete the folder yourself. Nothing else is read or written outside the vault.
- **Files in the vault:** one note per word looked up, in the folder set in the settings (by default `Dictionary/<language>/`), and the plugin's settings (`data.json` in its plugin folder). At start, notes in that folder of the plugin's earlier format (a `dict-entry` property) are rewritten in the current one.
- **Ads:** none.
- **Telemetry:** none. Nothing about you or your use is collected or sent anywhere.
- **Source code:** open source, under the MIT licence (`LICENSE`), built from the Rust source in this repository and the shared crate and build tool in [obsidian-cargo-lib](https://github.com/Gameboard-dev/obsidian-cargo-lib) (Layout and build, above).

| Rule | How Dictionary and Synonyms meets it |
|---|---|
| No self-install or self-update | The plugin never installs or updates itself or any code. Dictionary data is downloaded only when you press Download, and is data, not code. |
| No obfuscation | `main.js` is a short loader; the plugin's code is a WebAssembly module (inlined in `main.js` in a release) compiled from the Rust source here, with its JavaScript bindings generated by wasm-bindgen and kept readable. Build instructions are above. |
| No client-side telemetry, no ads | None. |
| Disclosures | Network use and files outside the vault, above. |
| Licence and attribution | `LICENSE` (MIT). `THIRD-PARTY-LICENSES.txt`: the Rust crates compiled into the plugin, with their licences (generated by `licences/generate.py` with cargo-about), and the sources of the words with their licences and the attribution they ask for. No dictionary data is shipped. Each note's `sources` property names where its text came from and under which licence. |
| Desktop only | `isDesktopOnly` is `true`: the downloads use Node's `https`, `zlib`, `fs`, `os` and `path`, which the submission requirements allow only with `isDesktopOnly`. The plugin checks for them before use. |
| Commands and hotkeys | The plugin adds no commands and sets no hotkeys. |
| Security | No `innerHTML`, `outerHTML` or `insertAdjacentHTML`; text is set with `textContent` and the DOM API. No `eval`: in a release the bindings are part of `main.js`. The Web Worker that sorts a download runs the same bundled bindings from a Blob URL, never code from elsewhere. |
| Settings | One heading (`setHeading`) for the downloads section, in sentence case, without the word "settings". |
| Vault | Notes are read and written with the Vault API (`cachedRead`, `process`, `create`, `createFolder`); the folder setting goes through `normalizePath`. |
| Resources | Events are registered with `registerDomEvent`; on unload the plugin closes its popover and stops a running download and its worker. |
| `this.app` | The plugin uses the plugin's own `app`, never the global one. |

## Licence

MIT (`LICENSE`). Third-party licences, and the licences of the dictionaries the plugin reads from (Wiktionary and Wikipedia CC BY-SA 4.0, Open English WordNet CC BY 4.0, Datamuse's terms and WordNet 3.0's licence): `THIRD-PARTY-LICENSES.txt`. This plugin uses the [Datamuse API](https://www.datamuse.com/api/).
