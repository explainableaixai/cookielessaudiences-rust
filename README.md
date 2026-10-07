# cookielessaudiences, a Rust client

Async Rust access to cookieless audience profiles and IAB categories for any page URL. No cookies, no identifiers, no personal data.

Returns `serde_json::Value`, so you decide how much of the response to model.

## Cargo

```toml
[dependencies]
cookielessaudiences = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

TLS is rustls, so there is no OpenSSL to link.

## Smallest working program

```rust
use cookielessaudiences::Client;

#[tokio::main]
async fn main() -> Result<(), cookielessaudiences::Error> {
    let client = Client::new(std::env::var("COOKIELESS_KEY").unwrap());
    let page = client.segment("https://example.com/blog").await?;

    println!("type: {}", page["audience_type"]);
    println!("age:  {}", page["demographics"]["age_bracket"]);
    Ok(())
}
```

## What is on the client

| Function | Purpose |
|---|---|
| `Client::new(key)` | builds a client with a 120 second timeout |
| `segment(url)` | structured v2 audience profile |
| `segment_legacy(url)` | the older free-text shape |
| `categorize(url)` | IAB v3 and v2 categories with confidence |
| `categorize_text(text)` | IAB categories for plain text |
| `Client::vocabularies()` | all allowed values, no key |
| `labels_for(&value)` | readable names for `INT.*` and `PI.*` codes |

## The error type

```rust
use cookielessaudiences::Error;

match client.segment(url).await {
    Ok(v) => println!("{v}"),
    Err(Error::Api { status: 403, .. }) => eprintln!("credits used up or key inactive"),
    Err(Error::Api { status, message, .. }) => eprintln!("{status}: {message}"),
    Err(Error::Transport(e)) => eprintln!("network problem: {e}"),
}
```

`Error::Api` keeps the whole response body, which is handy when you want to log the exact payload a support ticket needs.

## Fan out with a bounded stream

Cargo gains `futures = "0.3"` for this one.

```rust
use futures::{stream, StreamExt};

let urls = vec!["https://a.example", "https://b.example", "https://c.example"];
let client = std::sync::Arc::new(client);

let results: Vec<_> = stream::iter(urls)
    .map(|u| {
        let c = client.clone();
        async move { (u, c.segment(u).await) }
    })
    .buffer_unordered(8)
    .collect()
    .await;

for (url, res) in results {
    match res {
        Ok(v) => println!("{url}: {}", v["audience_type"]),
        Err(e) => println!("{url}: {e}"),
    }
}
```

Eight at a time is gentle. The service tolerates far more when you need throughput.

## Use it for brand safety and fit

Teams that check whether a publisher matches an advertiser read the same fields. Combine `content_context` with `interests` and you have the raw material for [brand fit analysis](https://www.cookielessaudiences.com/use-cases/brand-fit-analysis.php) without profiling a single person.

## Use it for IAB packaging

`categorize()` returns IAB v3 with 703 categories and v2 with 698 categories. Read the [IAB audience taxonomy 1.1](https://www.cookielessaudiences.com/features/iab-audience-taxonomy.php) page to see how interests and purchase intent line up with the standard.

## Pitfalls

- The audience endpoint takes form-encoded POST, not JSON. The client handles that.
- Subdomains with thin content return status 410 unless you pass a root domain yourself.
- Status lives in the body. Do not rely on the HTTP code.
- Do not call from a browser or mobile app; keep the key on a server.

<!--expanded-->
## Design notes for the crate

The crate has one job: send form encoded requests to the Cookieless Audiences API and hand back JSON you can trust. It deliberately stops there. No response structs, no builder with twenty options, no retry policy hidden inside. Those choices belong to your application, and a thin client gives you room to make them.

The status of every call lives inside the JSON body. A call that returns HTTP 200 can still carry `status: 403`. The client reads that field and turns anything other than 200 into `Error::Api`, so your match arms can reason about outcomes instead of transport details.

The crate uses rustls for TLS. That keeps cross compilation simple and avoids a system OpenSSL dependency in containers. If you build for musl targets or for minimal images, that choice removes a whole category of build problems.

## A typed layer on top of the raw value

You may want structs for the fields you rely on. Add `serde` to your own crate and deserialize from the `Value` the client returns:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Audience {
    audience_type: Option<String>,
    demographics: Option<Demographics>,
    interests: Option<Interests>,
}

#[derive(Debug, Deserialize)]
struct Demographics {
    age_bracket: Option<Vec<String>>,
    income_level: Option<String>,
    confidence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Interests {
    tier1: Option<Vec<String>>,
}

let page = client.segment("https://example.com").await?;
let typed: Audience = serde_json::from_value(page)?;
```

Mark every field optional. A block without evidence comes back empty, and an optional field models that honestly.

## Handling the vocabularies

Vocabularies are public and need no key. Fetch them at startup, build a set of valid codes and validate everything you store:

