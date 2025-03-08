# typst-server

A simple http frontend for the typst compiler.

## Usage

Typst templates are stored in the templates directory. Set the template through the `template` GET parameter. JSON data can be passed to the template as the POST body, which is acceesible to the template through the `post` variable.

```typst
// templates/example.typ

#set text(font: "Open Sans")

Hello #{
    if "name" in post { post.name }
    else { "World" }
}!
```

Generating a PDF from the template using a GET request.

```http
GET /?template=example

<<<

Hello World!
```

Generating a PDF from the template using a POST request.

```http
POST /?template=example

{"name":"John"}

<<<

Hello John!
```