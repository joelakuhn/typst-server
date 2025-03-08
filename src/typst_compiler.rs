#![cfg_attr(windows, feature(abi_vectorcall))]
use std::collections::HashMap;
use std::path::Path;
use std::fs;

use typst::ecow::EcoVec;
use typst::Library;
use typst::diag::{ FileError, FileResult, SourceDiagnostic };
use typst::syntax::{ FileId, Source, Span, VirtualPath };
use typst::text::{ Font, FontBook };
use typst::World;
use typst::foundations::{Binding, Datetime, Value, Bytes};

use typst::utils::LazyHash;

mod fonts;
use fonts::{FontSearcher, FontSlot};

use typst_pdf::PdfOptions;

// WORLD

struct TypstServerWorld {
    library: LazyHash<Library>,
    main: Source,
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
}

impl TypstServerWorld {
    fn new(builder: &Typst) -> Self {
        let mut fontsearcher = FontSearcher::new();
        fontsearcher.search_system();

        for font_path in &builder.fonts {
            let path = Path::new(&font_path);
            if path.is_dir() { fontsearcher.search_dir(&path); }
            else if path.is_file() { fontsearcher.search_file(&path); }
        }

        let body = match builder.body.as_ref() {
            Some(body) => body,
            None => "",
        };

        let file_id = FileId::new(None, VirtualPath::new("./::http_source::"));

        Self {
            library: LazyHash::new(make_library(builder)),
            main: Source::new(file_id, body.to_owned()),
            book: LazyHash::new(fontsearcher.book),
            fonts: fontsearcher.fonts,
        }
    }
}

impl World for TypstServerWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, _id: FileId) -> FileResult<Source> {
        Ok(self.main.clone())
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn font(&self, id: usize) -> Option<Font> {
        let slot = &self.fonts[id];
        let data = read(&slot.path).unwrap();
        let bytes : Bytes = Bytes::new(data);
        Font::new(bytes, slot.index)
    }

    fn file(&self, path: FileId) -> FileResult<Bytes> {
        // if path.components().any(|c| c.as_os_str() == "..") {
        //     Err(FileError::AccessDenied)
        // }
        // else if !path.is_relative() {
        //     Err(FileError::AccessDenied)
        // }
        // else {

            let data = read(path.vpath().as_rooted_path()).unwrap();
            let bytes : Bytes = Bytes::new(data);
            Ok(bytes)
        // }
    }

    fn today(&self, _offset:Option<i64>) -> Option<Datetime> {
        Some(Datetime::from_ymd(1970, 1, 1).unwrap())
    }
}

// HELPERS

fn make_library(builder: &Typst) -> Library {
    let mut lib = Library::builder().build();
    let scope = lib.global.scope_mut();

    for (k, v) in builder.json.to_owned().into_iter() {
        let serde_value: Result<serde_json::Value, _> = serde_json::from_slice(v.as_bytes());
        if serde_value.is_ok() {
            let typst_val = json_to_typst(serde_value.unwrap());
            scope.bind(k.into(), Binding::new(typst_val, Span::detached()));
        }
    }

    for (k, v) in builder.vars.to_owned().into_iter() {
        scope.bind(k.into(), Binding::new(v, Span::detached()));
    }

    return lib;
}

fn read(path: &Path) -> FileResult<Vec<u8>> {
    let f = |e| FileError::from_io(e, path);
    if fs::metadata(path).map_err(f)?.is_dir() {
        Err(FileError::IsDirectory)
    } else {
        fs::read(path).map_err(f)
    }
}

// CONVERTERS

fn json_to_typst(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(v) => Value::Bool(v),
        serde_json::Value::Number(v) => match v.as_i64() {
            Some(int) => Value::Int(int),
            None => Value::Float(v.as_f64().unwrap_or(f64::NAN)),
        },
        serde_json::Value::String(v) => Value::Str(v.into()),
        serde_json::Value::Array(v) => {
            Value::Array(v.into_iter().map(json_to_typst).collect())
        }
        serde_json::Value::Object(v) => Value::Dict(
            v.into_iter()
                .map(|(key, value)| (key.into(), json_to_typst(value)))
                .collect(),
        ),
    }
}

// DIAGNOSTICS

fn get_error_message(_world: &TypstServerWorld, body: &str, errors: &EcoVec<SourceDiagnostic>) -> String {
    let mut full_message = String::from("");
    let mut first = true;
    for error in errors {
        if first { first = false }
        else { full_message.push_str("\n"); }

        full_message.push_str(&String::from(error.message.to_owned()));

        let range = error.span.range();
        if range.is_some() {
            let range = range.unwrap();
            let body_bytes = body.as_bytes();

            let mut line_number = 1;
            for b in body_bytes[0..range.start].iter() {
                if *b == 0x0A {
                    line_number += 1
                }
            }

            full_message.push_str(&format!("Typst error on line {}: ", line_number));

            let mut start = range.start;
            let mut end = range.end;
            if start > 0 && body_bytes[start] == 0x0A {
                start -= 1
            }
            while body_bytes[start] != 0x0A {
                if start == 0 { break; }
                start -= 1;
            }
            if start == 0x0A { start += 1 }
            if end < body_bytes.len() && body_bytes[end] == 0x0A {
                end += 1;
            }
            while end < body_bytes.len() && body_bytes[end] != 0x0A {
                end += 1;
            }
            if end == 0x0A { end -= 1 }


            match String::from_utf8(body_bytes[start..end].into()) {
                Ok(code) => {
                    full_message.push_str("\n");
                    full_message.push_str(&code);

                }
                _ => {},
            }
        }
    }
    return full_message;
}

pub struct Typst {
    body: Option<String>,
    json: HashMap<String, String>,
    vars: HashMap<String, Value>,
    fonts: Vec<String>,
}

impl Typst {
    pub fn new(body: Option<String>) -> Self {
        Self {
            body: body,
            json: HashMap::new(),
            vars: HashMap::new(),
            fonts: vec![],
        }
    }

    pub fn json(&mut self, key: String, value: String) {
        self.json.insert(key, value);
    }

    pub fn compile(&mut self) -> Result<Vec<u8>, String> {
        let world = TypstServerWorld::new(self);

        if !self.body.is_some() {
            return Err(String::from("No body for typst compiler"));
        }

        let warned_output = typst::compile(&world);
        let output = warned_output.output;
        match output {
            Ok(document) => {
                match typst_pdf::pdf(&document, &PdfOptions::default()) {
                    Ok(buffer) => Ok(buffer.into_iter().collect::<Vec<u8>>()),
                    Err(errors) => {
                        Err(get_error_message(&world, &self.body.as_ref().unwrap(), &errors))
                    }
                }
            }
            Err(errors) => {
                Err(get_error_message(&world, &self.body.as_ref().unwrap(), &errors))
            }
        }
    }
}
