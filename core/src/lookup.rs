//! Synonyms when a word is selected, in a dropdown under it, one per row (hover to highlight, click to put it in place
//! of the selection; mouse only); definitions when it is selected with Ctrl (Cmd on macOS) held. Each word is looked up once and kept: in this session's memory and in its note in the dictionary folder.
//!
//! Sources: synonyms from Datamuse (<https://www.datamuse.com/api/>, `ml` results tagged `syn`), for languages it
//! covers, else Wiktionary's; definitions from the language's downloaded dictionary (files.rs, desktop only, if there is
//! one), else Wiktionary (the MediaWiki Action API, <https://www.mediawiki.org/wiki/API:Revisions>), and for a term
//! no dictionary has, Wikipedia's opening sentences (the TextExtracts API,
//! <https://www.mediawiki.org/wiki/Extension:TextExtracts#API>). Wikimedia allows 10 requests a minute to a client
//! that does not identify itself and 200 to one whose User-Agent gives a contact
//! (<https://www.mediawiki.org/wiki/Wikimedia_APIs/Rate_limits>, <https://foundation.wikimedia.org/wiki/Policy:User-Agent_policy>);
//! when it asks to wait (HTTP 429 with Retry-After), nothing more is sent until then, and Datamuse's short definitions
//! stand in.
//!
//! APIs: Obsidian's active editor (<https://docs.obsidian.md/Reference/TypeScript+API/Workspace/activeEditor>, the
//! `Editor` methods <https://docs.obsidian.md/Reference/TypeScript+API/Editor>), CodeMirror's `coordsAtPos`
//! (<https://codemirror.net/docs/ref/#view.EditorView.coordsAtPos>), `requestUrl`
//! (<https://docs.obsidian.md/Reference/TypeScript+API/requestUrl>), and the vault (`create`, `adapter.read`,
//! `adapter.write`, `adapter.mkdir`).

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::app::with;
use crate::dict::{self, Entry, Sense, Summary};
use crate::js::{self, call, f1, get, new, obj, set, str};

const MAX_WORDS: usize = 4;
const SHOWN_SYNONYMS: usize = 30;
const SHOWN_SENSES: usize = 8; // per part of speech; the note has them all
const WAIT_DEFAULT_S: f64 = 30.0; // when a 429 gives no Retry-After

/// The synonyms dropdown: its rows (element, word), the highlighted row, the editor and the text it replaces.
struct Menu {
    rows: Vec<(JsValue, String)>,
    at: usize,
    editor: JsValue,
    selected: String,
}

#[derive(Default)]
struct State {
    /// Entries looked up this session, by language and word (found or not: an entry with nothing in it).
    cache: HashMap<String, Entry>,
    popover: Option<JsValue>,
    menu: Option<Menu>,
    shown: u32, // the latest lookup; an older one finishing later shows nothing
    /// Wikimedia asked to wait until then (`performance.now()`, ms).
    wait_until: f64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Want {
    Synonyms,
    Definitions,
}

/// The selected word or short phrase, without the punctuation around it; `None` for anything longer or without letters.
fn word(selected: &str) -> Option<String> {
    let w = selected.trim().trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-').trim_matches(['\'', '-']);
    let ok = !w.is_empty() && !w.contains('\n') && w.chars().count() <= 60 && w.chars().any(char::is_alphabetic) && w.split_whitespace().count() <= MAX_WORDS;
    // a single letter ("b" of "Figure 3b"), or a phrase starting or ending with one ("because i"), is a slip of the
    // selection, not a word to look up (user, 2026-10-28)
    let single = |p: Option<&str>| p.is_some_and(|x| x.chars().count() == 1);
    let ok = ok && !single(w.split_whitespace().next()) && !single(w.split_whitespace().last());
    ok.then(|| w.to_string())
}

/// The form a word is kept under: lower case, unless it has capitals after its first letter (an abbreviation, a name
/// like "NetFlow"), which keep them.
fn key(w: &str) -> String {
    if w.chars().skip(1).any(char::is_uppercase) { w.to_string() } else { w.to_lowercase() }
}

