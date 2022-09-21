use anyhow::{Error, format_err};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value as JsonValue;

const REMOVE_TAGS: &str = r#"<span.*?>|</span>|<link[^>]+>|\n+|(?s)<!--.+-->|<p class="mw-empty-elt">(\s|\n)*</p>"#;

pub struct WikiPage {
    pub title: String,
    pub pageid: String,
    pub extract: String,
}

fn wiki_url(lang: &str) -> String {
    format!("https://{}.wikipedia.org/w/api.php", lang)
}

pub fn search(query: &str, lang: &str) -> Result<Vec<WikiPage>, Error> {
    let params = vec![
        ("action", "query"),
        ("list", "search"),
        ("srlimit", "10"),
        ("format", "json"),
        ("srsearch", query),
    ];
    let url = wiki_url(lang);
    let client = Client::new();

    let response = client.get(&url)
                         .query(&params)
                         .send()?;

    if !response.status().is_success() {
        return Err(format_err!("Unable to connect to {}: {}", url, response.status()));
    }

    let body: JsonValue = response.json()?;

    if let Some(results) = body.get("query")
                               .and_then(|x| x.get("search").and_then(JsonValue::as_array)) {

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

        let response = client.get(&url)
                             .query(&params)
                             .send()?;

        if !response.status().is_success() {
            return Err(format_err!("Failed to retrieve summaries: {}", response.status()));
        }

        let body: JsonValue = response.json()?;

        if let Some(pages) = body.get("query")
                                 .and_then(|x| x.get("pages").and_then(JsonValue::as_object)) {

            let mut results: Vec<WikiPage> = Vec::new();
            let re = Regex::new(REMOVE_TAGS).unwrap();
            let re2 = Regex::new(r"^<p>").unwrap();

            for pageid in pageids {
                if let Some(page) = pages.get(&pageid) {
                    let title = page.get("title").and_then(JsonValue::as_str).unwrap_or_default().to_string();
                    let extract = page.get("extract").and_then(JsonValue::as_str).unwrap_or_default();
                    let extract = format!("<h2 class='title'>{}</h2>{}",
                                          title,
                                          re2.replace(&re.replace_all(extract, ""), "<p class='first'>"));
                    results.push(
                        WikiPage {
                            title,
                            pageid,
                            extract,
                        }
                    );
                }
            }
            return Ok(results);
        }
    }
    Err(format_err!("Unexpected value returned."))
}

pub fn fetch(pageid: &str, lang: &str) -> Result<String, Error> {
    let params = vec![
        ("action", "query"),
        ("prop", "extracts"),
        ("format", "json"),
        ("pageids", pageid),
    ];
    let url = wiki_url(lang);
    let client = Client::new();

    let response = client.get(&url)
                         .query(&params)
                         .send()?;

    if !response.status().is_success() {
        return Err(format_err!("Unable to connect to {}: {}", url, response.status()));
    }

    let body: JsonValue = response.json()?;
    if let Some(page) = body.get("query")
                            .and_then(|x| x.get("pages"))
                            .and_then(|x| x.get(&pageid)) {
        if let Some(text) = page.get("extract").and_then(JsonValue::as_str) {
            let re = Regex::new(REMOVE_TAGS).unwrap();
            let html = format!("<html><head><title>{}</title>\n\
                                <meta name='author' content='Wikipedia' />\n\
                                </head><body>{}</body></html>",
                               page.get("title").and_then(JsonValue::as_str).unwrap_or_default(),
                               re.replace_all(text, ""));
            Ok(html)
        } else {
            Err(format_err!("Unexpected value returned."))
        }
    } else {
        Err(format_err!("Page not found."))
    }
}

/*
Sample wikipedia search session

// search

curl "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch=rust&format=json&srlimit=2"

{
    "batchcomplete": "",
    "continue": {
        "sroffset": 2,
        "continue": "-||"
    },
    "query": {
        "searchinfo": {
            "totalhits": 15084,
            "suggestion": "rush",
            "suggestionsnippet": "rush"
        },
        "search": [
            {
                "ns": 0,
                "title": "Rust",
                "pageid": 26477,
                "size": 26202,
                "wordcount": 2836,
                "snippet": "<span class=\"searchmatch\">Rust</span> is an iron oxide, a usually reddish-brown oxide formed by the reaction of iron and oxygen in the catalytic presence of water or air moisture. Rust",
                "timestamp": "2022-04-18T15:59:04Z"
            },
            {
                "ns": 0,
                "title": "Rust (programming language)",
                "pageid": 29414838,
                "size": 71968,
                "wordcount": 5653,
                "snippet": "<span class=\"searchmatch\">Rust</span> is a multi-paradigm, general-purpose programming language designed for performance and safety, especially safe concurrency. It is syntactically similar",
                "timestamp": "2022-04-14T06:39:54Z"
            }
        ]
    }
}

// fetch full article

curl "https://en.wikipedia.org/w/api.php?action=query&prop=extracts&pageids=29414838&format=json"
{
    "batchcomplete": "",
    "warnings": {
        "extracts": {
            "*": "HTML may be malformed and/or unbalanced and may omit inline images. Use at your own risk. Known problems are listed at https://www.mediawiki.org/wiki/Special:MyLanguage/Extension:TextExtracts#Caveats."
        }
    },
    "query": {
        "pages": {
            "29414838": {
                "pageid": 29414838,
                "ns": 0,
                "title": "Rust (programming language)",
                "extract": "<p class=\"mw-empty-elt\">\n</p>\n<p><b>Rust</b> is a multi-paradigm, general-purpose programming language designed for performance and safety, [[...snipped...]]"
            }
        }
    }
}
*/