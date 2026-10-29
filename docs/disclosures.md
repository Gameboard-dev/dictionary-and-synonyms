# Disclosures and compliance

[← README](../README.md)

Dictionary and Synonyms is written to meet Obsidian's
[Developer policies](https://docs.obsidian.md/community-directory/developer-policies), the
[Submission requirements for plugins](https://docs.obsidian.md/community-directory/submission-requirements-for-plugins)
and the [Plugin guidelines](https://docs.obsidian.md/Plugins/Releasing/Plugin+guidelines). This page goes through what
those policies ask a plugin to disclose, and what they forbid, and says plainly what the plugin does about each and why.
The links below were checked on 2026-10-29.

- [In short](#in-short)
- [Payment and accounts](#payment-and-accounts)
- [Network use](#network-use)
- [What the services do with a request](#what-the-services-do-with-a-request)
- [Telemetry](#telemetry)
- [Files outside the vault](#files-outside-the-vault)
- [Files in the vault](#files-in-the-vault)
- [Ads](#ads)
- [Source code and licences](#source-code-and-licences)
- [Desktop only](#desktop-only)
- [No self-installation or self-updating](#no-self-installation-or-self-updating)
- [The plugin guidelines](#the-plugin-guidelines)

## In short

Dictionary and Synonyms is free and open source, needs no account, shows no ads and collects nothing about you. When
you select a word that has no note yet, it sends that word, and nothing else from your notes, to Datamuse, Wiktionary
and Wikipedia, because their dictionaries are far too large to ship with a plugin. It downloads a whole dictionary only
when you press Download, and keeps it in your computer's cache folder. It never downloads, installs or updates code.

## Payment and accounts

None. The plugin and the services it uses are free, every feature works without paying anyone, and there is no
sign-in and no API key.

## Network use

Each host the plugin contacts, when, who starts it, what is sent, and why:

| Host | When | Started by | What is sent | Why |
|---|---|---|---|---|
| Datamuse (`api.datamuse.com`) | You select a word whose note has no synonyms yet, in English or Spanish; or Wikimedia cannot answer a definition | Your selection | The selected word or phrase, and the vocabulary (English or Spanish) | Its synonyms, and short definitions when Wikimedia cannot answer |
| Wiktionary (`en.wiktionary.org`) | You select a word with Ctrl or Cmd held and it has no definitions yet (in neither its note nor a downloaded dictionary); for synonyms in a language Datamuse does not cover | Your selection | The selected word or phrase (then in lower case, and a form's lemma, such as "run" for "running") | Its definitions, and synonyms for those languages |
| Wikipedia (`<language>.wikipedia.org`) | No dictionary has the word | Your selection | The selected word or phrase, as a title, then as a search | A short encyclopedia summary for a term no dictionary has |
| en-word.net | The settings tab is open (a HEAD request); you press Download | You | An ordinary file request, no word | The size of Open English WordNet (11 MB), then the file itself |
| kaikki.org | The settings tab is open (a HEAD request); you press Download | You | An ordinary file request, no word | The size of the Wiktionary extract (3.1 GB), then the file itself |

The HEAD request asks only for a download's size; it transfers no data. A word whose note already holds what you ask
for needs no network; with a dictionary downloaded, definitions are read from it, while English and Spanish synonyms
still come from Datamuse. The plugin makes no request when Obsidian starts and none in the background.

The requests to Wikimedia and the downloads carry a User-Agent with the plugin's name, its version and the *Contact for
Wikimedia* setting (by default this repository's address, not anything about you), as Wikimedia's
[User-Agent policy](https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy) asks.

**Datamuse and API keys.** Datamuse's [page](https://www.datamuse.com/api/#limits) (read 2026-10-29) says that from
early 2027 every request will need an API key (it gives both 1 January and 1 February 2027). Until the plugin has a
setting for one, synonyms for a word without a note may stop arriving then; definitions do not depend on Datamuse.

## What the services do with a request

The plugin sends no data of its own to any server, but each service receives the request itself, with your IP address,
and handles it under its own terms:

- **Datamuse:** keeps a temporary log of the queries, discarded at the end of the day after the request, and saves no
  long-term usage data beyond counts of requests by broad category ([Privacy](https://www.datamuse.com/api/#privacy),
  on the API page; Datamuse publishes no separate privacy policy).
- **Wikimedia Foundation** (Wiktionary, Wikipedia): its [Privacy Policy](https://foundation.wikimedia.org/wiki/Policy:Privacy_policy)
  covers its sites and APIs, treats IP addresses and user-agent information as personal information, and in most
  cases deletes, aggregates or de-identifies personal information after 90 days. Use of its sites is under its
  [Terms of Use](https://foundation.wikimedia.org/wiki/Policy:Terms_of_Use) and
  [API usage guidelines](https://foundation.wikimedia.org/wiki/Policy:API_usage_guidelines).
- **en-word.net** (Open English WordNet, run by John P. McCrae at the Data Science Institute, University of Galway) and
  **kaikki.org** (maintained by Tatu Ylonen): neither publishes a privacy policy. They receive only an ordinary file
  request when you press Download (and the HEAD request for the size), never a word or anything from your vault.

## Telemetry

None, client-side or server-side. Nothing about you or your use is collected or sent anywhere.

## Files outside the vault

A downloaded dictionary is kept in your computer's cache folder, because a dictionary of hundreds of megabytes does not
belong in a vault that may be synced: Windows `%LOCALAPPDATA%\dictionary-and-synonyms`, macOS
`~/Library/Caches/dictionary-and-synonyms`, Linux `$XDG_CACHE_HOME/dictionary-and-synonyms` (by default
`~/.cache/dictionary-and-synonyms`). The download itself is in the same folder (a `.part` file while it is incomplete, so that it can be
resumed, then a `.gz` file) and is deleted once it has been processed. Delete in the settings removes a dictionary,
or delete the folder yourself. Nothing else is read or written outside the vault.

## Files in the vault

One note per word looked up, in the folder set in the settings (by default `Dictionary/<language>/`), and the plugin's
settings (`data.json` in its plugin folder). At start, notes in that folder of the plugin's earlier format (a
`dict-entry` property) are rewritten in the current one.

## Ads

None, static or dynamic.

## Source code and licences

Open source, under the MIT licence ([LICENSE](../LICENSE)), built from the Rust source in this repository and the shared
crate and build tool in [obsidian-cargo-lib](https://github.com/Gameboard-dev/obsidian-cargo-lib)
([Layout and build](../README.md#layout-and-build)). The code is not obfuscated: `main.js` is a short loader, and the
plugin's code is a WebAssembly module compiled from that source (inlined in `main.js` in a release), with readable
JavaScript bindings generated by wasm-bindgen.

No dictionary data is shipped with the plugin. The words keep their own licences, and each note's `sources` property
names where its text came from and under which licence:

- Wiktionary and Wikipedia: [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/);
- Open English WordNet: [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/);
- Datamuse: its terms on the [API page](https://www.datamuse.com/api/); its synonyms draw on WordNet 3.0, under
  Princeton's [WordNet licence](https://wordnet.princeton.edu/license-and-commercial-use);
- kaikki.org's extract: Wiktionary's text, CC BY-SA 4.0.

[THIRD-PARTY-LICENSES.txt](../THIRD-PARTY-LICENSES.txt) lists the Rust crates compiled into the plugin with their
licences (generated by `licences/generate.py` with cargo-about), and the sources of the words with the attribution they
ask for.

## Desktop only

`isDesktopOnly` is `true`: the downloads use Node's `https`, `zlib`, `fs`, `os` and `path`, which exist only in desktop
Obsidian and which the submission requirements allow only with `isDesktopOnly`. The plugin checks for them before use;
where they are missing, nothing is downloaded and words are looked up over the network.

## No self-installation or self-updating

The plugin never installs or updates itself or any code. A dictionary is downloaded only when you press its Download
button, and it is data, not code.

## The plugin guidelines

| Guideline | How Dictionary and Synonyms meets it |
|---|---|
| No obfuscation | `main.js` is a short loader; the plugin's code is a WebAssembly module (inlined in `main.js` in a release) compiled from the Rust source here, with its JavaScript bindings generated by wasm-bindgen and kept readable. Build instructions are in the README. |
| No client-side telemetry, no ads | None. |
| Disclosures | Network use and files outside the vault, above. |
| Licence and attribution | `LICENSE` (MIT); `THIRD-PARTY-LICENSES.txt`. Each note's `sources` property names where its text came from and under which licence. |
| Desktop only | `isDesktopOnly` is `true` (above). |
| Commands and hotkeys | The plugin adds no commands and sets no hotkeys. |
| Security | No `innerHTML`, `outerHTML` or `insertAdjacentHTML`; text is set with `textContent` and the DOM API. No `eval`: in a release the bindings are part of `main.js`. The Web Worker that sorts a download runs the same bundled bindings from a Blob URL, never code from elsewhere. |
| Settings | Headings with `setHeading`, in sentence case, without the word "settings". |
| Vault | Notes are read and written with the Vault API (`cachedRead`, `process`, `create`, `createFolder`); the folder setting goes through `normalizePath`. |
| Resources | Events are registered with `registerDomEvent`; on unload the plugin closes its popover and stops a running download and its worker. |
| `this.app` | The plugin uses the plugin's own `app`, never the global one. |