/// After a mouse selection ends in the active editor: definitions with Ctrl or Cmd held, else synonyms (each only when
/// its setting is on). Called for every mouseup; does nothing elsewhere.
pub fn mouseup(e: &JsValue) {
    let ctrl = get(e, "ctrlKey").is_truthy() || get(e, "metaKey").is_truthy();
    let target = get(e, "target");
    js::timeout(10.0, move || selection_ended(Some(&target), ctrl));
}

/// After the keyboard extends a selection (Shift with arrows, Ctrl+A …): synonyms.
pub fn keyup(e: &JsValue) {
    if get(e, "shiftKey").is_truthy() || str(&get(e, "key")) == "Shift" {
        js::timeout(300.0, || selection_ended(None, false));
    }
}

/// Any key but a modifier closes the popover (the dropdown is used with the mouse only).
pub fn keydown(e: &JsValue) {
    if !["Shift", "Control", "Meta", "Alt"].contains(&str(&get(e, "key")).as_str()) {
        hide();
    }
}

/// Any click outside the popover closes it.
pub fn dismiss(e: &JsValue) {
    let inside = state(|s| s.popover.clone()).is_some_and(|p| call!(p, contains, get(e, "target")).is_truthy());
    if !inside {
        hide();
    }
}

pub fn hide() {
    let p = state(|s| {
        s.menu = None;
        s.popover.take()
    });
    if let Some(p) = p {
        call!(p, remove);
    }
}

fn selection_ended(target: Option<&JsValue>, ctrl: bool) {
    let Some(Some((editor, lang, want))) = with(|a| {
        let s = &a.settings;
        let want = if ctrl { (s.definitions_on_ctrl, Want::Definitions) } else { (s.synonyms_on_select, Want::Synonyms) };
        let editor = get(&get(&get(&get(&a.plugin, "app"), "workspace"), "activeEditor"), "editor");
        let inside = target.is_none_or(|t| call!(get(&get(&editor, "cm"), "dom"), contains, t.clone()).is_truthy());
        (want.0 && editor.is_object() && inside).then(|| (editor, s.dict_lang.clone(), want.1))
    }) else { return };
    if !call!(editor, somethingSelected).is_truthy() {
        return;
    }
    let selected = str(&call!(editor, getSelection));
    let Some(w) = word(&selected) else { return };
    let from = call!(editor, posToOffset, call!(editor, getCursor, "from"));
    let coords = call!(get(&editor, "cm"), coordsAtPos, from);
    if !coords.is_object() {
        return;
    }
    let id = state(|s| {
        s.shown += 1;
        s.shown
    });
    if want == Want::Definitions {
        show(&coords, &message(&format!("Looking up \u{201c}{w}\u{201d}\u{2026}")));
    }
    js::spawn(async move {
        let found = match want {
            Want::Synonyms => synonyms(&w, &lang).await.map(Err),
            Want::Definitions => definitions(&w, &lang).await.map(Ok),
        };
        if state(|s| s.shown) != id || call!(editor, getSelection).as_string().and_then(|s| word(&s)).is_none_or(|s| s != w) {
            return; // another selection since
        }
        match found {
            Ok(Err(list)) if !list.is_empty() => show_menu(&coords, list, &editor, &w),
            Ok(Err(_)) => hide(),
            Ok(Ok(e)) if !e.senses.is_empty() || e.summary.is_some() => show(&coords, &definitions_view(&e, &lang)),
            Ok(Ok(_)) => show(&coords, &message(&format!("No entry for \u{201c}{w}\u{201d} in the dictionary or the encyclopedia ({}).", dict::language(&lang).name))),
            Err(why) if want == Want::Definitions => show(&coords, &message(&format!("Could not look up \u{201c}{w}\u{201d}: {why}"))),
            Err(_) => hide(),
        }
    });
}

// ----- looking up

