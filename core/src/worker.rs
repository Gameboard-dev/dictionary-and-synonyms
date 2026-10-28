//! In a Web Worker (started from the same module, app.rs): a downloaded dictionary's decompressed text, a chunk at a
//! time, made into the sorted file files.rs searches: one `word \t entry-JSON` line per word, entries in dict.rs's
//! shape. Messages: `{kind: "begin", source, lang}`, `{kind: "chunk", data}` (answered `{kind: "ack", entries}`),
//! `{kind: "finish"}` (answered `{kind: "done", data, entries}`).
//!
//! - Wiktionary (kaikki.org's extract, Wiktextract's format, <https://github.com/tatuylonen/wiktextract>): one JSON
//!   entry a line, every language; only the language's lines are parsed (`lang_code`).
//! - Open English WordNet (WN-LMF, <https://globalwordnet.github.io/schemas/>): lexical entries (a lemma, a part of
//!   speech, senses in order of frequency) and synsets (a definition and members), read with quick-xml
//!   (<https://docs.rs/quick-xml>); a word's senses carry the definition and the other members as synonyms.

use std::cell::RefCell;
use std::collections::HashMap;

use js_sys::{Array, Uint8Array};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::js::{self, call, f1, get, obj, str};

enum Proc {
    Wiktionary { tags: [String; 2], carry: Vec<u8>, words: HashMap<String, Value> },
    WordNet { xml: Vec<u8> },
}

thread_local! {
    static PROC: RefCell<Option<Proc>> = const { RefCell::new(None) };
}

#[derive(Deserialize)]
struct RawWord {
    word: String,
}

#[derive(Deserialize)]
struct RawSense {
    #[serde(default)]
    glosses: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    synonyms: Vec<RawWord>,
    #[serde(default)]
    form_of: Vec<RawWord>,
}

#[derive(Deserialize)]
struct Raw {
    word: String,
    lang: String,
    pos: String,
    #[serde(default)]
    senses: Vec<RawSense>,
    #[serde(default)]
    synonyms: Vec<RawWord>,
}

/// Wiktextract's part-of-speech codes as Wiktionary's headings.
fn pos_name(p: &str) -> String {
    let named = [("adj", "Adjective"), ("adv", "Adverb"), ("name", "Proper noun"), ("prep", "Preposition"), ("conj", "Conjunction"),
        ("intj", "Interjection"), ("pron", "Pronoun"), ("det", "Determiner"), ("num", "Numeral"), ("abbrev", "Abbreviation"), ("particle", "Particle")];
    named.iter().find(|(k, _)| *k == p).map(|(_, v)| v.to_string()).unwrap_or_else(|| {
        let mut c = p.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    })
}

/// One Wiktextract line into the words, if it is in the language.
fn wiktionary_line(line: &[u8], tags: &[String; 2], words: &mut HashMap<String, Value>) {
    let Ok(line) = std::str::from_utf8(line) else { return };
    if !tags.iter().any(|t| line.contains(t.as_str())) {
        return;
    }
    let Ok(r) = serde_json::from_str::<Raw>(line) else { return };
    let pos = pos_name(&r.pos);
    let e = words.entry(r.word.clone()).or_insert_with(|| json!({ "word": r.word, "lang": r.lang, "senses": [], "synonyms": [], "form_of": [],
        "source": format!("https://en.wiktionary.org/wiki/{}", r.word.replace(' ', "_")), "defined": true }));
    for (i, s) in r.senses.iter().enumerate() {
        let Some(text) = s.glosses.last() else { continue };
        if i == 0 && e["senses"].as_array().is_some_and(Vec::is_empty) {
            e["form_of"] = json!(s.form_of.iter().map(|f| &f.word).collect::<Vec<_>>());
        }
        if let Some(v) = e["senses"].as_array_mut() {
            v.push(json!({ "pos": pos, "labels": s.tags, "text": text, "synonyms": s.synonyms.iter().map(|w| &w.word).collect::<Vec<_>>() }));
        }
    }
    if let Some(v) = e["synonyms"].as_array_mut() {
        v.extend(r.synonyms.iter().map(|w| json!([pos, w.word])));
    }
}

