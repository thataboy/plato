use anyhow::{Error, format_err};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value as JsonValue;
use crate::app::Context;

pub fn wiki(query: &str, context: &Context) -> Result<(String, Vec<String>, usize), Error> {
    let params = vec![
        ("action", "query"),
        ("list", "search"),
        ("srlimit", "10"),
        ("format", "json"),
        ("srsearch", query),
    ];
    let server = &context.settings.wikipedia_server.trim();
    let url = format!("{}{}w/api.php", server, if server.ends_with("/") {""} else {"/"});
    let client = Client::new();

    let response = client.get(&url)
                         .query(&params)
                         .send()?;

    if !response.status().is_success() {
        return Err(format_err!("Unable to connect to {}: {}", server, response.status()));
    }

    let mut text = String::new();
    let mut titles: Vec<String> = Vec::new();
    let body: JsonValue = response.json().unwrap();
    let mut cnt: usize = 0;

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

        let response = client.get(&url)
                             .query(&params)
                             .send()?;

        if !response.status().is_success() {
            return Err(format_err!("Failed to retrieve summaries: {}", response.status()));
        }

        let body: JsonValue = response.json().unwrap();

        if let Some(pages) = body.get("query").unwrap()
                                 .get("pages").and_then(JsonValue::as_object) {

            let re = Regex::new(r#"<link[^>]+>|\n+|(?s)<!--.+-->|<p class="mw-empty-elt">(\s|\n)*</p>"#).unwrap();
            let re2 = Regex::new(r"^<p>").unwrap();

            text.push_str("<dl>");
            for pageid in pageids {
                if let Some(page) = pages.get(&pageid) {
                    let title = page.get("title").and_then(JsonValue::as_str).unwrap();
                    titles.push(title.to_string());
                    text.push_str(&format!("<dt class='title' id='{cnt}'>{}</dt>", title));
                    let extract = page.get("extract").and_then(JsonValue::as_str).unwrap();
                    text.push_str(&format!("<dd class='extract'>{}</dd>",
                        &re2.replace(&re.replace_all(extract, ""), "<p class='first'>")));
                    cnt += 1;
                }
            }
            text.push_str("</dl>");

        }
    }
    Ok((text, titles, cnt))
}