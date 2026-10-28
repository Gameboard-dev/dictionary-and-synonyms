//! The dictionary: a word's senses (part of speech, labels, definition, synonyms) from Wiktionary, read from a page's
//! wikitext as the MediaWiki Action API returns it (<https://www.mediawiki.org/wiki/API:Revisions>), and the note that
//! caches it in the vault. English Wiktionary describes words of every language in English, each language in its own
//! section, so another language is a row in `LANGUAGES`. Wiktionary's conventions: entry layout
//! (<https://en.wiktionary.org/wiki/Wiktionary:Entry_layout>), labels (`{{lb}}`), links (`{{l}}`), synonyms (`{{syn}}`,
//! Synonyms sections) and form-of templates (`{{plural of}}`, `{{inflection of}}`, …). Text is CC BY-SA 4.0.

use serde::{Deserialize, Serialize};

/// A language: its code and name, its section on English Wiktionary, the labels of its own variety and of others
/// (senses labelled with another variety, US for British English, are listed after the rest and keep their label), and
/// Datamuse's vocabulary for synonyms (`Some("")` English, `Some("es")` Spanish; `None`: Wiktionary's synonyms instead).
pub struct Lang {
    pub code: &'static str,
    pub name: &'static str,
    pub section: &'static str,
    pub own: &'static [&'static str],
    pub others: &'static [&'static str],
    pub datamuse: Option<&'static str>,
}

impl Lang {
    /// The language's Wikipedia edition ("en", "fr").
    pub fn wiki(&self) -> &'static str {
        self.code.split('-').next().unwrap_or("en")
    }
}

const fn lang(code: &'static str, name: &'static str, section: &'static str, datamuse: Option<&'static str>) -> Lang {
    Lang { code, name, section, own: &[], others: &[], datamuse }
}

pub const LANGUAGES: &[Lang] = &[
    Lang { code: "en-GB", name: "English (British)", section: "English", own: &["UK", "British", "Commonwealth"], others: &["US", "American", "Canada", "Australia"], datamuse: Some("") },
    Lang { code: "en-US", name: "English (American)", section: "English", own: &["US", "American"], others: &["UK", "British", "Commonwealth", "Australia"], datamuse: Some("") },
    lang("es", "Spanish", "Spanish", Some("es")),
    lang("fr", "French", "French", None),
    lang("de", "German", "German", None),
    lang("it", "Italian", "Italian", None),
    lang("pt", "Portuguese", "Portuguese", None),
    lang("nl", "Dutch", "Dutch", None),
];

pub fn language(code: &str) -> &'static Lang {
    LANGUAGES.iter().find(|l| l.code == code).unwrap_or(&LANGUAGES[0])
}

