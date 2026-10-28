//! Whole dictionaries, downloaded only when the user asks (Settings), for lookups without the network: Open English
//! WordNet (English, 11 MB, <https://en-word.net/downloads>, CC BY 4.0) or kaikki.org's extract of English Wiktionary
//! (every language's entries, 3.1 GB, <https://kaikki.org/dictionary/rawdata.html>), of which one language is kept.
//! Desktop only: it uses Node's modules, which Obsidian's desktop app provides (`require`): https to download, zlib to
//! decompress, fs for the files (<https://nodejs.org/api/https.html>, <https://nodejs.org/api/zlib.html>,
//! <https://nodejs.org/api/fs.html>).
//!
//! - The download resumes where it stopped (an HTTP Range request,
//!   <https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Range_requests>) and a dropped connection or a busy
//!   server is retried with exponential backoff (backon, <https://docs.rs/backon>); Cancel stops it and keeps what has
//!   arrived, for the next Download.
//! - Decompression runs in Node, off the main thread; the decompressed text goes, a megabyte at a time, to a Web Worker
//!   (worker.rs) that keeps the language's entries and sorts them, so Obsidian stays responsive.
//! - The result, one `word \t entry-JSON` line per word, sorted, is kept outside the vault (the operating system's
//!   per-user cache), and the download is deleted. A lookup is a binary search reading a few kilobytes.

use std::cell::RefCell;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use js_sys::{Array, Function, Promise, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::app::with;
use crate::dict::{self, Entry};
use crate::js::{self, call, f0, f1, get, obj, set, str};

#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    WordNet,
    Wiktionary,
}

impl Source {
    fn url(self) -> &'static str {
        match self {
            Source::WordNet => "https://en-word.net/downloads/english-wordnet-2025.xml.gz",
            Source::Wiktionary => "https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Source::WordNet => "wordnet",
            Source::Wiktionary => "wiktionary",
        }
    }
}

/// The download in progress (one at a time), as the settings rows show it.
#[derive(Clone, Default)]
pub struct Job {
    source: Option<Source>,
    lang: String,
    pub state: String, // downloading, processing, ready, cancelled, failed
    pub done: f64,
    pub total: f64,
    pub entries: f64,
    pub error: String,
    cancel: bool,
    request: Option<JsValue>, // the HTTP request or the read stream, to stop on Cancel
}