/// The notes written before 2026-10-28 (the entry as one JSON property, then the same again as Markdown) rewritten
/// once as properties only, their retrieval date kept; one of a single letter (a slip of the selection, never a word
/// looked up now) is deleted.
pub async fn rewrite_old_notes() {
    let Some((adapter, folder)) = with(|a| (a.adapter.clone(), a.settings.dict_folder.clone())) else { return };
    let folder = folder.trim().trim_matches('/').to_string();
    if folder.is_empty() || !js::wait(call!(adapter, exists, folder.clone())).await.is_ok_and(|v| v.is_truthy()) {
        return;
    }
    let (mut todo, mut rewritten, mut deleted) = (vec![folder], 0, 0);
    while let Some(dir) = todo.pop() {
        let Ok(l) = js::wait(call!(adapter, list, dir.as_str())).await else { continue };
        todo.extend(js_sys::Array::from(&get(&l, "folders")).iter().map(|f| str(&f)));
        for f in js_sys::Array::from(&get(&l, "files")).iter().map(|f| str(&f)).filter(|f| f.ends_with(".md")) {
            let Ok(md) = js::wait(call!(adapter, read, f.as_str())).await.map(|t| str(&t)) else { continue };
            if !dict::old_form(&md) {
                continue;
            }
            let Some(e) = dict::from_note(&md) else { continue };
            if e.word.chars().count() <= 1 {
                if js::wait(call!(adapter, remove, f.as_str())).await.is_ok() {
                    deleted += 1;
                }
                continue;
            }
            let date = md.lines().find_map(|l| l.strip_prefix("retrieved: ")).map(|d| d.trim().to_string()).unwrap_or_default();
            if js::wait(call!(adapter, write, f.as_str(), dict::note(&e, &date))).await.is_ok() {
                rewritten += 1;
            }
        }
    }
    if rewritten + deleted > 0 {
        let text = format!("Dictionary: {rewritten} notes rewritten as properties only; {deleted} of a single letter deleted");
        with(|a| new(&get(&a.obs, "Notice"), &[JsValue::from(text.as_str()), JsValue::from(8000.0)]));
    }
}

/// The word's note in the folder (a setting), cleaned by Obsidian's `normalizePath` as its guidelines ask for paths a
/// user types (<https://docs.obsidian.md/Reference/TypeScript+API/normalizePath>).
fn note_path(folder: &str, lang: &str, word: &str) -> String {
    let path = format!("{}/{}/{}.md", folder.trim().trim_end_matches('/'), dict::language(lang).section, dict::file_name(word));
    with(|a| get(&a.obs, "normalizePath")).filter(JsValue::is_function).map_or(path.clone(), |n| str(&js::apply(&n, &JsValue::NULL, &[path.into()])))
}

/// The vault's file or folder at `path` (`Vault.getAbstractFileByPath`), if there is one.
fn in_vault(vault: &JsValue, path: &str) -> Option<JsValue> {
    Some(call!(vault, getAbstractFileByPath, path)).filter(JsValue::is_object)
}

fn cache_key(lang: &str, w: &str) -> String {
    format!("{lang}\u{1}{}", key(w))
}

/// What is known of a word: this session's entry, else its note's, else an empty entry to fill.
async fn load(w: &str, lang: &str) -> Entry {
    if let Some(e) = state(|s| s.cache.get(&cache_key(lang, w)).cloned()) {
        return e;
    }
    let Some((vault, folder)) = with(|a| (a.vault.clone(), a.settings.dict_folder.clone())) else { return Entry::default() };
    // through the Vault API (its cache), as Obsidian's guidelines prefer to the adapter
    if let Some(file) = in_vault(&vault, &note_path(&folder, lang, &key(w))) {
        if let Ok(md) = js::wait(call!(vault, cachedRead, file)).await {
            if let Some(e) = dict::from_note(&str(&md)) {
                return e;
            }
        }
    }
    Entry { word: key(w), lang: dict::language(lang).section.into(), ..Default::default() }
}