```rust
use std::collections::HashSet;

let vocab = Client::vocabularies().await?;
let mut valid: HashSet<String> = HashSet::new();
for group in ["interests", "purchase_intent"] {
    if let Some(obj) = vocab.get(group).and_then(|v| v.as_object()) {
        for key in obj.keys() { valid.insert(key.clone()); }
    }
}
```

The enumerated lists are the contract. Eight age brackets, a five point gender skew, six income bands, seven education levels and fourteen life stages never change shape within a vocabulary version, and the interest and intent codes are stable identifiers. The [IAB audience taxonomy 1.1 feature page](https://www.cookielessaudiences.com/features/iab-audience-taxonomy.php) describes how the three branches of the standard line up with the codes.

## Running many requests politely

A tokio runtime makes it easy to start thousands of futures at once. Resist the temptation. The service accepts up to 50 parallel threads by default, and your own limit should sit well below what your plan allows.

```rust
use tokio::sync::Semaphore;
use std::sync::Arc;

let gate = Arc::new(Semaphore::new(16));
let mut tasks = Vec::new();
for url in urls {
    let permit = gate.clone().acquire_owned().await.unwrap();
    let client = client.clone();
    tasks.push(tokio::spawn(async move {
        let out = client.segment(&url).await;
        drop(permit);
        (url, out)
    }));
}
```

`Client` holds a `reqwest::Client` internally. Wrap it in an `Arc` as shown or derive `Clone` in your own wrapper so tasks share one connection pool.

## Command line tool in thirty lines

A small binary is a good way to test a key and explore pages from a terminal:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let key = std::env::var("COOKIELESS_KEY")?;
    let client = cookielessaudiences::Client::new(key);
    for url in args.by_ref() {
        match client.segment(&url).await {
            Ok(v) => println!("{url}\t{}\t{}", v["audience_type"], v["demographics"]["income_level"]),
            Err(e) => eprintln!("{url}\t{e}"),
        }
    }
    Ok(())
}
```

Tab separated output pipes well into `sort`, `cut` and spreadsheet imports.

## Where Rust services use this data

Ad servers, curation tools and data pipelines written in Rust often need a quick audience label for a domain list. Typical jobs include tagging an inventory package, ranking candidate sites against a persona, and writing coded attributes into a columnar file. The [seller defined audiences guide](https://www.cookielessaudiences.com/features/seller-defined-audiences.php) is worth reading before you design a tagging service, because it describes how segment identifiers travel in bid requests without exposing a user.

If your service also deals with people data, the [job board resume import page](https://www.resumereaderapi.com/use-cases/job-boards.php) shows how a one tap upload becomes a structured profile. For teams who build deal sourcing tools, the [Acquisition Universe FAQ](https://www.acquisitionuniverse.com/faq.php) answers the practical questions about coverage and freshness that come up when you replace a purchased list with a screened one.

## Error handling checklist

- Retry transport errors with exponential backoff and a small cap.
- Never retry `Error::Api` with status 400, 401 or 407. The request is wrong, and retrying will not fix it.
- Do not retry 403. Alert someone instead.
- Retry 411 once. Pages sometimes fail to load for a moment.
- Log the body of every `Error::Api`. It carries the explanation you need for a support request.

## Versioning

The crate follows semantic versioning. The 0.x series may adjust the public surface as the API grows. Pin `cookielessaudiences = "0.1"` and read the changelog before upgrading.

## Testing the crate and your code

The crate ships a unit test for the label resolver and a doc test for the main call. For your own code, put the client behind a small trait so tests can substitute a fake:

```rust
#[async_trait::async_trait]
trait Segmenter {
    async fn segment(&self, url: &str) -> Result<serde_json::Value, cookielessaudiences::Error>;
}
```

A fake that returns canned JSON keeps your test suite fast and free of network access. Reserve one live test for CI, gated behind an environment variable, that calls the public vocabularies endpoint. It needs no key, so it never costs credits, and it will tell you quickly if the service moved a field.

Write a property test for your own mapping code. Generate random combinations of age brackets and income bands from the vocabulary lists and check that your reducer never panics. Because the vocabularies are closed lists, exhaustive and randomized testing are both practical, and they catch the edge cases that ordinary examples miss.

<!--further-->
## Further reading and practical notes

Rust newcomers who want more background on the async patterns used above will find the [official Rust site](https://www.rust-lang.org/) a good starting point, and [The Rust Programming Language book](https://doc.rust-lang.org/book/) explains ownership, error handling with `Result` and the module system in plain language.

Two small habits make a crate like this one easier to live with. First, keep the client in one place. Build it at startup, wrap it in an `Arc` and pass it to the parts of your program that need it, instead of constructing a new client for every call. A new client means a new connection pool, and a pool is what makes repeated calls fast. Second, treat the JSON value as a boundary. Convert it into your own types as early as you can, and let the rest of your program work with those types. The day the service adds a field, nothing in your program will need to change, because the conversion function is the only place that reads raw keys.

## Questions

**Why `Value` and not structs?** The response gains fields as vocabularies grow. A `Value` keeps your build green.

**Blocking version?** Run the future with `tokio::runtime::Runtime::block_on`.

**License?** MIT.

Contact: info@alpha-quantum.com
