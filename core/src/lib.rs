//! Dictionary and Synonyms: an Obsidian plugin in Rust. main.js (the only JavaScript, a few lines) loads this module
//! and calls `start` on Obsidian's main thread (app); select a word for its synonyms, Ctrl+select for its definitions
//! (lookup, from dict's sources); whole dictionaries can be downloaded for use offline (files), prepared in a Web
//! Worker running this same module (worker).

pub use obsidian_kit::js; // shared with the other plugins (../../shared)

mod app;
mod dict;
mod files;
mod lookup;
mod worker;