/// Open English WordNet's XML into the words.
fn wordnet(xml: &[u8]) -> Result<HashMap<String, Value>, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    let attr = |e: &quick_xml::events::BytesStart, k: &str| {
        e.try_get_attribute(k).ok().flatten().and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()).map(|v| v.into_owned()).unwrap_or_default()
    };
    // entries: (id, lemma, part of speech, synset ids); synsets: id -> (part of speech, definition, member entry ids)
    let mut entries: Vec<(String, String, String, Vec<String>)> = Vec::new();
    let mut synsets: HashMap<String, (String, String, Vec<String>)> = HashMap::new();
    let (mut synset, mut in_definition, mut buf) = (None::<String>, false, Vec::new());
    loop {
        match reader.read_event_into(&mut buf).map_err(|e| e.to_string())? {
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                "LexicalEntry" => entries.push((attr(&e, "id"), String::new(), String::new(), Vec::new())),
                "Lemma" => {
                    if let Some(last) = entries.last_mut() {
                        (last.1, last.2) = (attr(&e, "writtenForm"), attr(&e, "partOfSpeech"));
                    }
                }
                "Sense" => {
                    if let Some(last) = entries.last_mut() {
                        last.3.push(attr(&e, "synset"));
                    }
                }
                "Synset" => {
                    let id = attr(&e, "id");
                    synsets.insert(id.clone(), (attr(&e, "partOfSpeech"), String::new(), attr(&e, "members").split_whitespace().map(String::from).collect()));
                    synset = Some(id);
                }
                "Definition" => in_definition = synset.as_ref().is_some_and(|s| synsets[s].1.is_empty()), // the first only
                _ => {}
            },
            Event::Text(t) if in_definition => {
                if let Some(s) = synset.as_ref().and_then(|s| synsets.get_mut(s)) {
                    s.1 += &t.xml10_content();
                }
            }
            Event::GeneralRef(r) if in_definition => {
                let c = match &*r {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                };
                if let Some(s) = synset.as_ref().and_then(|s| synsets.get_mut(s)) {
                    s.1 += c;
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "Definition" => in_definition = false,
                "Synset" => synset = None,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    let lemma: HashMap<&str, &str> = entries.iter().map(|(id, l, _, _)| (id.as_str(), l.as_str())).collect();
    let pos_name = |p: &str| match p {
        "n" => "Noun",
        "v" => "Verb",
        "a" | "s" => "Adjective",
        "r" => "Adverb",
        _ => "Other",
    };
    let mut words: HashMap<String, Value> = HashMap::new();
    for (id, word, pos, senses) in &entries {
        let e = words.entry(word.clone()).or_insert_with(|| json!({ "word": word, "lang": "English", "senses": [], "synonyms": [], "form_of": [],
            "source": format!("https://en-word.net/lemma/{}", word.replace(' ', "_")), "defined": true }));
        for s in senses {
            let Some((_, definition, members)) = synsets.get(s) else { continue };
            let synonyms: Vec<&str> = members.iter().filter(|m| *m != id).filter_map(|m| lemma.get(m.as_str()).copied()).collect();
            if let Some(v) = e["senses"].as_array_mut() {
                v.push(json!({ "pos": pos_name(pos), "labels": [], "text": definition.trim(), "synonyms": synonyms }));
            }
        }
    }
    Ok(words)
}

/// The words as the sorted file: one `word \t entry-JSON` line each, by the word's bytes.
fn sorted(words: HashMap<String, Value>) -> Vec<u8> {
    let mut v: Vec<(String, Value)> = words.into_iter().collect();
    v.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut out = Vec::new();
    for (w, e) in v {
        out.extend(w.replace(['\t', '\n'], " ").as_bytes());
        out.push(b'\t');
        out.extend(e.to_string().as_bytes());
        out.push(b'\n');
    }
    out
}

fn post(m: JsValue, transfer: Option<&Uint8Array>) {
    match transfer {
        Some(a) => _ = call!(js::global(), postMessage, m, Array::of1(&a.buffer())),
        None => _ = call!(js::global(), postMessage, m),
    }
}

fn handle(m: &JsValue) -> Result<(), String> {
    match str(&get(m, "kind")).as_str() {
        "begin" => {
            let lang = str(&get(m, "lang"));
            let p = match str(&get(m, "source")).as_str() {
                "wordnet" => Proc::WordNet { xml: Vec::new() },
                _ => Proc::Wiktionary { tags: [format!("\"lang_code\": \"{lang}\""), format!("\"lang_code\":\"{lang}\"")], carry: Vec::new(), words: HashMap::new() },
            };
            PROC.with(|s| *s.borrow_mut() = Some(p));
        }
        "chunk" => {
            let data = Uint8Array::new(&get(m, "data")).to_vec();
            let entries = PROC.with(|s| match s.borrow_mut().as_mut() {
                Some(Proc::Wiktionary { tags, carry, words }) => {
                    carry.extend_from_slice(&data);
                    let mut start = 0;
                    while let Some(i) = carry[start..].iter().position(|&b| b == b'\n') {
                        wiktionary_line(&carry[start..start + i], tags, words);
                        start += i + 1;
                    }
                    carry.drain(..start);
                    words.len()
                }
                Some(Proc::WordNet { xml }) => {
                    xml.extend_from_slice(&data);
                    0
                }
                None => 0,
            });
            post(obj! { kind: "ack", entries: entries as f64 }, None);
        }
        "finish" => {
            let words = match PROC.with(|s| s.borrow_mut().take()) {
                Some(Proc::Wiktionary { tags, carry, mut words }) => {
                    wiktionary_line(&carry, &tags, &mut words);
                    words
                }
                Some(Proc::WordNet { xml }) => wordnet(&xml)?,
                None => HashMap::new(),
            };
            let n = words.len();
            let data = Uint8Array::from(&sorted(words)[..]);
            post(obj! { kind: "done", data: data.clone(), entries: n as f64 }, Some(&data));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiktionary_keeps_one_language_merged_by_word_and_sorted() {
        let lines = [
            json!({ "word": "happy", "lang": "English", "lang_code": "en", "pos": "adj", "senses": [{ "glosses": ["Feeling joy."], "tags": ["informal"], "synonyms": [{ "word": "glad" }] }] }),
            json!({ "word": "happy", "lang": "English", "lang_code": "en", "pos": "verb", "senses": [{ "glosses": ["To make happy."] }], "synonyms": [{ "word": "cheer" }] }),
            json!({ "word": "running", "lang": "English", "lang_code": "en", "pos": "verb", "senses": [{ "glosses": ["present participle of run"], "form_of": [{ "word": "run" }] }] }),
            json!({ "word": "heureux", "lang": "French", "lang_code": "fr", "pos": "adj", "senses": [{ "glosses": ["happy"] }] }),
        ];
        let tags = ["\"lang_code\": \"en\"".to_string(), "\"lang_code\":\"en\"".to_string()];
        let mut words = HashMap::new();
        for l in &lines {
            wiktionary_line(l.to_string().as_bytes(), &tags, &mut words);
        }
        assert_eq!(words.len(), 2, "only the language asked for");
        assert_eq!(words["happy"]["senses"][0], json!({ "pos": "Adjective", "labels": ["informal"], "text": "Feeling joy.", "synonyms": ["glad"] }));
        assert_eq!(words["happy"]["synonyms"], json!([["Verb", "cheer"]]));
        assert_eq!(words["running"]["form_of"], json!(["run"]));
        let file = String::from_utf8(sorted(words)).unwrap();
        assert!(file.starts_with("happy\t{") && file.contains("\nrunning\t{"), "{file}");
        let e: crate::dict::Entry = serde_json::from_str(file.lines().next().unwrap().split_once('\t').unwrap().1).unwrap();
        assert!(e.defined && e.senses.len() == 2, "the plugin reads it as an entry");
    }

    #[test]
    fn wordnet_senses_carry_definitions_and_synonyms() {
        let xml = br#"<?xml version="1.0"?><LexicalResource><Lexicon id="oewn">
            <LexicalEntry id="oewn-calibration-n"><Lemma writtenForm="calibration" partOfSpeech="n"/><Sense id="s1" synset="oewn-1-n"/></LexicalEntry>
            <LexicalEntry id="oewn-standardization-n"><Lemma writtenForm="standardization" partOfSpeech="n"/><Sense id="s2" synset="oewn-1-n"/></LexicalEntry>
            <Synset id="oewn-1-n" partOfSpeech="n" members="oewn-calibration-n oewn-standardization-n"><Definition>the act of checking &amp; adjusting a measuring instrument</Definition></Synset>
            </Lexicon></LexicalResource>"#;
        let words = wordnet(xml).unwrap();
        assert_eq!(words["calibration"]["senses"][0], json!({ "pos": "Noun", "labels": [], "text": "the act of checking & adjusting a measuring instrument", "synonyms": ["standardization"] }));
        assert_eq!(words["standardization"]["senses"][0]["synonyms"], json!(["calibration"]));
    }
}

/// In the worker: take the messages above.
#[wasm_bindgen]
pub fn worker_start() {
    crate::js::set(&js::global(), "onmessage", f1(|e| {
        if let Err(message) = handle(&get(&e, "data")) {
            post(obj! { kind: "error", message: message }, None);
        }
        JsValue::UNDEFINED
    }));
}