thread_local! {
    static JOB: RefCell<Job> = RefCell::new(Job::default());
    static SIZES: RefCell<Vec<(&'static str, f64)>> = const { RefCell::new(Vec::new()) };
}

fn job<R>(f: impl FnOnce(&mut Job) -> R) -> R {
    JOB.with(|j| f(&mut j.borrow_mut()))
}

/// A Node module; `None` where there is no Node (Obsidian on a phone, a browser).
fn node(name: &str) -> Option<JsValue> {
    with(|a| a.require.as_ref().and_then(|r| r.call1(&JsValue::NULL, &name.into()).ok())).flatten().filter(JsValue::is_object)
}

/// The folder for downloaded dictionaries, outside the vault: Windows `%LOCALAPPDATA%`, macOS `~/Library/Caches`,
/// elsewhere `$XDG_CACHE_HOME` or `~/.cache`, then `dictionary-and-synonyms`.
pub fn dir() -> Option<String> {
    let (os, path) = (node("os")?, node("path")?);
    let process = get(&js::global(), "process");
    let env = |k: &str| get(&get(&process, "env"), k).as_string().filter(|v| !v.is_empty());
    let home = str(&call!(os, homedir));
    let base = match str(&get(&process, "platform")).as_str() {
        "win32" => env("LOCALAPPDATA").unwrap_or_else(|| str(&call!(path, join, home, "AppData", "Local"))),
        "darwin" => str(&call!(path, join, home, "Library", "Caches")),
        _ => env("XDG_CACHE_HOME").unwrap_or_else(|| str(&call!(path, join, home, ".cache"))),
    };
    Some(str(&call!(path, join, base, "dictionary-and-synonyms")))
}

fn file(lang: &str, src: Source) -> Option<String> {
    let path = node("path")?;
    Some(str(&call!(path, join, dir()?, format!("{}-{}.tsv", dict::language(lang).wiki(), src.tag()))))
}

fn exists(p: &str) -> bool {
    node("fs").is_some_and(|fs| call!(fs, existsSync, p).is_truthy())
}

fn size_of(p: &str) -> f64 {
    node("fs").map_or(0.0, |fs| get(&call!(fs, statSync, p), "size").as_f64().unwrap_or(0.0))
}

// ----- lookups

/// Bytes `[at, at + len)` of an open file (fewer at its end).
fn read(fs: &JsValue, fd: &JsValue, at: f64, len: u32) -> Vec<u8> {
    let buf = Uint8Array::new_with_length(len);
    let n = call!(fs, readSync, fd.clone(), buf.clone(), 0, len, at).as_f64().unwrap_or(0.0) as u32;
    buf.subarray(0, n).to_vec()
}

/// The line starting at or after byte `at` (the very first line when `at` is 0): (its word, its entry JSON).
fn line_at(fs: &JsValue, fd: &JsValue, at: f64, size: f64) -> Option<(String, String)> {
    let mut pos = at;
    if at > 0.0 {
        loop {
            let chunk = read(fs, fd, pos, 65536);
            if chunk.is_empty() {
                return None;
            }
            if let Some(i) = chunk.iter().position(|&b| b == b'\n') {
                pos += i as f64 + 1.0;
                break;
            }
            pos += chunk.len() as f64;
        }
    }
    let mut line = Vec::new();
    while pos < size {
        let chunk = read(fs, fd, pos, 65536);
        if chunk.is_empty() {
            break;
        }
        match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => {
                line.extend_from_slice(&chunk[..i]);
                break;
            }
            None => {
                line.extend_from_slice(&chunk);
                pos += chunk.len() as f64;
            }
        }
    }
    let line = String::from_utf8(line).ok()?;
    let (w, e) = line.split_once('\t')?;
    Some((w.to_string(), e.to_string()))
}

/// A word in one sorted file: a binary search, the word as given and then in lower case.
fn lookup_in(path: &str, word: &str) -> Option<Entry> {
    let fs = node("fs")?;
    if !exists(path) {
        return None;
    }
    let fd = call!(fs, openSync, path, "r");
    let size = get(&call!(fs, fstatSync, fd.clone()), "size").as_f64().unwrap_or(0.0);
    let mut found = None;
    for w in [word.to_string(), word.to_lowercase()] {
        let (mut lo, mut hi) = (0.0f64, size);
        while lo < hi {
            let mid = (lo + (hi - lo) / 2.0).floor();
            match line_at(&fs, &fd, mid, size) {
                Some((k, _)) if k.as_bytes() < w.as_bytes() => lo = mid + 1.0,
                _ => hi = mid,
            }
        }
        // the line at `lo` is the first whose word is not before `w` (a position inside a line reads the next one)
        if let Some((_, e)) = line_at(&fs, &fd, lo, size).filter(|(k, _)| *k == w) {
            found = serde_json::from_str(&e).ok();
            break;
        }
    }
    call!(fs, closeSync, fd);
    found
}

/// A word's entry from the language's downloaded dictionaries (Wiktionary's first, then WordNet's); `None` without one.
pub fn lookup(lang: &str, word: &str) -> Option<Entry> {
    [Source::Wiktionary, Source::WordNet].into_iter().find_map(|src| lookup_in(&file(lang, src)?, word))
}

// ----- downloading

enum Fail {
    Retry(String),
    Stop(String),
}