/// Keep an entry for the session, and in its note when it holds anything (a word with no entry anywhere gets no note).
async fn store(lang: &str, e: &Entry) {
    state(|s| s.cache.insert(cache_key(lang, &e.word), e.clone()));
    let has = !e.senses.is_empty() || e.summary.is_some() || e.thesaurus.as_ref().is_some_and(|t| !t.is_empty());
    let Some((vault, folder)) = with(|a| (a.vault.clone(), a.settings.dict_folder.clone())) else { return };
    if !has {
        return;
    }
    let path = note_path(&folder, lang, &e.word);
    let date: String = str(&call!(new(&get(&js::global(), "Date"), &[]), toISOString)).chars().take(10).collect(); // YYYY-MM-DD
    let md = dict::note(e, &date);
    // the Vault API, as Obsidian's guidelines prefer: `process` for a note that exists (the same word looked up again
    // for more: both kept), else `create`, after `createFolder` for each missing folder on the way
    if let Some(file) = in_vault(&vault, &path) {
        _ = js::wait(call!(vault, process, file, f1(move |_| md.clone().into()))).await;
        return;
    }
    let mut dir = String::new();
    for part in path.split('/').collect::<Vec<_>>().split_last().map_or(&[][..], |(_, d)| d) {
        dir = if dir.is_empty() { part.to_string() } else { format!("{dir}/{part}") };
        if in_vault(&vault, &dir).is_none() {
            _ = js::wait(call!(vault, createFolder, dir.clone())).await;
        }
    }
    _ = js::wait(call!(vault, create, path, md)).await;
}

/// A word's synonyms, best first, with their parts of speech: Datamuse's, or Wiktionary's for a language Datamuse does
/// not cover.
pub async fn synonyms(w: &str, lang: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut e = load(w, lang).await;
    if e.thesaurus.is_none() {
        let Some(vocabulary) = dict::language(lang).datamuse else { return Ok(definitions(w, lang).await?.all_synonyms()) };
        e.thesaurus = Some(datamuse_synonyms(w, vocabulary).await?);
        store(lang, &e).await;
    }
    Ok(e.all_synonyms())
}

/// A word's definitions: the downloaded dictionary, else Wiktionary (the word as selected, then in lower case; for a
/// form such as "running", its lemma's senses after its own), and Wikipedia's summary when neither has it. If
/// Wikimedia cannot answer, Datamuse's short definitions stand in.
pub async fn definitions(w: &str, lang: &str) -> Result<Entry, String> {
    let mut e = load(w, lang).await;
    if e.defined {
        return Ok(e);
    }
    let found = match local(w, lang).await {
        Some(f) => Ok(Some(f)),
        None => wiktionary_with_forms(w, lang).await,
    };
    let mut trouble = None;
    match found {
        Ok(Some(f)) => {
            (e.senses, e.synonyms, e.form_of, e.lemma, e.source) = (f.senses, f.synonyms, f.form_of, f.lemma, f.source);
        }
        Ok(None) => {}
        Err(why) => match dict::language(lang).datamuse {
            Some(v) => match datamuse_definitions(w, v).await {
                Ok(Some(f)) => (e.senses, e.source) = (f.senses, f.source),
                _ => trouble = Some(why),
            },
            None => trouble = Some(why),
        },
    }
    if e.senses.is_empty() && e.summary.is_none() {
        match wikipedia(w, lang).await {
            Ok(s) => e.summary = s,
            Err(why) => trouble = trouble.or(Some(why)),
        }
    }
    if let Some(why) = trouble.filter(|_| e.senses.is_empty() && e.summary.is_none()) {
        return Err(why); // not kept: asked again next time
    }
    e.defined = true;
    e.order_for(lang);
    store(lang, &e).await;
    Ok(e)
}

/// Wiktionary for the word as selected, then in lower case; a form's lemma's senses follow its own.
async fn wiktionary_with_forms(w: &str, lang: &str) -> Result<Option<Entry>, String> {
    let lower = w.to_lowercase();
    let mut found = wiktionary(w, lang).await?;
    if found.is_none() && lower != w {
        found = wiktionary(&lower, lang).await?;
    }
    if let Some(e) = &mut found {
        if let Some(lemma) = e.form_of.first().cloned().filter(|l| !l.eq_ignore_ascii_case(w)) {
            if let Ok(Some(l)) = wiktionary(&lemma, lang).await {
                e.lemma = Some(l.word.clone());
                e.senses.extend(l.senses);
                e.synonyms.extend(l.synonyms);
            }
        }
    }
    Ok(found)
}

