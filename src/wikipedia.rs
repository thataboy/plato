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
    let url = wiki_url(context);
    let client = Client::new();

    let response = client.get(&url)
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

        let response = client.get(&url)
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
                "extract": "<p class=\"mw-empty-elt\">\n</p>\n<p><b>Rust</b> is a multi-paradigm, general-purpose programming language designed for performance and safety, especially safe concurrency. It is syntactically similar to C++, but can guarantee memory safety by using a <i>borrow checker</i> to validate references. It achieves memory safety without garbage collection, and reference counting is optional. It has been called a systems programming language, and in addition to high-level features such as functional programming it also offers mechanisms for low-level memory management.\n</p><p>First appearing in 2010, Rust was designed by Graydon Hoare at Mozilla Research with contributions from Dave Herman, Brendan Eich, and others. The designers refined the language while writing the Servo experimental browser engine and the Rust compiler. Rust's major influences include C++, OCaml, Haskell, and Erlang. It has gained increasing use and investment in industry, by companies including Amazon, Discord, Dropbox, Facebook (Meta), Google (Alphabet), and Microsoft.\n</p><p>Rust has been voted the \"most loved programming language\" in the Stack Overflow Developer Survey every year since 2016, and was used by 7% of the respondents in 2021.</p>\n\n\n<h2><span id=\"History\">History</span></h2>\n\n<p>The language grew out of a personal project begun in 2006 by Mozilla employee Graydon Hoare. Hoare has stated that the project was possibly named after rust fungi and that the name is also a subsequence of \"robust\". Mozilla began sponsoring the project in 2009 and announced it in 2010. The same year, work shifted from the initial compiler (written in OCaml) to an LLVM-based self-hosting compiler written in Rust.  Named <span>rustc</span>, it successfully compiled itself in 2011.</p><p>The first numbered pre-alpha release of the Rust compiler occurred in January 2012. Rust 1.0, the first stable release, was released on May 15, 2015. Following 1.0, stable point releases are delivered every six weeks, while features are developed in nightly Rust with daily releases, then tested with beta releases that last six weeks. Every two to three years, a new \"edition\" is produced. This is to provide an easy reference point for changes due to the frequent nature of Rust's <i>train release schedule,</i> and to provide a window to make limited breaking changes. Editions are largely compatible.</p><p>Along with conventional static typing, before version 0.4, Rust also supported typestates. The typestate system modeled assertions before and after program statements through use of a special <code>check</code> statement. Discrepancies could be discovered at compile time rather than at runtime as with assertions in C or C++ code. The typestate concept was not unique to Rust, as it was introduced in the NIL language. Typestates were removed because they were little used, though the same functionality can be achieved by leveraging Rust's move semantics.</p><p>The object system style changed considerably within versions 0.2, 0.3, and 0.4 of Rust. Version 0.2 introduced classes for the first time, and version 0.3 added several features, including destructors and polymorphism through the use of interfaces. In Rust 0.4, traits were added as a means to provide inheritance; interfaces were unified with traits and removed as a separate feature. Classes were also removed and replaced by a combination of implementations and structured types.</p><p>Starting in Rust 0.9 and ending in Rust 0.11, Rust had two built-in pointer types: <code>~</code> and <code>@</code>, simplifying the core memory model. It reimplemented those pointer types in the standard library as <code>Box</code> and (the now removed) <code>Gc</code>.\n</p><p>In January 2014, before the first stable release, Rust 1.0, the editor-in-chief of <i>Dr. Dobb's</i>, Andrew Binstock, commented on Rust's chances of becoming a competitor to C++ and to the other up-and-coming languages D, Go, and Nim (then Nimrod). According to Binstock, while Rust was \"widely viewed as a remarkably elegant language\", adoption slowed because it repeatedly changed between versions.</p><p>In August 2020, Mozilla laid off 250 of its 1,000 employees worldwide as part of a corporate restructuring caused by the long-term impact of the COVID-19 pandemic. Among those laid off were most of the Rust team, while the Servo team was completely disbanded. The event raised concerns about the future of Rust.</p><p>In the following week, the Rust Core Team acknowledged the severe impact of the layoffs and announced that plans for a Rust foundation were underway. The first goal of the foundation would be taking ownership of all trademarks and domain names, and also take financial responsibility for their costs.</p><p>On February 8, 2021 the formation of the Rust Foundation was officially announced by its five founding companies (AWS, Huawei, Google, Microsoft, and Mozilla).</p><p>On April 6, 2021, Google announced support for Rust within Android Open Source Project as an alternative to C/C++.</p>\n<h2><span id=\"Syntax\">Syntax</span></h2><p>\nHere is a \"Hello, World!\" program written in Rust. The <code>println!</code> macro prints the message to standard output.</p>\n<p>The syntax of Rust is similar to C and C++, with blocks of code delimited by curly brackets, and control flow keywords such as <code>if</code>, <code>else</code>, <code>while</code>, and <code>for</code>, although the specific syntax for defining functions is more similar to Pascal. Despite the syntactic resemblance to C and C++, the semantics of Rust are closer to that of the ML family of languages and the Haskell language. Nearly every part of a function body is an expression, even control flow operators. For example, the ordinary <code>if</code> expression also takes the place of C's ternary conditional, an idiom used by ALGOL 60. As in Lisp, a function need not end with a <code>return</code> expression: in this case if the semicolon is omitted, the last expression in the function creates the return value, as seen in the following recursive implementation of the factorial function:\n</p>\n\n<p>The following iterative implementation uses the <code>..=</code> operator to create an inclusive range:\n</p>\n\n<p>More advanced features in Rust include the use of generic functions to achieve type polymorphism. The following is a Rust program to calculate the sum of two things, for which addition is implemented, using a generic function:\n</p>\n\n<p>Unlike other languages, Rust does not use null pointers to indicate a lack of data, as doing so can lead to accidental dereferencing. Therefore, in order to uphold its safety guarantees, null pointers cannot be dereferenced unless explicitly declaring the code block unsafe with an <code>unsafe</code> block. Rust instead uses a Haskell-like <code>Option</code> type, which has two variants, <code>Some(T)</code> (which indicates that a value is present) and <code>None</code> (analogous to the null pointer). <code>Option</code> values must be handled using syntactic sugar such as the <code>if let</code> construction in order to access the inner value (in this case, a string):\n</p>\n\n<h2><span id=\"Features\">Features</span></h2>\n\n<p>Rust is intended to be a language for highly concurrent and highly safe systems, and <i>programming in the large</i>, that is, creating and maintaining boundaries that preserve large-system integrity. This has led to a feature set with an emphasis on safety, control of memory layout, and concurrency.\n</p>\n<h3><span id=\"Memory_safety\">Memory safety</span></h3>\n<p>Rust is designed to be memory safe. It does not permit null pointers, dangling pointers, or data races. Data values can be initialized only through a fixed set of forms, all of which require their inputs to be already initialized.  To replicate pointers being either valid or <code>NULL</code>, such as in linked list or binary tree data structures, the Rust core library provides an option type, which can be used to test whether a pointer has <code>Some</code> value or <code>None</code>.  Rust has added syntax to manage lifetimes, which are checked at compile time by the <i>borrow checker</i>. Unsafe code can subvert some of these restrictions using the <code>unsafe</code> keyword.</p>\n<h3><span id=\"Memory_management\">Memory management</span></h3>\n<p>Rust does not use automated garbage collection. Memory and other resources are managed through the resource acquisition is initialization convention, with optional reference counting. Rust provides deterministic management of resources, with very low overhead. Rust favors stack allocation of values and does not perform implicit boxing.\n</p><p>There is the concept of references (using the <code>&amp;</code> symbol), which does not involve run-time reference counting. The safety of such pointers is verified at compile time, preventing dangling pointers and other forms of undefined behavior. Rust's type system separates shared, immutable pointers of the form <code>&amp;T</code> from unique, mutable pointers of the form <code>&amp;mut T</code>. A mutable pointer can be coerced to an immutable pointer, but not vice versa.\n</p>\n<h3><span id=\"Ownership\">Ownership</span></h3>\n<p>Rust has an ownership system where all values have a unique owner, and the scope of the value is the same as the scope of the owner. Values can be passed by immutable reference, using <code>&amp;T</code>, by mutable reference, using <code>&amp;mut T</code>, or by value, using <code>T</code>. At all times, there can either be multiple immutable references or one mutable reference (an implicit readers–writer lock). The Rust compiler enforces these rules at compile time and also checks that all references are valid.\n</p>\n<h3><span id=\"Types_and_polymorphism\">Types and polymorphism</span></h3>\n<p>Rust's type system supports a mechanism called traits, inspired by type classes in the Haskell language. Traits annotate types and are used to define <i>shared behavior</i> between different types. For example, floats and integers both implement the <code>Add</code> trait because they can both be added; and any type that can be printed out as a string implements the <code>Display</code> or <code>Debug</code> traits. This facility is known as ad hoc polymorphism.\n</p><p>Rust uses type inference for variables declared with the keyword <code>let</code>. Such variables do not require a value to be initially assigned to determine their type. A compile time error results if any branch of code leaves the variable without an assignment. Variables assigned multiple times must be marked with the keyword <code>mut</code> (short for mutable).\n</p><p>A function can be given generic parameters, which allows the same function to be applied to different types. Generic functions can constrain the generic type to implement a particular trait or traits; for example, an \"add one\" function might require the type to implement \"Add\". This means that a generic function can be type-checked as soon as it is defined.\n</p><p>The implementation of Rust generics is similar to the typical implementation of C++ templates: a separate copy of the code is generated for each instantiation. This is called monomorphization and contrasts with the type erasure scheme typically used in Java and Haskell. Rust's type erasure is also available by using the keyword <code>dyn</code>. The benefit of monomorphization is optimized code for each specific use case; the drawback is increased compile time and size of the resulting binaries.\n</p><p>In Rust, user-defined types are created with the <code>struct</code> or <code>enum</code> keywords. These types usually contain fields of data like objects or classes in other languages. The <code>impl</code> keyword can define methods for the types (data and functions are defined separately) or implement a trait for the types. A trait is a contract that a structure has certain required methods implemented. Traits are used to restrict generic parameters and because traits can provide a struct with more methods than the user defined. For example, the trait <code>Iterator</code> requires that the <code>next</code> method be defined for the type. Once the <code>next</code> method is defined the trait provides common functional helper methods over the iterator like <code>map</code> or <code>filter</code>.\n</p><p>Type aliases, including generic arguments, can also be defined with the <code>type</code> keyword.\n</p><p>The object system within Rust is based around implementations, traits and structured types. Implementations fulfill a role similar to that of classes within other languages and are defined with the keyword <code>impl</code>. Traits provide inheritance and polymorphism; they allow methods to be defined and mixed in to implementations. Structured types are used to define fields. Implementations and traits cannot define fields themselves, and only traits can provide inheritance. Among other benefits, this prevents the diamond problem of multiple inheritance, as in C++. In other words, Rust supports interface inheritance but replaces implementation inheritance with composition; see composition over inheritance.\n</p>\n<h3><span id=\"Macros_for_language_extension\">Macros for language extension</span></h3>\n<p>It is possible to extend the Rust language using the procedural macro mechanism.</p><p>Procedural macros use Rust functions that run at compile time to modify the compiler's token stream. This complements the declarative macro mechanism (also known as <i>macros by example</i>), which uses pattern matching to achieve similar goals.\n</p><p>Procedural macros come in three flavors:\n</p>\n<ul><li>Function-like macros <code>custom!(...)</code></li>\n<li>Derive macros <code>#[derive(CustomDerive)]</code></li>\n<li>Attribute macros <code>#[custom_attribute]</code></li></ul><p>The <code>println!</code> macro is an example of a function-like macro and <code>serde_derive</code> is a commonly used library for generating code\nfor reading and writing data in many formats such as JSON. Attribute macros are commonly used for language bindings such as the <code>extendr</code> library for Rust bindings to R.</p><p>The following code shows the use of the <code>Serialize</code>, <code>Deserialize</code> and <code>Debug</code> derive procedural macros\nto implement JSON reading and writing as well as the ability to format a structure for debugging.\n</p>\n\n<h3><span id=\"Interface_with_C_and_C.2B.2B\"></span><span id=\"Interface_with_C_and_C++\">Interface with C and C++</span></h3>\n<p>Rust has a foreign function interface (FFI) that can be used both to call code written in languages such as C from Rust and to call Rust code from those languages. While calling C++ has historically been challenging (from any language), Rust has a library, CXX, to allow calling to or from C++, and \"CXX has zero or negligible overhead.\"</p>\n<h2><span id=\"Components\">Components</span></h2>\n<p>Besides the compiler and standard library, the Rust ecosystem includes additional components for software development. Component installation is typically managed by <i>rustup,</i> a Rust toolchain installer developed by the Rust project.</p>\n<h3><span id=\"Cargo\">Cargo</span></h3>\n<p>Cargo is Rust's build system and package manager. Cargo downloads, compiles, distributes, and uploads packages, called <i>crates</i>, maintained in the official registry. Cargo also wraps Clippy and other Rust components.\n</p><p>Cargo requires projects to follow a certain directory structure, with some flexibility. Projects using Cargo may be either a single crate or a <i>workspace</i> composed of multiple crates that may depend on each other.</p><p>The dependencies for a crate are specified in a <i>Cargo.toml</i> file along with SemVer version requirements, telling Cargo which versions of the dependency are compatible with the crate using them. By default, Cargo sources its dependencies from the user-contributed registry <i>crates.io</i>, but Git repositories and crates in the local filesystem can be specified as dependencies, too.</p>\n<h3><span id=\"Rustfmt\">Rustfmt</span></h3>\n<p>Rustfmt is a code formatter for Rust. It takes Rust source code as input and changes the whitespace and indentation to produce code formatted in accordance to the Rust style guide or rules specified in a <i>rustfmt.toml</i> file. Rustfmt can be invoked as a standalone program or on a Rust project through Cargo.</p>\n<h3><span id=\"Clippy\">Clippy</span></h3>\n<p>Clippy is Rust's built-in linting tool to improve the correctness, performance, and readability of Rust code. It was created in 2014 and named after the eponymous Microsoft Office feature. As of 2021, Clippy has more than 450 rules, which can be browsed online and filtered by category. Some rules are disabled by default.\n</p>\n<h3><span id=\"IDE_support\">IDE support</span></h3>\n<p>The most popular language servers for Rust are <i>rust-analyzer</i> and <i>RLS</i>. These projects provide IDEs and text editors with more information about a Rust project.\nBasic features include linting checks via Clippy and formatting via Rustfmt, among other functions. RLS also provides automatic code completion via <i>Racer</i>, though development of Racer was slowed down in favor of rust-analyzer.</p>\n<h2><span id=\"Performance\">Performance</span></h2>\n<p>Rust aims \"to be as efficient and portable as idiomatic C++, without sacrificing safety\". Since Rust utilizes LLVM, any performance improvements in LLVM also carry over to Rust.</p>\n<h2><span id=\"Adoption\">Adoption</span></h2>\n\n<p><br>\nRust has been adopted by major software engineering companies. For example, Dropbox is now written in Rust, as are some components at Amazon, Microsoft, Facebook, Discord,\nand the Mozilla Foundation. Rust was the third-most-loved programming language in the 2015 Stack Overflow annual survey and took first place for 2016–2021.</p>\n<h3><span id=\"Web_browsers_and_services\">Web browsers and services</span></h3>\n<ul><li>Firefox has two projects written in Rust: the Servo parallel browser engine developed by Mozilla in collaboration with Samsung; and Quantum, which is composed of several sub-projects for improving Mozilla's Gecko browser engine.</li>\n<li>OpenDNS uses Rust in two of its components.</li>\n<li>Deno, a secure runtime for JavaScript and TypeScript, is built with V8, Rust, and Tokio.</li>\n<li>Figma, a web-based vector graphics editor, is written in Rust.</li></ul><h3><span id=\"Operating_systems\">Operating systems</span></h3>\n<ul><li>Redox is a \"full-blown Unix-like operating system\" including a microkernel written in Rust.</li>\n<li>Theseus, an experimental OS with \"intralingual design\", is written in Rust.</li>\n<li>The Google Fuchsia capability-based operating system has some tools written in Rust.</li>\n<li>Stratis is a file system manager written in Rust for Fedora and RHEL 8.</li>\n<li>exa is a Unix/Linux command line alternative to ls written in Rust.</li>\n<li>Since 2021, there is a patch series for adding Rust support to the Linux kernel.</li></ul><h3><span id=\"Other_notable_projects_and_platforms\">Other notable projects and platforms</span></h3>\n<ul><li>Discord uses Rust for portions of its backend, as well as client-side video encoding, to augment the core infrastructure written in Elixir.</li>\n<li>Microsoft Azure IoT Edge, a platform used to run Azure services and artificial intelligence on IoT devices, has components implemented in Rust.</li>\n<li>Polkadot (cryptocurrency) is a blockchain platform written in Rust.</li>\n<li>Ruffle is an open-source SWF emulator written in Rust.</li>\n<li>TerminusDB, an open source graph database designed for collaboratively building and curating knowledge graphs, is written in Prolog and Rust.</li>\n<li>Amazon Web Services has multiple projects written in Rust, including Firecracker, a virtualization solution, and Bottlerocket, a Linux distribution and containerization solution.</li></ul><h2><span id=\"Community\">Community</span></h2>\n\n<p>Rust's official website lists online forums, messaging platforms, and in-person meetups for the Rust community.\nConferences dedicated to Rust development include:\n</p>\n<ul><li>RustConf: an annual conference in Portland, Oregon. Held annually since 2016 (except in 2020 and 2021 because of the COVID-19 pandemic).</li>\n<li>Rust Belt Rust: a #rustlang conference in the Rust Belt</li>\n<li>RustFest: Europe's @rustlang conference</li>\n<li>RustCon Asia</li>\n<li>Rust LATAM</li>\n<li>Oxidize Global</li></ul><h2><span id=\"Governance\">Governance</span></h2>\n<link rel=\"mw-deduplicated-inline-style\" href=\"mw-data:TemplateStyles:r1066479718\"><p>The <b>Rust Foundation</b> is a non-profit membership organization incorporated in Delaware, United States, with the primary purposes of supporting the maintenance and development of the language, cultivating the Rust project team members and user communities, managing the technical infrastructure underlying the development of Rust, and managing and stewarding the Rust trademark.\n</p><p>It was established on February 8, 2021, with five founding corporate members (Amazon Web Services, Huawei, Google, Microsoft, and Mozilla). The foundation's board is chaired by Shane Miller. Starting in late 2021, its Executive Director and CEO is Rebecca Rumbul. Prior to this, Ashley Williams was interim executive director.</p>\n<h2><span id=\"See_also\">See also</span></h2>\n<ul><li>List of programming languages</li>\n<li>History of programming languages</li>\n<li>Comparison of programming languages</li></ul><h2><span id=\"Explanatory_notes\">Explanatory notes</span></h2>\n\n\n<h2><span id=\"References\">References</span></h2>\n<link rel=\"mw-deduplicated-inline-style\" href=\"mw-data:TemplateStyles:r1011085734\">\n<h2><span id=\"Further_reading\">Further reading</span></h2>\n<ul><li><link rel=\"mw-deduplicated-inline-style\" href=\"mw-data:TemplateStyles:r1067248974\"><cite id=\"CITEREFKlabnikNichols2019\" class=\"citation book cs1\">Klabnik, Steve; Nichols, Carol (August 12, 2019). <i>The Rust Programming Language (Covers Rust 2018)</i>. No Starch Press. ISBN <bdi>978-1-7185-0044-0</bdi>.</cite><span title=\"ctx_ver=Z39.88-2004&amp;rft_val_fmt=info%3Aofi%2Ffmt%3Akev%3Amtx%3Abook&amp;rft.genre=book&amp;rft.btitle=The+Rust+Programming+Language+%28Covers+Rust+2018%29&amp;rft.pub=No+Starch+Press&amp;rft.date=2019-08-12&amp;rft.isbn=978-1-7185-0044-0&amp;rft.aulast=Klabnik&amp;rft.aufirst=Steve&amp;rft.au=Nichols%2C+Carol&amp;rft_id=https%3A%2F%2Fbooks.google.com%2Fbooks%3Fid%3D0Vv6DwAAQBAJ&amp;rfr_id=info%3Asid%2Fen.wikipedia.org%3ARust+%28programming+language%29\"></span> (online version)</li>\n<li><link rel=\"mw-deduplicated-inline-style\" href=\"mw-data:TemplateStyles:r1067248974\"><cite id=\"CITEREFBlandyOrendorff2017\" class=\"citation book cs1\">Blandy, Jim; Orendorff, Jason (2017). <i>Programming Rust: Fast, Safe Systems Development</i>. O'Reilly Media. ISBN <bdi>978-1-4919-2728-1</bdi>.</cite><span title=\"ctx_ver=Z39.88-2004&amp;rft_val_fmt=info%3Aofi%2Ffmt%3Akev%3Amtx%3Abook&amp;rft.genre=book&amp;rft.btitle=Programming+Rust%3A+Fast%2C+Safe+Systems+Development&amp;rft.pub=O%27Reilly+Media&amp;rft.date=2017&amp;rft.isbn=978-1-4919-2728-1&amp;rft.aulast=Blandy&amp;rft.aufirst=Jim&amp;rft.au=Orendorff%2C+Jason&amp;rft_id=https%3A%2F%2Fbooks.google.com%2Fbooks%3Fid%3D1heDrgEACAAJ&amp;rfr_id=info%3Asid%2Fen.wikipedia.org%3ARust+%28programming+language%29\"></span></li></ul><h2><span id=\"External_links\">External links</span></h2>\n\n<ul><li><span><span>Official website</span></span> </li>\n<li>Rust-lang on GitHub</li></ul>"
            }
        }
    }
}
*/