/// One attempt at the download into `part`, resumed from what is there; follows redirects.
async fn fetch_once(src: Source, part: String) -> Result<(), Fail> {
    let (fs, https) = (node("fs").ok_or(Fail::Stop("no file system here".into()))?, node("https").ok_or(Fail::Stop("no network module here".into()))?);
    let mut url = src.url().to_string();
    for _ in 0..5 {
        let have = if exists(&part) { size_of(&part) } else { 0.0 };
        let headers = js::empty();
        set(&headers, "User-Agent", crate::lookup::user_agent());
        if have > 0.0 {
            set(&headers, "Range", format!("bytes={have}-"));
        }
        let (fs2, part2, url2) = (fs.clone(), part.clone(), url.clone());
        let attempt = Promise::new(&mut |resolve: Function, reject: Function| {
            let (fs, part) = (fs2.clone(), part2.clone());
            let rej = reject.clone();
            let req = call!(https, get, url2.clone(), obj! { headers: headers.clone() }, f1(move |res| {
                let code = get(&res, "statusCode").as_f64().unwrap_or(0.0);
                let location = get(&get(&res, "headers"), "location");
                if (300.0..400.0).contains(&code) && location.is_string() {
                    call!(res, resume);
                    _ = resolve.call1(&JsValue::NULL, &obj! { redirect: location });
                    return JsValue::UNDEFINED;
                }
                if code == 416.0 {
                    call!(res, resume);
                    _ = resolve.call1(&JsValue::NULL, &obj! { done: true });
                    return JsValue::UNDEFINED;
                }
                if code != 200.0 && code != 206.0 {
                    call!(res, resume);
                    let retry = code == 429.0 || code >= 500.0;
                    _ = reject.call1(&JsValue::NULL, &obj! { retry: retry, message: format!("the source answered {code}") });
                    return JsValue::UNDEFINED;
                }
                let start = if code == 206.0 { have } else { 0.0 };
                let length = str(&get(&get(&res, "headers"), "content-length")).parse::<f64>().unwrap_or(0.0);
                let total = start + length;
                job(|j| (j.done, j.total) = (start, total));
                let out = call!(fs, createWriteStream, part.clone(), obj! { flags: if code == 206.0 { "a" } else { "w" } });
                call!(res, on, "data", f1(|chunk| {
                    let n = get(&chunk, "length").as_f64().unwrap_or(0.0);
                    job(|j| j.done += n);
                    JsValue::UNDEFINED
                }));
                let r = reject.clone();
                call!(res, on, "error", f1(move |e| {
                    _ = r.call1(&JsValue::NULL, &obj! { retry: true, message: str(&get(&e, "message")) });
                    JsValue::UNDEFINED
                }));
                let res_ok = resolve.clone();
                call!(out, on, "finish", f0(move || _ = res_ok.call1(&JsValue::NULL, &obj! { total: total })));
                call!(res, pipe, out);
                JsValue::UNDEFINED
            }));
            call!(req, on, "error", f1(move |e| {
                _ = rej.call1(&JsValue::NULL, &obj! { retry: true, message: str(&get(&e, "message")) });
                JsValue::UNDEFINED
            }));
            job(|j| j.request = Some(req));
        });
        let got = js::wait(attempt.into()).await;
        if job(|j| j.cancel) {
            return Err(Fail::Stop("cancelled".into()));
        }
        match got {
            Ok(v) if get(&v, "redirect").is_string() => url = str(&get(&v, "redirect")),
            Ok(v) if get(&v, "done").is_truthy() => return Ok(()),
            Ok(v) => {
                let total = get(&v, "total").as_f64().unwrap_or(0.0);
                let have = size_of(&part);
                return if total > 0.0 && have < total {
                    Err(Fail::Retry(format!("the connection closed at {:.0} of {:.0} MB", have / 1e6, total / 1e6)))
                } else {
                    Ok(())
                };
            }
            Err(e) if get(&e, "retry").is_truthy() => return Err(Fail::Retry(str(&get(&e, "message")))),
            Err(e) => return Err(Fail::Stop(str(&get(&e, "message")))),
        }
    }
    Err(Fail::Stop("too many redirects".into()))
}