const POS: &[&str] = &["Noun", "Verb", "Adjective", "Adverb", "Pronoun", "Preposition", "Postposition", "Conjunction", "Interjection",
    "Proper noun", "Determiner", "Article", "Numeral", "Particle", "Phrase", "Prepositional phrase", "Idiom", "Proverb", "Prefix",
    "Suffix", "Affix", "Infix", "Abbreviation", "Initialism", "Acronym", "Symbol", "Letter", "Contraction", "Participle"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sense {
    pub pos: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub text: String,
    #[serde(default)]
    pub synonyms: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub word: String,
    pub lang: String,
    pub senses: Vec<Sense>,
    /// Synonyms listed for a part of speech as a whole (a Synonyms section): (part of speech, word).
    #[serde(default)]
    pub synonyms: Vec<(String, String)>,
    /// The words this one is a form of ("run" for "running"), whose senses follow its own.
    #[serde(default)]
    pub form_of: Vec<String>,
    #[serde(default)]
    pub lemma: Option<String>,
    pub source: String,
    /// Whether the senses were looked up (Wiktionary, or Datamuse's definitions when Wiktionary could not answer).
    #[serde(default)]
    pub defined: bool,
    /// Datamuse's synonyms, best first, each with its parts of speech; `None` until asked.
    #[serde(default)]
    pub thesaurus: Option<Vec<(String, Vec<String>)>>,
    /// The encyclopedia's summary, for a term the dictionary lacks ("network intrusion detection").
    #[serde(default)]
    pub summary: Option<Summary>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub title: String,
    pub text: String,
    pub url: String,
}

impl Entry {
    /// Every synonym once, in order, with the parts of speech it was given for: Datamuse's when it was asked,
    /// otherwise those Wiktionary's senses and Synonyms sections give.
    pub fn all_synonyms(&self) -> Vec<(String, Vec<String>)> {
        if let Some(t) = self.thesaurus.as_ref().filter(|t| !t.is_empty()) {
            return t.clone();
        }
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        let pairs = self.senses.iter().flat_map(|s| s.synonyms.iter().map(move |w| (&s.pos, w))).chain(self.synonyms.iter().map(|(p, w)| (p, w)));
        for (pos, w) in pairs {
            if w.eq_ignore_ascii_case(&self.word) {
                continue;
            }
            match out.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(w)) {
                Some((_, ps)) if !ps.contains(pos) => ps.push(pos.clone()),
                Some(_) => {}
                None => out.push((w.clone(), vec![pos.clone()])),
            }
        }
        out
    }

    /// Senses of this variety first, others' after (they keep their label); otherwise Wiktionary's order.
    pub fn order_for(&mut self, code: &str) {
        let l = language(code);
        let other = |s: &Sense| s.labels.iter().any(|x| l.others.iter().any(|o| x.contains(o))) && !s.labels.iter().any(|x| l.own.iter().any(|m| x.contains(m)));
        self.senses.sort_by_key(|s| other(s));
    }
}

/// A template's or link's parts, split on `|` outside nested `{{ }}` and `[[ ]]`.
fn split_args(inner: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match (b[i], b.get(i + 1)) {
            (b'{', Some(b'{')) | (b'[', Some(b'[')) => {
                depth += 1;
                i += 1;
            }
            (b'}', Some(b'}')) | (b']', Some(b']')) => {
                depth -= 1;
                i += 1;
            }
            (b'|', _) if depth == 0 => {
                out.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&inner[start..]);
    out
}

/// The end of the `{{ … }}` or `[[ … ]]` opening at `i` (the index after it).
fn matching(s: &str, i: usize, open: &[u8; 2], close: &[u8; 2]) -> Option<usize> {
    let b = s.as_bytes();
    let (mut depth, mut j) = (0, i);
    while j + 1 < b.len() {
        if b[j] == open[0] && b[j + 1] == open[1] {
            depth += 1;
            j += 2;
        } else if b[j] == close[0] && b[j + 1] == close[1] {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return Some(j);
            }
        } else {
            j += 1;
        }
    }
    None
}

#[derive(Default)]
struct Cleaned {
    text: String,
    labels: Vec<String>,
    form_of: Option<String>,
    synonyms: Vec<String>,
}

/// Positional arguments of a template (named ones, `a=b`, left out), inline modifiers (`word<qq:…>`) removed.
fn positional<'a>(args: &[&'a str]) -> Vec<&'a str> {
    args.iter().filter(|a| !a.contains('=')).map(|a| a.split('<').next().unwrap_or(a).trim()).collect()
}