/// The language's downloaded dictionary, if there is one (files.rs; desktop only).
async fn local(w: &str, lang: &str) -> Option<Entry> {
    crate::files::lookup(lang, w).filter(|e| !e.senses.is_empty())
}

/// This plugin as Wikimedia asks a client to name itself: its name and version, and a contact (Settings → Dictionary).
pub fn user_agent() -> String {
    with(|a| {
        let version = str(&get(&get(&a.plugin, "manifest"), "version"));
        format!("DictionaryAndSynonyms/{version} ({}; Obsidian plugin)", a.settings.wikimedia_contact.trim())
    })
    .unwrap_or_default()
}

/// A GET through Obsidian's `requestUrl` (no CORS limits): (status, JSON, headers).
async fn request(url: String, wikimedia: bool) -> Result<(f64, JsValue, JsValue), String> {
    let request_url = with(|a| get(&a.obs, "requestUrl")).unwrap_or(JsValue::UNDEFINED);
    let headers = js::empty();
    if wikimedia {
        let ua = user_agent();
        set(&headers, "User-Agent", ua.clone());
        set(&headers, "Api-User-Agent", ua);
    }
    let res = js::wait(js::apply(&request_url, &JsValue::NULL, &[obj! { url: url, headers: headers, throw: false }.into()])).await
        .map_err(|e| str(&get(&e, "message")))?;
    let status = get(&res, "status").as_f64().unwrap_or(0.0);
    let json = if status == 200.0 { get(&res, "json") } else { JsValue::NULL };
    Ok((status, json, get(&res, "headers")))
}

/// A Wikimedia API request, unless Wikimedia has asked to wait; a 429 sets the wait (Retry-After, in seconds).
async fn wikimedia(url: String) -> Result<JsValue, String> {
    let now = js::now();
    let until = state(|s| s.wait_until);
    if now < until {
        return Err(format!("Wikimedia asked to wait; try again in {:.0} s", (until - now) / 1000.0));
    }
    let (status, json, headers) = request(url, true).await?;
    match status {
        200.0 => Ok(json),
        429.0 => {
            let wait = str(&get(&headers, "retry-after")).parse::<f64>().unwrap_or(WAIT_DEFAULT_S);
            state(|s| s.wait_until = js::now() + wait * 1000.0);
            Err(format!("Wikimedia asked to wait {wait:.0} s"))
        }
        s => Err(format!("Wikimedia answered {s}")),
    }
}

/// A Wiktionary page's wikitext (following redirects), parsed for the language; `None` if there is no page or no
/// section for the language.
async fn wiktionary(title: &str, lang: &str) -> Result<Option<Entry>, String> {
    let url = format!("https://en.wiktionary.org/w/api.php?action=query&format=json&formatversion=2&prop=revisions&rvprop=content&rvslots=main&redirects=1&titles={}",
        String::from(js_sys::encode_uri_component(title)));
    let json = wikimedia(url).await?;
    let page = js_sys::Reflect::get_u32(&get(&get(&json, "query"), "pages"), 0).unwrap_or(JsValue::UNDEFINED);
    let found_title = get(&page, "title").as_string().unwrap_or_else(|| title.to_string());
    let text = get(&get(&get(&js_sys::Reflect::get_u32(&get(&page, "revisions"), 0).unwrap_or(JsValue::UNDEFINED), "slots"), "main"), "content");
    Ok(text.as_string().and_then(|t| dict::parse(&found_title, &t, dict::language(lang).section, lang)).filter(|e| !e.senses.is_empty()))
}