/// Download `src` into the folder (resuming a partial file), retrying with exponential backoff.
async fn download(src: Source) -> Result<String, String> {
    let fs = node("fs").ok_or("downloads need Obsidian's desktop app")?;
    let path = node("path").ok_or("downloads need Obsidian's desktop app")?;
    let folder = dir().ok_or("no folder for downloads")?;
    call!(fs, mkdirSync, folder.clone(), obj! { recursive: true });
    let name = src.url().rsplit('/').next().unwrap_or("download.gz");
    let gz = str(&call!(path, join, folder.clone(), name));
    if exists(&gz) {
        return Ok(gz); // downloaded before, not yet processed
    }
    let part = format!("{gz}.part");
    let backoff = ExponentialBuilder::default().with_min_delay(Duration::from_secs(2)).with_max_delay(Duration::from_secs(120)).with_max_times(12);
    (|| fetch_once(src, part.clone()))
        .retry(backoff)
        .sleep(|d: Duration| js::sleep(d.as_millis() as f64))
        .when(|e| matches!(e, Fail::Retry(_)) && !job(|j| j.cancel))
        .notify(|e, wait| {
            if let Fail::Retry(why) = e {
                job(|j| j.error = format!("{why}; trying again in {} s", wait.as_secs()));
            }
        })
        .await
        .map_err(|e| match e {
            Fail::Retry(s) | Fail::Stop(s) => s,
        })?;
    call!(fs, renameSync, part, gz.clone());
    Ok(gz)
}

/// Decompress the download in Node and stream it to the worker, which keeps the language's entries; then write its
/// sorted file and delete the download.
async fn process(src: Source, lang: &str, gz: &str) -> Result<(), String> {
    let (fs, zlib) = (node("fs").ok_or("no file system")?, node("zlib").ok_or("no zlib")?);
    let total = size_of(gz);
    job(|j| (j.state, j.done, j.total, j.error) = ("processing".into(), 0.0, total, String::new()));
    let worker = with(|a| a.worker()).ok_or("no worker")?;
    call!(worker, postMessage, obj! { kind: "begin", source: src.tag(), lang: dict::language(lang).wiki() });
    let input = call!(fs, createReadStream, gz, obj! { highWaterMark: 1 << 20 });
    let gunzip = call!(zlib, createGunzip, obj! { chunkSize: 1 << 20 });
    job(|j| j.request = Some(input.clone()));
    call!(input, on, "data", f1(|c| {
        let n = get(&c, "length").as_f64().unwrap_or(0.0);
        job(|j| j.done += n);
        JsValue::UNDEFINED
    }));
    let (w2, g2) = (worker.clone(), gunzip.clone());
    let result = Promise::new(&mut |resolve: Function, reject: Function| {
        let (worker, gunzip) = (w2.clone(), g2.clone());
        // one chunk at a time: paused until the worker has taken it
        let (w, g) = (worker.clone(), gunzip.clone());
        call!(gunzip, on, "data", f1(move |chunk| {
            call!(g, pause);
            let copy = Uint8Array::new(&chunk); // Node's Buffer may share a pooled block: send a copy, transferred
            call!(w, postMessage, obj! { kind: "chunk", data: copy.clone() }, Array::of1(&copy.buffer()));
            JsValue::UNDEFINED
        }));
        let w = worker.clone();
        call!(gunzip, on, "end", f0(move || _ = call!(w, postMessage, obj! { kind: "finish" })));
        let rej = reject.clone();
        call!(gunzip, on, "error", f1(move |e| {
            _ = rej.call1(&JsValue::NULL, &get(&e, "message"));
            JsValue::UNDEFINED
        }));
        let g = gunzip.clone();
        set(&worker, "onmessage", f1(move |e| {
            let m = get(&e, "data");
            match str(&get(&m, "kind")).as_str() {
                "ack" => {
                    job(|j| j.entries = get(&m, "entries").as_f64().unwrap_or(j.entries));
                    if job(|j| j.cancel) {
                        _ = reject.call1(&JsValue::NULL, &"cancelled".into());
                    } else {
                        call!(g, resume);
                    }
                }
                "done" => {
                    job(|j| j.entries = get(&m, "entries").as_f64().unwrap_or(0.0));
                    _ = resolve.call1(&JsValue::NULL, &get(&m, "data"));
                }
                _ => _ = reject.call1(&JsValue::NULL, &get(&m, "message")),
            }
            JsValue::UNDEFINED
        }));
        call!(input, pipe, gunzip);
    });
    let data = js::wait(result.into()).await.map_err(|e| str(&e));
    call!(worker, terminate);
    let data = data?;
    let out = file(lang, src).ok_or("no folder")?;
    let tmp = format!("{out}.part");
    js::wait(call!(get(&fs, "promises"), writeFile, tmp.clone(), data)).await.map_err(|e| str(&get(&e, "message")))?;
    call!(fs, renameSync, tmp, out);
    call!(fs, unlinkSync, gz);
    Ok(())
}

