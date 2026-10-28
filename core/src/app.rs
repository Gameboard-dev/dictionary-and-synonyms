//! The plugin on Obsidian's main thread: its settings (data.json) and settings tab, the events that open the popover
//! (lookup.rs), and the worker that prepares downloaded dictionaries (files.rs, worker.rs).
//!
//! Obsidian's API: <https://docs.obsidian.md/Reference/TypeScript+API/Plugin> (loadData, saveData, addSettingTab,
//! registerDomEvent); Web Workers from a Blob (<https://developer.mozilla.org/en-US/docs/Web/API/Worker/Worker>).

use std::cell::RefCell;

use js_sys::{Array, Function};
use obsidian_kit::settings::{self, Ctl, Row};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::js::{self, call, f0, f1, get, new, obj, set};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Synonyms when a word is selected; definitions when it is selected with Ctrl (lookup.rs).
    pub synonyms_on_select: bool,
    pub definitions_on_ctrl: bool,
    /// A code from dict::LANGUAGES ("en-GB", "en-US", "fr" …).
    pub dict_lang: String,
    /// The vault folder whose notes keep the lookups, one subfolder per language.
    pub dict_folder: String,
    /// The contact Wikimedia asks a client to give in its User-Agent (lookup.rs).
    pub wikimedia_contact: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            synonyms_on_select: true,
            definitions_on_ctrl: true,
            dict_lang: "en-GB".into(),
            dict_folder: "Dictionary".into(),
            wikimedia_contact: "https://github.com/Gameboard-dev/dictionary-and-synonyms".into(),
        }
    }
}

