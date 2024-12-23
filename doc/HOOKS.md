Hooks are defined in `Settings.toml`.

Here's an example hook, that launches the default article fetcher included in
*Plato*'s release archive:
```toml
[[libraries.hooks]]
path = "Articles"
program = "bin/article_fetcher/article_fetcher"
sort-method = "added"
first-column = "title-and-author"
```

The above chunk needs to be added after one of the `[[libraries]]` section.

`path` is the path of the directory that will trigger the hook. `program` is
the path to the executable associated with this hook. The `sort-method`,
`first-column` keys are optional. When specified, they will
override the *home*'s settings of the same name, while `path` is being
selected.

The *Toogle Select* sub-menu of the library menu can be used to trigger a hook
when there's no imported documents in `path`. Otherwise, you can just tap the
directory in the navigation bar. When the hook is triggered, the associated
`program` is executed as a background process. It will receive the library path,
directory path, wifi and online statuses (*true* or *false*) as arguments.

A fetcher can use its standard output (resp. standard input) to send events to
(resp. receive events from) *Plato*. An event is a JSON object with a required
`type` key. Events are read and written line by line, one per line.

The events that can be written to standard output are:

```
// Display a notification message.
{"type": "notify", "message": STRING}
// Add a document to the current library. `info` is the camel cased JSON version
// of the `Info` structure defined in `src/metadata.rs`.
{"type": "addDocument", "info": OBJECT}
// Remove a document from the current library.
{"type": "removeDocument", "path": STRING}
// Enable or disable the WiFi.
{"type": "setWifi", "enable": BOOL}
// Search for books inside `path` matching `query` and sort the results by `sortBy`.
{"type": "search", "path": STRING, "query": STRING, "sortBy": [STRING, BOOL]}
```

The events that can be read from standard input are:

```

// Sent in response to `search`.
// `results` is an array of *Info* objects.
{"type": "search": "results": ARRAY}
// Sent to all the fetchers when the network becomes available.
{"type": "network", "status": "up"}
```

When a directory is deselected, *Plato* will send the `SIGTERM` signal to all
the matching fetchers.