/// Wikipedia's opening sentences on a term: the article of that title (following redirects), else the best search
/// result if every word of the term is in its title or opening (so "NetFlow" does not give "Netflix"); never a
/// disambiguation page.
async fn wikipedia(term: &str, lang: &str) -> Result<Option<Summary>, String> {
    let wiki = dict::language(lang).wiki();
    let api = format!("https://{wiki}.wikipedia.org/w/api.php?action=query&format=json&formatversion=2&prop=extracts|pageprops&ppprop=disambiguation&exintro=1&explaintext=1&exsentences=3&redirects=1");
    let q = String::from(js_sys::encode_uri_component(term));
    let page = |json: &JsValue| js_sys::Reflect::get_u32(&get(&get(json, "query"), "pages"), 0).unwrap_or(JsValue::UNDEFINED);
    let usable = |p: &JsValue| p.is_object() && !get(p, "missing").is_truthy() && !str(&get(p, "extract")).trim().is_empty() && !get(&get(p, "pageprops"), "disambiguation").is_string();
    let mut found = page(&wikimedia(format!("{api}&titles={q}")).await?);
    if !usable(&found) {
        found = page(&wikimedia(format!("{api}&generator=search&gsrlimit=1&gsrsearch={q}")).await?);
        let norm = |s: &str| s.to_lowercase().replace('-', " ").replace("isation", "ization").replace("ise", "ize");
        let seen = norm(&format!("{} {}", str(&get(&found, "title")), str(&get(&found, "extract"))));
        if !usable(&found) || !norm(term).split_whitespace().filter(|w| w.len() >= 3).all(|w| seen.contains(w)) {
            return Ok(None);
        }
    }
    let title = str(&get(&found, "title"));
    let url = format!("https://{wiki}.wikipedia.org/wiki/{}", title.replace(' ', "_"));
    Ok(Some(Summary { title, text: str(&get(&found, "extract")).trim().to_string(), url }))
}

/// Datamuse's part-of-speech tags as Wiktionary's headings.
fn datamuse_pos(tag: &str) -> Option<&'static str> {
    [("n", "Noun"), ("v", "Verb"), ("adj", "Adjective"), ("adv", "Adverb")].iter().find(|(t, _)| *t == tag).map(|(_, p)| *p)
}

fn datamuse_url(query: &str, vocabulary: &str) -> String {
    let v = if vocabulary.is_empty() { String::new() } else { format!("&v={vocabulary}") };
    format!("https://api.datamuse.com/words?{query}{v}")
}

/// Datamuse's synonyms of a word: its "means like" results tagged as synonyms, best first, at most 30.
async fn datamuse_synonyms(w: &str, vocabulary: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let (status, json, _) = request(datamuse_url(&format!("ml={}&md=p&max=100", String::from(js_sys::encode_uri_component(w))), vocabulary), false).await?;
    if status != 200.0 {
        return Err(format!("Datamuse answered {status}"));
    }
    let mut out = Vec::new();
    for r in js_sys::Array::from(&json).iter() {
        let tags: Vec<String> = js_sys::Array::from(&get(&r, "tags")).iter().map(|t| str(&t)).collect();
        let word = str(&get(&r, "word"));
        if tags.iter().any(|t| t == "syn") && !word.eq_ignore_ascii_case(w) {
            out.push((word, tags.iter().filter_map(|t| datamuse_pos(t)).map(String::from).collect()));
        }
        if out.len() == SHOWN_SYNONYMS {
            break;
        }
    }
    Ok(out)
}

/// Datamuse's short definitions (WordNet's and Wiktionary's), for when Wikimedia cannot answer.
async fn datamuse_definitions(w: &str, vocabulary: &str) -> Result<Option<Entry>, String> {
    let (status, json, _) = request(datamuse_url(&format!("sp={}&md=d&max=1", String::from(js_sys::encode_uri_component(w))), vocabulary), false).await?;
    if status != 200.0 {
        return Err(format!("Datamuse answered {status}"));
    }
    let first = js_sys::Reflect::get_u32(&json, 0).unwrap_or(JsValue::UNDEFINED);
    let senses: Vec<Sense> = js_sys::Array::from(&get(&first, "defs")).iter().filter_map(|d| {
        let d = str(&d);
        let (tag, text) = d.split_once('\t')?;
        Some(Sense { pos: datamuse_pos(tag).unwrap_or("Other").into(), text: text.trim().to_string(), ..Default::default() })
    }).collect();
    Ok((!senses.is_empty()).then(|| Entry { word: w.into(), senses, source: "https://www.datamuse.com/api/".into(), defined: true, ..Default::default() }))
}