pub struct App {
    pub plugin: JsValue,
    pub obs: JsValue,
    pub vault: JsValue,
    pub adapter: JsValue,
    /// Node's `require` on the desktop; `None` on a phone (no downloads there).
    pub require: Option<Function>,
    pub settings: Settings,
    module: JsValue,
    glue: String,
    persist: Option<f64>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Run `f` on the app (`None` once the plugin is unloaded).
pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

const UI: &[Row] = &[
    ("", "Documentation", "How the plugin works, its sources and its settings: ", Ctl::Link("https://github.com/Gameboard-dev/dictionary-and-synonyms#readme")),
    ("", "", "Select a word to see its synonyms; click one to put it in its place. Select it with Ctrl held (Cmd on macOS) to see its definitions. Each word is looked up once and kept as a note in the folder below.", Ctl::Note),
    ("synonymsOnSelect", "Synonyms on select", "", Ctl::Toggle),
    ("definitionsOnCtrl", "Definitions on Ctrl+select", "", Ctl::Toggle),
    ("dictLang", "Language", "Whose entries to show. British and American English list their own variety's senses first and label the other's.", Ctl::Choice(languages)),
    ("dictFolder", "Folder", "Where the looked-up words are kept, one note each, in a subfolder per language.", Ctl::Text),
    ("wikimediaContact", "Contact for Wikimedia", "Wikimedia asks every program that uses Wiktionary and Wikipedia to name a contact (a URL or an email address) in its requests; it allows 200 lookups a minute to one that does and 10 to one that does not.", Ctl::Text),
    ("", "Download a whole dictionary", "", Ctl::Head),
    ("", "", "For lookups without the network (desktop only). Downloaded dictionaries are kept outside the vault, in your computer's cache folder.", Ctl::Note),
    ("", "Open English WordNet", "", Ctl::Custom(crate::files::wordnet_row)),
    ("", "Wiktionary", "", Ctl::Custom(crate::files::wiktionary_row)),
];

fn languages() -> Vec<(&'static str, &'static str)> {
    crate::dict::LANGUAGES.iter().map(|l| (l.code, l.name)).collect()
}

#[wasm_bindgen]
pub struct Handle;

#[wasm_bindgen]
impl Handle {
    pub fn unload(&self) {
        crate::lookup::hide();
        crate::files::cancel();
        APP.with(|a| a.borrow_mut().take());
    }
}

/// Called by main.js: the plugin object, the `obsidian` module, `require` (null on a phone), the compiled module and
/// its bindings.
#[wasm_bindgen]
pub fn start(plugin: JsValue, obs: JsValue, require: JsValue, module: JsValue, glue: String) -> Handle {
    let vault = get(&get(&plugin, "app"), "vault");
    let app = App {
        adapter: get(&vault, "adapter"),
        // Ignis (Obsidian in a browser) hands plugins look-alikes of Node's modules that cannot download or unpack:
        // there, as on a phone, nothing is downloaded and lookups use the online sources
        require: require.dyn_into::<Function>().ok().filter(|_| !crate::js::in_ignis()),
        settings: Settings::default(),
        persist: None,
        plugin,
        obs,
        vault,
        module,
        glue,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    js::spawn(async {
        let Some(plugin) = with(|a| a.plugin.clone()) else { return };
        let data = js::wait(call!(plugin, loadData)).await.unwrap_or(JsValue::NULL);
        let settings = serde_wasm_bindgen::from_value::<Settings>(data).unwrap_or_default();
        with(|a| {
            a.settings = settings;
            a.register();
        });
        crate::lookup::rewrite_old_notes().await;
    });
    Handle
}

impl App {
    /// The settings tab, and the popover's events: after a selection, and closed by any other click, key or scroll.
    fn register(&self) {
        let p = self.plugin.clone();
        let tab = new(&get(&self.obs, "PluginSettingTab"), &[get(&p, "app"), p.clone()]);
        let t = tab.clone();
        set(&tab, "display", f0(move || {
            let Some(obs) = with(|a| a.obs.clone()) else { return };
            settings::render(&t, &obs, UI, |k| with(|a| a.field(k)).unwrap_or(JsValue::UNDEFINED), |k, v| _ = with(|a| a.set_field(k, v)));
        }));
        call!(p, addSettingTab, tab);
        let doc = get(&js::global(), "document");
        let dom = |event: &str, f: JsValue| _ = call!(p, registerDomEvent, doc.clone(), event, f);
        dom("mouseup", f1(|e| {
            crate::lookup::mouseup(&e);
            JsValue::UNDEFINED
        }));
        dom("keyup", f1(|e| {
            crate::lookup::keyup(&e);
            JsValue::UNDEFINED
        }));
        dom("mousedown", f1(|e| {
            crate::lookup::dismiss(&e);
            JsValue::UNDEFINED
        }));
        dom("keydown", f1(|e| {
            crate::lookup::keydown(&e);
            JsValue::UNDEFINED
        }));
        call!(p, registerDomEvent, doc, "scroll", f1(|_| {
            crate::lookup::hide();
            JsValue::UNDEFINED
        }), obj! { capture: true, passive: true });
    }

    fn field(&self, key: &str) -> JsValue {
        get(&serde_wasm_bindgen::to_value(&self.settings).unwrap_or_default(), key)
    }

    fn set_field(&mut self, key: &str, v: JsValue) {
        let o = serde_wasm_bindgen::to_value(&self.settings).unwrap_or_default();
        set(&o, key, v);
        if let Ok(s) = serde_wasm_bindgen::from_value::<Settings>(o) {
            self.settings = s;
        }
        if let Some(h) = self.persist.take() {
            js::clear_timeout(h);
        }
        // data.json after a pause, so typing in a text field writes once
        self.persist = Some(js::timeout(500.0, || {
            with(|a| {
                a.persist = None;
                call!(a.plugin, saveData, serde_wasm_bindgen::to_value(&a.settings).unwrap_or_default());
            });
        }));
    }

    /// A new worker running this module (worker.rs), started from a Blob, so nothing but main.js and the .wasm is
    /// shipped; a compiled `WebAssembly.Module` can be posted to it.
    pub fn worker(&self) -> JsValue {
        let src = format!("{}\nself.onmessage = e => {{ self.onmessage = null; wasm_bindgen.initSync({{ module: e.data }}); wasm_bindgen.worker_start(); }};", self.glue);
        let opts = js::empty();
        set(&opts, "type", "text/javascript");
        let blob = new(&get(&js::global(), "Blob"), &[Array::of1(&src.into()).into(), opts]);
        let url = call!(get(&js::global(), "URL"), createObjectURL, blob);
        let worker = new(&get(&js::global(), "Worker"), &[url]);
        call!(worker, postMessage, self.module.clone());
        worker
    }
}