/// Wikitext to plain text, keeping what templates mean: labels apart, links' words, glosses in brackets, form-of
/// templates as "plural of run" (and the lemma), synonym templates as synonyms.
fn clean(s: &str, lang: &str) -> Cleaned {
    let mut c = Cleaned::default();
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if rest.starts_with("{{") {
            let Some(end) = matching(s, i, b"{{", b"}}") else { break };
            let args = split_args(&s[i + 2..end - 2]);
            let name = args[0].trim();
            let pos = positional(&args[1..]);
            let after_lang = |k: usize| pos.get(k + 1).copied().unwrap_or("");
            match name {
                "lb" | "lbl" | "label" | "tlb" | "term-label" => c.labels.extend(pos.iter().skip(1).filter(|l| !["_", "and", "or", ""].contains(l)).map(|l| l.replace('_', " "))),
                "l" | "m" | "link" | "mention" | "l-self" | "ll" => out += if !after_lang(1).is_empty() { after_lang(1) } else { after_lang(0) },
                "gloss" | "gl" => out += &format!("({})", clean(pos.first().copied().unwrap_or(""), lang).text),
                "q" | "qual" | "qualifier" | "i" | "qf" | "sense" | "s" => out += &format!("({})", pos.join(", ")),
                "w" | "wikipedia" | "pedia" => out += pos.get(1).or(pos.first()).copied().unwrap_or(""),
                "taxlink" | "taxfmt" | "vern" | "non-gloss definition" | "non-gloss" | "n-g" | "ngd" | "def" => out += &clean(pos.first().copied().unwrap_or(""), lang).text,
                "syn" | "synonyms" | "syn-lite" => c.synonyms.extend(pos.iter().skip(1).filter(|w| !w.is_empty() && !w.starts_with("Thesaurus:")).map(|w| w.to_string())),
                n if n.ends_with(" of") || n.ends_with(" form of") => {
                    // {{plural of|en|run}}; {{en-past of|run}} (the language in the name)
                    let lemma = if n.starts_with(&format!("{lang}-")) { pos.first().copied() } else { pos.get(1).copied() }.unwrap_or("");
                    let shown = n.strip_prefix(&format!("{lang}-")).unwrap_or(n);
                    let shown = if shown == "infl of" { "inflection of" } else { shown };
                    out += &format!("{shown} {lemma}");
                    if c.form_of.is_none() && !lemma.is_empty() {
                        c.form_of = Some(lemma.to_string());
                    }
                }
                _ => {}
            }
            i = end;
        } else if rest.starts_with("[[") {
            let Some(end) = matching(s, i, b"[[", b"]]") else { break };
            let parts = split_args(&s[i + 2..end - 2]);
            let target = parts[0];
            if !target.contains(':') || target.starts_with("w:") {
                let shown = parts.last().copied().unwrap_or(target);
                out += shown.split('#').next().unwrap_or(shown);
            }
            i = end;
        } else if rest.starts_with("<ref") {
            i += rest.find("</ref>").map(|e| e + 6).or_else(|| rest.find("/>").map(|e| e + 2)).unwrap_or(rest.len());
        } else if rest.starts_with('<') && rest.find('>').is_some() {
            i += rest.find('>').unwrap() + 1;
        } else if rest.starts_with("'''") || rest.starts_with("''") {
            i += if rest.starts_with("'''") { 3 } else { 2 };
        } else {
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    let text = out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ").replace(" ,", ",").replace(" .", ".").replace("( ", "(");
    c.text = text.trim_start_matches([';', ':', ',', ' ']).trim().to_string(); // what a dropped template led into
    c
}

/// The words a Synonyms section's line lists (`* {{l|en|glad}}, [[content]]`, `{{col|en|a|b}}`).
fn listed(line: &str, lang: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < line.len() {
        if line[i..].starts_with("{{") {
            let Some(end) = matching(line, i, b"{{", b"}}") else { break };
            let args = split_args(&line[i + 2..end - 2]);
            let pos = positional(&args[1..]);
            match args[0].trim() {
                "l" | "link" | "l-self" => out.extend(pos.get(1).map(|w| w.to_string())),
                n if n.starts_with("col") || n == "syn" => out.extend(pos.iter().skip(1).filter(|w| !w.is_empty() && !w.starts_with("Thesaurus:")).map(|w| w.to_string())),
                _ => {}
            }
            i = end;
        } else if line[i..].starts_with("[[") {
            let Some(end) = matching(line, i, b"[[", b"]]") else { break };
            let target = split_args(&line[i + 2..end - 2])[0];
            if !target.contains(':') {
                out.push(target.to_string());
            }
            i = end;
        } else {
            i += line[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    let _ = lang;
    out
}

/// A page's entry for one language: its senses by part of speech, with labels and synonyms. `None` if the page has
/// no section for the language.
pub fn parse(title: &str, wikitext: &str, section: &str, lang_code: &str) -> Option<Entry> {
    let lang = lang_code.split('-').next().unwrap_or(lang_code);
    let heading = |l: &str| -> Option<(usize, String)> {
        let t = l.trim();
        let n = t.chars().take_while(|&c| c == '=').count();
        (n >= 2 && t.ends_with('=')).then(|| (n, t.trim_matches('=').trim().to_string()))
    };
    let mut lines = wikitext.lines();
    lines.find(|l| heading(l).is_some_and(|(n, h)| n == 2 && h == section))?;
    let mut e = Entry { word: title.to_string(), lang: section.to_string(), source: format!("https://en.wiktionary.org/wiki/{}", title.replace(' ', "_")), defined: true, ..Default::default() };
    let (mut pos, mut in_synonyms) = (None::<String>, false);
    for line in lines {
        if let Some((n, h)) = heading(line) {
            if n <= 2 {
                break; // the next language
            }
            if POS.contains(&h.as_str()) {
                (pos, in_synonyms) = (Some(h), false);
            } else {
                in_synonyms = h == "Synonyms";
                if !in_synonyms && n <= 3 {
                    pos = None; // Etymology, Pronunciation …
                }
            }
            continue;
        }
        let Some(p) = &pos else { continue };
        if in_synonyms {
            if line.starts_with('*') {
                e.synonyms.extend(listed(line, lang).into_iter().map(|w| (p.clone(), w)));
            }
        } else if let Some(def) = line.strip_prefix('#').map(|r| r.trim_start_matches('#')).filter(|r| !r.starts_with([':', '*'])) {
            let c = clean(def.trim(), lang); // "#" and "##" are senses; "#:", "##:", "#*" … examples and quotations
            if !c.text.chars().any(char::is_alphanumeric) && c.form_of.is_none() {
                continue;
            }
            if e.senses.is_empty() {
                e.form_of.extend(c.form_of); // a form ("running") when its first sense says so; its lemma follows
            }
            e.senses.push(Sense { pos: p.clone(), labels: c.labels, text: c.text, synonyms: c.synonyms });
        } else if line.starts_with("#:") {
            let c = clean(line.trim_start_matches(['#', ':']), lang);
            if let Some(last) = e.senses.last_mut() {
                last.synonyms.extend(c.synonyms); // "#: {{syn|en|glad|content}}" under its sense
            }
        }
    }
    Some(e)
}

/// The file name a word's note gets (characters a file name cannot hold replaced).
pub fn file_name(word: &str) -> String {
    word.chars().map(|c| if "\\/:*?\"<>|#^[]".contains(c) { '_' } else { c }).collect::<String>().trim().to_string()
}

/// Where each part of an entry came from, for the note's properties: (source, licence or terms).
fn sources(e: &Entry) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    if e.defined && !e.source.is_empty() {
        let terms = if e.source.contains("wiktionary") { "CC BY-SA 4.0" } else if e.source.contains("en-word.net") { "CC BY 4.0" } else { "see the source" };
        out.push((e.source.clone(), terms));
    }
    if let Some(s) = &e.summary {
        out.push((s.url.clone(), "CC BY-SA 4.0"));
    }
    if e.thesaurus.is_some() {
        out.push(("https://www.datamuse.com/api/".into(), "free API, synonyms"));
    }
    out
}

/// A part of speech as a property name's ending: "Proper noun" → "proper-noun".
fn slug(pos: &str) -> String {
    pos.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("-")
}

/// A property name's ending as a part of speech: "proper-noun" → "Proper noun".
fn unslug(s: &str) -> String {
    let t = s.replace('-', " ");
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// A string as a YAML scalar Obsidian reads (JSON's quoting is YAML's).
fn q(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// The note that caches an entry (user, 2026-10-28): properties only, each one Obsidian shows on its own: the word,
/// its language and lemma, its definitions and synonyms by part of speech (`definitions-noun`, `synonyms-verb` …;
/// a definition's labels in brackets before it), the encyclopedia's summary, what was looked up, the sources and the
/// date. Nothing is repeated below them, and the note is read back from these same properties.
pub fn note(e: &Entry, date: &str) -> String {
    let mut md = String::from("---\n");
    md += &format!("word: {}\nlanguage: {}\n", q(&e.word), q(&e.lang));
    if let Some(l) = &e.lemma {
        md += &format!("lemma: {}\n", q(l));
    }
    let list = |md: &mut String, name: &str, items: &[String]| {
        if !items.is_empty() {
            *md += &format!("{name}:\n");
            for i in items {
                *md += &format!("  - {}\n", q(i));
            }
        }
    };
    list(&mut md, "form-of", &e.form_of);
    // definitions, by part of speech in the order they come
    let mut parts: Vec<(String, Vec<String>)> = Vec::new();
    for sense in &e.senses {
        let text = if sense.labels.is_empty() { sense.text.clone() } else { format!("({}) {}", sense.labels.join(", "), sense.text) };
        match parts.iter_mut().find(|(p, _)| *p == sense.pos) {
            Some((_, v)) => v.push(text),
            None => parts.push((sense.pos.clone(), vec![text])),
        }
    }
    for (pos, defs) in &parts {
        list(&mut md, &format!("definitions-{}", slug(pos)), defs);
    }
    // synonyms, by part of speech (a synonym given for two is in both); those without one under `synonyms`
    let mut syn: Vec<(String, Vec<String>)> = Vec::new();
    for (w, poses) in e.all_synonyms() {
        let poses = if poses.is_empty() { vec![String::new()] } else { poses };
        for pos in poses {
            match syn.iter_mut().find(|(p, _)| *p == pos) {
                Some((_, v)) => v.push(w.clone()),
                None => syn.push((pos, vec![w.clone()])),
            }
        }
    }
    for (pos, words) in &syn {
        list(&mut md, &if pos.is_empty() { "synonyms".to_string() } else { format!("synonyms-{}", slug(pos)) }, words);
    }
    if let Some(s) = &e.summary {
        md += &format!("summary: {}\nsummary-title: {}\nsummary-url: {}\n", q(&s.text), q(&s.title), q(&s.url));
    }
    let mut looked: Vec<String> = Vec::new();
    if e.defined {
        looked.push("definitions".into());
    }
    if e.thesaurus.is_some() {
        looked.push("synonyms".into());
    }
    list(&mut md, "looked-up", &looked);
    if !e.source.is_empty() {
        md += &format!("dictionary: {}\n", q(&e.source));
    }
    let sources: Vec<String> = sources(e).into_iter().map(|(s, terms)| format!("{s} ({terms})")).collect();
    list(&mut md, "sources", &sources);
    md += &format!("retrieved: {date}\n---\n");
    md
}

/// The properties of a note: each name with its value or list items (strings as written, quoted or not).
fn properties(md: &str) -> Vec<(String, Vec<String>)> {
    let md = md.replace("\r\n", "\n");
    let mut lines = md.lines();
    if lines.next() != Some("---") {
        return Vec::new();
    }
    let unq = |v: &str| {
        let v = v.trim();
        if v.starts_with('"') {
            serde_json::from_str::<String>(v).unwrap_or_else(|_| v.trim_matches('"').to_string())
        } else {
            v.trim_matches('\'').to_string()
        }
    };
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for l in lines.take_while(|l| *l != "---") {
        if let Some(item) = l.trim_start().strip_prefix("- ").filter(|_| l.starts_with(' ')) {
            if let Some((_, v)) = out.last_mut() {
                v.push(unq(item));
            }
        } else if let Some((k, v)) = l.split_once(':') {
            let v = v.trim();
            out.push((k.trim().to_string(), if v.is_empty() { Vec::new() } else { vec![unq(v)] }));
        }
    }
    out
}

/// The entry kept in a note: its properties (since 2026-10-28), or, in a note of the old form, its `dict-entry`
/// property (`av-entry` in notes written while the dictionary was part of Augmented View).
pub fn from_note(md: &str) -> Option<Entry> {
    if let Some(line) = md.lines().skip(1).take_while(|l| l.trim_end() != "---").find_map(|l| l.strip_prefix("dict-entry: ").or_else(|| l.strip_prefix("av-entry: "))) {
        return serde_json::from_str(line.trim_end()).ok();
    }
    let props = properties(md);
    let one = |k: &str| props.iter().find(|(n, _)| n == k).and_then(|(_, v)| v.first().cloned());
    let word = one("word")?;
    let mut e = Entry { word, lang: one("language").unwrap_or_default(), lemma: one("lemma"), source: one("dictionary").unwrap_or_default(), ..Default::default() };
    let looked: Vec<String> = props.iter().find(|(n, _)| n == "looked-up").map(|(_, v)| v.clone()).unwrap_or_default();
    e.defined = looked.iter().any(|l| l == "definitions");
    let mut thesaurus: Vec<(String, Vec<String>)> = Vec::new();
    for (name, items) in &props {
        if name == "form-of" {
            e.form_of = items.clone();
        } else if let Some(pos) = name.strip_prefix("definitions-") {
            for d in items {
                let (labels, text) = match d.strip_prefix('(').and_then(|r| r.split_once(") ")) {
                    Some((l, t)) => (l.split(", ").map(str::to_string).collect(), t.to_string()),
                    None => (Vec::new(), d.clone()),
                };
                e.senses.push(Sense { pos: unslug(pos), labels, text, synonyms: Vec::new() });
            }
        } else if name == "synonyms" || name.starts_with("synonyms-") {
            let pos = name.strip_prefix("synonyms-").map(unslug);
            for w in items {
                match thesaurus.iter_mut().find(|(x, _)| x == w) {
                    Some((_, p)) => p.extend(pos.clone()),
                    None => thesaurus.push((w.clone(), pos.clone().into_iter().collect())),
                }
            }
        }
    }
    if let Some(text) = one("summary") {
        e.summary = Some(Summary { title: one("summary-title").unwrap_or_default(), text, url: one("summary-url").unwrap_or_default() });
    }
    // Datamuse's list when it was asked; otherwise the synonyms are the dictionary's, by part of speech
    if looked.iter().any(|l| l == "synonyms") {
        e.thesaurus = Some(thesaurus);
    } else {
        e.synonyms = thesaurus.into_iter().flat_map(|(w, ps)| ps.into_iter().map(move |p| (p, w.clone()))).collect();
    }
    Some(e)
}

/// Whether a note is of the form before 2026-10-28 (the entry as one JSON property, its content repeated below).
pub fn old_form(md: &str) -> bool {
    md.lines().skip(1).take_while(|l| l.trim_end() != "---").any(|l| l.starts_with("dict-entry: ") || l.starts_with("av-entry: "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_is_properties_and_reads_back() {
        let e = Entry {
            word: "attack".into(),
            lang: "English".into(),
            senses: vec![
                Sense { pos: "Noun".into(), labels: vec!["military".into()], text: "An attempt to cause damage.".into(), synonyms: vec![] },
                Sense { pos: "Verb".into(), labels: vec![], text: "To apply violent force.".into(), synonyms: vec![] },
            ],
            source: "https://en.wiktionary.org/wiki/attack".into(),
            defined: true,
            thesaurus: Some(vec![("onset".into(), vec!["Noun".into(), "Verb".into()]), ("assail".into(), vec!["Verb".into()])]),
            ..Default::default()
        };
        let md = note(&e, "2026-10-28");
        assert!(md.ends_with("retrieved: 2026-10-28\n---\n"), "{md}");
        assert!(!md.contains("dict-entry") && !md.contains("# attack"), "{md}");
        assert!(md.contains("definitions-noun:\n  - \"(military) An attempt to cause damage.\"\n"), "{md}");
        assert!(md.contains("synonyms-verb:\n  - \"onset\"\n  - \"assail\"\n"), "{md}");
        let back = from_note(&md).unwrap();
        assert_eq!(back.senses, e.senses);
        assert_eq!(back.thesaurus, e.thesaurus);
        assert!(back.defined && back.word == "attack" && back.source == e.source);
        // Obsidian's own way of writing a list (unquoted) reads too
        let edited = md.replace("  - \"assail\"", "  - assail");
        assert_eq!(from_note(&edited).unwrap().thesaurus, e.thesaurus);
    }

    const PAGE: &str = "==English==\n===Etymology===\nFrom Old English.\n\n===Adjective===\n{{en-adj}}\n\n# {{lb|en|of a person}} Having a feeling arising from a [[consciousness]] of well-being or of [[enjoyment]].\n#: {{syn|en|glad|content<qq:informal>|Thesaurus:happy}}\n#* {{quote-book|en|year=1900|passage=I am happy.}}\n# {{lb|en|US|informal}} [[willing|Willing]].\n## Content to do something.\n\n====Synonyms====\n* {{l|en|cheerful}}, [[merry]]\n\n===Verb===\n# {{lb|en|UK}} To make [[happy]].\n\n==French==\n===Adjective===\n# [[heureux|Happy]].\n";

    #[test]
    fn senses_labels_and_synonyms_of_one_language() {
        let e = parse("happy", PAGE, "English", "en-GB").unwrap();
        let s: Vec<(&str, &str)> = e.senses.iter().map(|s| (s.pos.as_str(), s.text.as_str())).collect();
        assert_eq!(s, [("Adjective", "Having a feeling arising from a consciousness of well-being or of enjoyment."), ("Adjective", "Willing."),
            ("Adjective", "Content to do something."), ("Verb", "To make happy.")]);
        assert_eq!(e.senses[0].labels, ["of a person"]);
        assert_eq!(e.senses[0].synonyms, ["glad", "content"]);
        assert_eq!(e.all_synonyms().iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>(), ["glad", "content", "cheerful", "merry"]);
        assert!(parse("happy", PAGE, "German", "de").is_none());
    }

    #[test]
    fn varieties_ordered_and_forms_found() {
        let mut e = parse("happy", PAGE, "English", "en-GB").unwrap();
        e.order_for("en-GB");
        assert_eq!(e.senses.last().unwrap().text, "Willing.", "the US sense goes last for British English");
        let f = parse("running", "==English==\n===Verb===\n# {{present participle of|en|run}}\n", "English", "en-US").unwrap();
        assert_eq!((f.senses[0].text.as_str(), f.form_of.as_slice()), ("present participle of run", &["run".to_string()][..]));
    }

    /// Prints what saved pages give (AV_WIKT=<folder of word.wiki files>): `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn show_pages() {
        let Ok(dir) = std::env::var("AV_WIKT") else { return };
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let word = f.path().file_stem().unwrap().to_string_lossy().replace('_', " ");
            let text = std::fs::read_to_string(f.path()).unwrap();
            let Some(e) = parse(&word, text.trim_start_matches('\u{feff}'), "English", "en-GB") else { continue };
            println!("== {word}: {} senses, form of {:?}", e.senses.len(), e.form_of);
            for s in e.senses.iter().take(6) {
                println!("  {} {:?} {} {:?}", s.pos, s.labels, s.text, s.synonyms);
            }
            println!("  synonyms: {:?}", e.all_synonyms().iter().take(12).map(|(w, _)| w.as_str()).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_note_keeps_its_entry() {
        let e = parse("happy", PAGE, "English", "en-GB").unwrap();
        let md = note(&e, "2026-10-24");
        assert!(md.contains("\ndefinitions-adjective:\n  - \"(of a person) Having a feeling"), "{md}");
        // read back: the senses as they were; a sense's own synonyms are kept with the part of speech's
        let back = from_note(&md).unwrap();
        let plain = |e: &Entry| e.senses.iter().map(|s| (s.pos.clone(), s.labels.clone(), s.text.clone())).collect::<Vec<_>>();
        assert_eq!(plain(&back), plain(&e));
        let words = |e: &Entry| {
            let mut w: Vec<String> = e.all_synonyms().into_iter().map(|(w, _)| w).collect();
            w.sort();
            w
        };
        assert_eq!(words(&back), words(&e));
        assert_eq!((back.word, back.lemma, back.form_of, back.defined), (e.word.clone(), e.lemma.clone(), e.form_of.clone(), e.defined));
        assert_eq!(file_name("and/or: x?"), "and_or_ x_");
    }
}