/// Start downloading (or resuming) and processing `src` for the language, unless a download is running.
pub fn start(lang: &str, src: Source) {
    let busy = job(|j| ["downloading", "processing"].contains(&j.state.as_str()));
    if busy {
        return;
    }
    job(|j| *j = Job { source: Some(src), lang: dict::language(lang).wiki().into(), state: "downloading".into(), ..Default::default() });
    let lang = lang.to_string();
    js::spawn(async move {
        let result = match download(src).await {
            Ok(gz) => process(src, &lang, &gz).await,
            Err(e) => Err(e),
        };
        job(|j| {
            j.request = None;
            match result {
                Ok(()) => j.state = "ready".into(),
                Err(_) if j.cancel => j.state = "cancelled".into(),
                Err(e) => (j.state, j.error) = ("failed".into(), e),
            }
        });
    });
}

/// Stop the download (what has arrived is kept for the next Download) or the processing.
pub fn cancel() {
    let request = job(|j| {
        j.cancel = true;
        j.request.take()
    });
    if let Some(r) = request {
        call!(r, destroy);
    }
}

/// Remove the language's file from `src`.
pub fn delete(lang: &str, src: Source) {
    if let (Some(fs), Some(f)) = (node("fs"), file(lang, src)) {
        if exists(&f) {
            call!(fs, unlinkSync, f);
        }
    }
    job(|j| {
        if j.source == Some(src) && !["downloading", "processing"].contains(&j.state.as_str()) {
            *j = Job::default();
        }
    });
}

/// What the settings row shows for `src` and the language: the running job's state, or whether the file is there.
pub fn status(lang: &str, src: Source) -> Job {
    let j = job(|j| j.clone());
    if j.source == Some(src) && j.lang == dict::language(lang).wiki() && j.state != "ready" {
        return j;
    }
    match file(lang, src).filter(|f| exists(f)) {
        Some(f) => Job { state: "ready".into(), total: size_of(&f), ..Default::default() },
        None => Job { state: "none".into(), ..Default::default() },
    }
}

/// The download's size, asked once per source with a HEAD request (`requestUrl`, which has no cross-origin limits).
async fn source_size(src: Source) -> Option<f64> {
    if let Some(n) = SIZES.with(|s| s.borrow().iter().find(|(u, _)| *u == src.url()).map(|(_, n)| *n)) {
        return Some(n);
    }
    let request_url = with(|a| get(&a.obs, "requestUrl"))?;
    let res = js::wait(js::apply(&request_url, &JsValue::NULL, &[obj! { url: src.url(), method: "HEAD", throw: false }])).await.ok()?;
    let n = str(&get(&get(&res, "headers"), "content-length")).parse::<f64>().ok()?;
    SIZES.with(|s| s.borrow_mut().push((src.url(), n)));
    Some(n)
}

// ----- the settings rows

