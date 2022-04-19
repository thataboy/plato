use anyhow::{Error, format_err};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value as JsonValue;
use crate::app::Context;

const REMOVE_TAGS: &str = r#"<span.*?>|</span>|<link[^>]+>|\n+|(?s)<!--.+-->|<p class="mw-empty-elt">(\s|\n)*</p>"#;

pub struct Page {
    pub title: String,
    pub pageid: String,
    pub extract: String,
}

fn wiki_url(context: &Context) -> String {
    let server = &context.settings.wikipedia_server.trim();
    format!("{}{}w/api.php", server, if server.ends_with("/") {""} else {"/"})
}

pub fn search(query: &str, context: &Context) -> Result<Vec<Page>, Error> {
    let params = vec![
        ("action", "query"),
        ("list", "search"),
        ("srlimit", "10"),
        ("format", "json"),
        ("srsearch", query),
    ];
    let client = Client::new();

    let response = client.get(&wiki_url(context))
                         .query(&params)
                         .send()?;

    if !response.status().is_success() {
        return Err(format_err!("Unable to connect: {}", response.status()));
    }

    let body: JsonValue = response.json().unwrap();

    if let Some(results) = body.get("query").unwrap()
                               .get("search").and_then(JsonValue::as_array) {
        if results.is_empty() {
            return Err(format_err!("No results found."));
        }

        let pageids = results.iter()
                             .map(|x| x.get("pageid").and_then(JsonValue::as_u64)
                                                     .unwrap().to_string())
                             .collect::<Vec<String>>();

        if pageids.is_empty() {
            return Err(format_err!("No pages found."));
        }

        let pageids_str = pageids.join("|");
        let params = vec![
            ("action", "query"),
            ("prop", "extracts"),
            ("exintro", "1"),
            ("format", "json"),
            ("pageids", &pageids_str),
        ];

        let response = client.get(&wiki_url(context))
                             .query(&params)
                             .send()?;

        if !response.status().is_success() {
            return Err(format_err!("Failed to retrieve summaries: {}", response.status()));
        }

        let body: JsonValue = response.json().unwrap();

        if let Some(json_pages) = body.get("query").unwrap()
                                      .get("pages").and_then(JsonValue::as_object) {

            let mut pages: Vec<Page> = Vec::new();
            let re = Regex::new(REMOVE_TAGS).unwrap();
            let re2 = Regex::new(r"^<p>").unwrap();

            for pageid in pageids {
                if let Some(page) = json_pages.get(&pageid) {
                    let title = page.get("title").and_then(JsonValue::as_str).unwrap().to_string();
                    let temp = page.get("extract").and_then(JsonValue::as_str).unwrap();
                    let extract = format!("<h2 class='title'>{}</h2>{}",
                                          title,
                                          re2.replace(&re.replace_all(temp, ""), "<p class='first'>"));
                    pages.push(
                        Page {
                            title,
                            pageid,
                            extract,
                        }
                    );
                }
            }
            return Ok(pages);
        }
    }
    Err(format_err!("Unexpected value returned."))
}

pub fn fetch(pageid: &str, context: &Context) -> Result<String, Error> {
    let params = vec![
        ("action", "query"),
        ("prop", "extracts"),
        ("format", "json"),
        ("pageids", pageid),
    ];
    let client = Client::new();

    let response = client.get(&wiki_url(context))
                         .query(&params)
                         .send()?;

    if !response.status().is_success() {
        return Err(format_err!("Unable to connect: {}", response.status()));
    }

    let body: JsonValue = response.json().unwrap();
    if let Some(page) = body.get("query").unwrap()
                            .get("pages").unwrap()
                            .get(pageid).and_then(JsonValue::as_object) {
        if page.get("missing").is_some() {
            return Err(format_err!("Page not found."));
        }
        let re = Regex::new(REMOVE_TAGS).unwrap();
        let extract = page.get("extract").and_then(JsonValue::as_str).unwrap();

        let text = format!("<html><head><title>{}</title></head><body>{}</body></html>",
                           page.get("title").and_then(JsonValue::as_str).unwrap(),
                           re.replace_all(extract, ""));
        Ok(text)
    } else {
        Err(format_err!("Unexpected value returned."))
    }
}