// ----- the popover

fn el(tag: &str, cls: &str, text: &str) -> JsValue {
    let e = call!(get(&js::global(), "document"), createElement, tag);
    set(&e, "className", cls);
    if !text.is_empty() {
        set(&e, "textContent", text);
    }
    e
}

fn add(parent: &JsValue, child: &JsValue) {
    call!(parent, appendChild, child.clone());
}

/// Show `content` below the selection's start (above it when there is no room), within the window. A press inside it
/// keeps the editor's focus and selection.
fn show(coords: &JsValue, content: &JsValue) {
    hide();
    let doc = get(&js::global(), "document");
    let pop = el("div", "dict-popover", "");
    add(&pop, content);
    // the synonyms dropdown gets its own class, not `.dict-popover:has(.dict-menu)` (the community directory's CSS lint
    // warns that :has makes the browser re-check selectors broadly)
    if call!(get(content, "classList"), contains, "dict-menu") == JsValue::TRUE {
        call!(get(&pop, "classList"), add, "is-menu");
    }
    call!(pop, addEventListener, "mousedown", f1(|e| {
        call!(e, preventDefault);
        JsValue::UNDEFINED
    }));
    add(&get(&doc, "body"), &pop);
    let (w, h) = (get(&js::global(), "innerWidth").as_f64().unwrap_or(800.0), get(&js::global(), "innerHeight").as_f64().unwrap_or(600.0));
    let r = call!(pop, getBoundingClientRect);
    let (pw, ph) = (get(&r, "width").as_f64().unwrap_or(0.0), get(&r, "height").as_f64().unwrap_or(0.0));
    let n = |k: &str| get(coords, k).as_f64().unwrap_or(0.0);
    let left = n("left").min(w - pw - 8.0).max(8.0);
    let top = if n("bottom") + 4.0 + ph > h && n("top") - 4.0 - ph > 0.0 { n("top") - 4.0 - ph } else { n("bottom") + 4.0 };
    let style = get(&pop, "style");
    set(&style, "left", format!("{left}px"));
    set(&style, "top", format!("{top}px"));
    state(|s| s.popover = Some(pop));
}

fn message(text: &str) -> JsValue {
    el("div", "dict-popover-note", text)
}

/// A synonym in the selection's case: "Glad" for "Happy", "GLAD" for "HAPPY".
fn match_case(selected: &str, syn: &str) -> String {
    if selected.chars().all(|c| !c.is_lowercase()) && selected.chars().count() > 1 {
        syn.to_uppercase()
    } else if selected.chars().next().is_some_and(char::is_uppercase) {
        let mut c = syn.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    } else {
        syn.to_string()
    }
}

/// The synonyms as a dropdown: one per row, best first, each with its parts of speech; the first highlighted.
fn show_menu(coords: &JsValue, list: Vec<(String, Vec<String>)>, editor: &JsValue, selected: &str) {
    let menu = el("div", "dict-menu", "");
    let mut rows = Vec::new();
    for (i, (w, pos)) in list.into_iter().take(SHOWN_SYNONYMS).enumerate() {
        let row = el("div", "dict-option", "");
        add(&row, &el("span", "dict-option-word", &w));
        let short: Vec<String> = pos.iter().map(|p| match p.as_str() { "Adjective" => "adj.".into(), "Adverb" => "adv.".into(), "Noun" => "n.".into(), "Verb" => "v.".into(), o => o.to_lowercase() }).collect();
        add(&row, &el("span", "dict-option-pos", &short.join(", ")));
        call!(row, addEventListener, "mouseenter", f1(move |_| {
            highlight(i);
            JsValue::UNDEFINED
        }));
        call!(row, addEventListener, "click", f1(move |_| {
            choose(i);
            JsValue::UNDEFINED
        }));
        add(&menu, &row);
        rows.push((row, w));
    }
    show(coords, &menu);
    state(|s| s.menu = Some(Menu { rows, at: 0, editor: editor.clone(), selected: selected.to_string() }));
}