#[derive(Clone)]
struct Row {
    source: Source,
    text: JsValue,
    bar: JsValue,
    buttons: Vec<(&'static str, JsValue)>,
}

pub fn wordnet_row(setting: &JsValue) {
    row(setting, Source::WordNet);
}

pub fn wiktionary_row(setting: &JsValue) {
    row(setting, Source::Wiktionary);
}

/// A settings row that downloads a dictionary: its size, a progress bar while it downloads and processes, and
/// Download, Cancel, Redownload, Delete. It updates every second while the settings tab shows it.
fn row(setting: &JsValue, source: Source) {
    let doc = get(&js::global(), "document");
    let el = |tag: &str, cls: &str| {
        let e = call!(doc, createElement, tag);
        set(&e, "className", cls);
        e
    };
    let (desc, controls) = (get(setting, "descEl"), get(setting, "controlEl"));
    let mut row = Row { source, text: el("div", ""), bar: el("progress", "dict-progress"), buttons: Vec::new() };
    call!(desc, appendChild, row.text.clone());
    call!(desc, appendChild, row.bar.clone());
    for action in ["download", "cancel", "redownload", "delete"] {
        let b = el("button", if action == "download" { "mod-cta" } else { "" });
        let mut label = action.to_string();
        label[..1].make_ascii_uppercase();
        set(&b, "textContent", label);
        call!(controls, appendChild, b.clone());
        call!(b, addEventListener, "click", f1(move |_| {
            let Some(lang) = with(|a| a.settings.dict_lang.clone()) else { return JsValue::UNDEFINED };
            match action {
                "download" => start(&lang, source),
                "cancel" => cancel(),
                "redownload" => {
                    delete(&lang, source);
                    start(&lang, source);
                }
                _ => delete(&lang, source),
            }
            JsValue::UNDEFINED
        }));
        row.buttons.push((action, b));
    }
    poll(row);
}

fn poll(row: Row) {
    js::spawn(async move {
        refresh(&row).await;
        if !get(&row.text, "isConnected").is_truthy() {
            return; // the settings tab was closed
        }
        js::timeout(1000.0, move || poll(row));
    });
}

async fn refresh(row: &Row) {
    let Some(lang) = with(|a| a.settings.dict_lang.clone()) else { return };
    let name = dict::language(&lang).name;
    let wordnet = row.source == Source::WordNet;
    let show = |text: String, bar: Option<f64>, buttons: &[&str]| {
        set(&row.text, "textContent", text);
        // Obsidian's `HTMLElement.toggle(show)` rather than styles set from code (the plugin guidelines)
        call!(row.bar, toggle, bar.is_some());
        set(&row.bar, "max", 1.0);
        set(&row.bar, "value", bar.unwrap_or(0.0));
        for (action, b) in &row.buttons {
            call!(b, toggle, buttons.contains(action));
        }
    };
    if node("fs").is_none() {
        return show("Downloads need Obsidian's desktop app.".into(), None, &[]);
    }
    if wordnet && dict::language(&lang).wiki() != "en" {
        return show("Open English WordNet has English only.".into(), None, &[]);
    }
    let size = |b: f64| if b >= 1e9 { format!("{:.1} GB", b / 1e9) } else { format!("{:.0} MB", b / 1e6) };
    let source = match source_size(row.source).await {
        Some(n) => size(n),
        None if wordnet => "about 11 MB".into(),
        None => "about 3.1 GB".into(),
    };
    let s = status(&lang, row.source);
    let error = if s.error.is_empty() { String::new() } else { format!(" ({})", s.error) };
    match s.state.as_str() {
        "downloading" => show(format!("Downloading: {} of {}{error}", size(s.done), size(s.total)), Some(s.done / s.total.max(1.0)), &["cancel"]),
        "processing" => show(format!("Keeping the {name} entries: {:.0} words so far.", s.entries), Some(s.done / s.total.max(1.0)), &["cancel"]),
        "ready" => show(format!("Downloaded ({} on disk, outside the vault): lookups use it without the network.", size(s.total)), None, &["redownload", "delete"]),
        "cancelled" => show(format!("Cancelled; Download continues from where it stopped ({source} in all)."), None, &["download"]),
        "failed" => show(format!("The download failed: {}. Download tries again from where it stopped.", s.error), None, &["download"]),
        _ if wordnet => show(format!("Open English WordNet, for lookups without the network: {source} to download (definitions and synonyms of about 150,000 English words and phrases; CC BY 4.0; no British or American labels)."), None, &["download"]),
        _ => show(format!("Every {name} entry of Wiktionary, for lookups without the network: {source} to download from kaikki.org (every language's entries; only {name} is kept, a few hundred MB, and the download is then deleted)."), None, &["download"]),
    }
}
