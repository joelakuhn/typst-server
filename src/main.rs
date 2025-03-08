mod typst_compiler;
use typst_compiler::Typst;

use std::{fs::File, io::Read, path::Path, vec};

use rocket::{form::validate::Contains, http::{ContentType, Status}};

#[macro_use] extern crate rocket;

fn text_response(status : Status, message : &str) -> (Status, (ContentType, Vec<u8>)) {
    return (status, (ContentType::Text, message.as_bytes().into()));
}

fn process_request(template: Option<String>, post_body: Option<String>) -> (Status, (ContentType, Vec<u8>)) {
    if !template.is_some() {
        return text_response(Status::BadRequest, "Must specify a template.");
    }

    if template.contains("..") {
        return text_response(Status::NotAcceptable, "Template name cannot traverse the file tree.");
    }

    let mut path = String::from("");
    path.push_str("templates/");
    path.push_str(template.unwrap().as_str());
    path.push_str(".typ");
    let path = Path::new(&path);
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(_) => return text_response(Status::NotFound, "Could not locate template."),
    };
    let mut template_body = String::new();
    match file.read_to_string(&mut template_body) {
        Ok(_) => {},
        Err(_) => return text_response(Status::NotFound, "Could not read template."),
    }

    let mut typst = Typst::new(Some(template_body));

    if post_body.is_some() {
        typst.json(String::from("post"), post_body.unwrap());
    }
    else {
        typst.json(String::from("post"), String::from("{}"));
    }

    match typst.compile() {
        Ok(pdf) => (Status::Ok, (ContentType::PDF, pdf)),
        Err(error_message) => return text_response(Status::InternalServerError, &error_message),
    }
}

#[post("/?<template>", data = "<post_body>")]
fn hello_post(template: Option<String>, post_body: String) -> (Status, (ContentType, Vec<u8>)) {
    return process_request(template, Some(post_body));
}

#[get("/?<template>")]
fn hello_get(template: Option<String>) -> (Status, (ContentType, Vec<u8>)) {
    return process_request(template, None);
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![
        hello_get,
        hello_post,
    ])
}