/// Highlight row `i` (the one under the mouse).
fn highlight(i: usize) {
    state(|s| {
        let Some(m) = s.menu.as_mut() else { return };
        m.at = i;
        for (j, (row, _)) in m.rows.iter().enumerate() {
            call!(get(row, "classList"), toggle, "is-selected", j == i);
        }
    });
}

/// Put row `i`'s synonym in place of the selection, in its case.
fn choose(i: usize) {
    let Some((editor, text)) = state(|s| s.menu.as_ref().and_then(|m| m.rows.get(i).map(|(_, w)| (m.editor.clone(), match_case(&m.selected, w))))) else { return };
    hide();
    call!(editor, replaceSelection, text);
}

fn definitions_view(e: &Entry, lang: &str) -> JsValue {
    let box_ = el("div", "", "");
    let head = el("div", "dict-popover-head", "");
    add(&head, &el("span", "dict-popover-word", &e.word));
    if let Some(l) = &e.lemma {
        add(&head, &el("span", "dict-popover-hint", &format!("form of {l}")));
    }
    add(&box_, &head);
    let body = el("div", "dict-definitions", "");
    let mut groups: Vec<(&str, Vec<&Sense>)> = Vec::new();
    for s in &e.senses {
        match groups.iter_mut().find(|(p, _)| *p == s.pos) {
            Some((_, v)) => v.push(s),
            None => groups.push((&s.pos, vec![s])),
        }
    }
    for (pos, senses) in groups {
        add(&body, &el("div", "dict-pos", pos));
        let ol = el("ol", "", "");
        for s in senses.iter().take(SHOWN_SENSES) {
            let li = el("li", "", "");
            if !s.labels.is_empty() {
                add(&li, &el("span", "dict-labels", &format!("({}) ", s.labels.join(", "))));
            }
            add(&li, &el("span", "", &s.text));
            if !s.synonyms.is_empty() {
                add(&li, &el("div", "dict-sense-synonyms", &format!("synonyms: {}", s.synonyms.join(", "))));
            }
            add(&ol, &li);
        }
        if senses.len() > SHOWN_SENSES {
            add(&ol, &el("li", "dict-more", &format!("{} more in the note", senses.len() - SHOWN_SENSES)));
        }
        add(&body, &ol);
    }
    if let Some(s) = &e.summary {
        add(&body, &el("div", "dict-pos", &format!("Encyclopedia: {}", s.title)));
        add(&body, &el("div", "dict-summary", &s.text));
    }
    add(&box_, &body);
    let foot = el("div", "dict-popover-foot", "");
    let open = el("a", "", "Open note");
    let path = with(|a| note_path(&a.settings.dict_folder, lang, &e.word)).unwrap_or_default();
    call!(open, addEventListener, "click", f1(move |_| {
        let p = path.clone();
        with(|a| call!(get(&get(&a.plugin, "app"), "workspace"), openLinkText, p, "", true));
        hide();
        JsValue::UNDEFINED
    }));
    add(&foot, &open);
    let mut from = Vec::new();
    if e.source.contains("wiktionary") {
        from.push("Wiktionary");
    } else if e.source.contains("datamuse") {
        from.push("Datamuse (Wikimedia was busy)");
    } else if e.source.contains("en-word.net") {
        from.push("Open English WordNet");
    }
    if e.summary.is_some() {
        from.push("Wikipedia");
    }
    add(&foot, &el("span", "dict-popover-hint", &format!(" \u{b7} {}", from.join(", "))));
    add(&box_, &foot);
    box_
}
