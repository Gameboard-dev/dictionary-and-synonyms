# Dictionary and Synonyms

An Obsidian plugin: select a word (or a phrase of up to four words) in the editor and its synonyms drop down beneath it; click one to put it in place of the selection, in the selection's case. Select it with Ctrl held (Cmd on macOS) to see its definitions instead: by part of speech, with Wiktionary's labels ("informal", "US"), and *Open note*. The list is used with the mouse (hover and click); any other click, key or scroll closes it. There is no pane.

- **Synonyms on select**, as a vertical list under the word; a click puts the synonym in place in the word's case.
- **Definitions on Ctrl+select** (Cmd+select on macOS), in a popover beside the word.
- **Every lookup kept as a note** of properties, read before the network is asked again.
- **Whole dictionaries for use offline**, downloaded only when you press Download, with the size shown first, a progress bar, Cancel and resume.
- **Sources with stated licences**: Datamuse, Wiktionary, Wikipedia, Open English WordNet and kaikki.org, each named with its licence in every note.

## Disclosures (Obsidian's developer policies)

Free, open source (MIT), no account, no ads and no telemetry. The plugin sends the word or phrase you select, and nothing else from your notes, to Datamuse, Wiktionary and Wikipedia, and only when that word has no note yet. It downloads a whole dictionary (from en-word.net or kaikki.org) only when you press Download, and keeps it in your computer's cache folder, outside the vault. It never downloads, installs or updates code. Desktop only, because the downloads use Node's modules. Each service, what it receives and why, its privacy policy, and how the plugin meets Obsidian's [Developer policies](https://docs.obsidian.md/community-directory/developer-policies): [Disclosures and compliance](docs/disclosures.md).

## Where the words come from

| What | Source | Licence |
|---|---|---|
| Synonyms | The Datamuse API (`/words?ml=…`, the entries tagged `syn`; <https://www.datamuse.com/api/>), free and without a key; English and Spanish | Datamuse's terms; its synonyms draw on WordNet 3.0 (Princeton's WordNet licence) |
| Definitions | Wiktionary through the MediaWiki Action API (`api.php`, <https://www.mediawiki.org/wiki/API:Revisions>), the API every Wikimedia site runs on | CC BY-SA 4.0 |
| Definitions Wiktionary lacks | Wikipedia's summary (TextExtracts): the exact title, else a search whose result must contain the selection's words; then Datamuse's definitions (`md=d`) | CC BY-SA 4.0 |
| Offline (optional) | A dictionary you have downloaded: Open English WordNet, or one language's entries from Wiktionary (Downloads, below) | WordNet CC BY 4.0; Wiktionary CC BY-SA 4.0 |

Definitions come from Wiktionary first, then from a Wikipedia summary for a term no dictionary has; when Wikimedia is busy, Datamuse's short definitions stand in. With a dictionary downloaded, a word's definitions are looked up there before the network.

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

Since 0.1.4 a release is built by GitHub Actions (`.github/workflows/release.yml`) when its tag (`x.y.z`, the manifest's version) is pushed: from the tagged source, in obsidian-cargo-lib at the commit pinned in the workflow, with the tests run first; the three files carry an artifact attestation of their build provenance. Check a downloaded file with `gh attestation verify main.js --repo Gameboard-dev/dictionary-and-synonyms`.

Dependencies: wasm-bindgen, wasm-bindgen-futures, js-sys, serde, serde_json, serde-wasm-bindgen, backon (retries), quick-xml (WordNet's XML).

## Known limits

- Synonyms only for the languages Datamuse has (English, Spanish); other languages get definitions only.
- Desktop only (`isDesktopOnly`), because the downloads use Node's modules. Where those modules are missing or are stand-ins (Obsidian run in a browser), nothing is downloaded and words are looked up over the network.
- Datamuse says on its page (read 2026-10-29) that from early 2027 every request will need an API key (the page gives both 1 January and 1 February 2027). Until the plugin has a setting for one, synonyms for a word that has no note yet may stop arriving then; definitions do not depend on Datamuse.

## Acknowledgements

Inspired by [Dictionary](https://github.com/phibr0/obsidian-dictionary) by phibr0. Dictionary and Synonyms is a separate program, written from scratch in Rust, and shares no code with it.

## Licence

MIT (`LICENSE`). Third-party licences, and the licences of the dictionaries the plugin reads from (Wiktionary and Wikipedia CC BY-SA 4.0, Open English WordNet CC BY 4.0, Datamuse's terms and WordNet 3.0's licence): `THIRD-PARTY-LICENSES.txt`. This plugin uses the [Datamuse API](https://www.datamuse.com/api/).
